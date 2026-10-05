// The Cray machine as the MiSTer wrapper sees it: the CPU, the dead start
// control and the core's small I/O page.
//
// The top 16 words of the 1M-word address space are an invented I/O page; a real
// CRAY-1 has no console on the CPU.  Word addresses:
//   0xFFFF0 CON_STAT   read:  bit 0 = input character waiting, bit 1 = output ready,
//                             bit 2 = console interrupt enabled, bit 3 = requested
//                      write: bit 0 = enable the console interrupt; clears a request
//   0xFFFF1 CON_DATA   read:  next input character (removes it)
//                      write: print the low 8 bits (waits while the output queue is full)
//   0xFFFF2 TEST_EXIT  write: end-of-test marker with an exit code; read: {done, code[62:0]}
//   0xFFFF3 CYCLES     read:  free-running clock counter
// Everything else in the page reads zero and ignores writes.
// With XMP = 1 memory has four million words and the page is their top 16:
// 0x3FFFF0 to 0x3FFFFF.
//
// Console interrupt: while it is enabled, a CTRL-C (code 3) arriving from the
// keyboard or the serial port requests it.  The CPU's console interrupt flag then
// sets (outside monitor mode), which returns control to the monitor program.
// The CTRL-C also goes into the input queue like any other character.
//
// Dead start: while reset is high the CPU is held.  When reset is released with
// rom_boot high, the monitor program is first copied from the ROM into memory
// from word 0, the job the maintenance control unit does on a real machine.
// With rom_boot low memory is left as it is.  The CPU then exchanges with the
// package at address 0 and runs.

module cray_system #(
	parameter XMP    = 0,
	parameter ROM_AW = 12  // the monitor ROM holds 2**ROM_AW words
) (
	input wire clk,
	input wire reset,
	input wire rom_boot, // sampled while reset is high

	// main memory
	output wire        mem_req,
	output wire        mem_we,
	output wire        mem_burst,
	output wire [21:0] mem_addr,
	output wire [63:0] mem_wdata,
	input  wire        mem_ack,
	input  wire [63:0] mem_rdata,

	// console
	output reg  [7:0] tx_data,
	output reg        tx_valid,
	input  wire       tx_ready,
	input  wire [7:0] rx_data,
	input  wire       rx_valid,
	output reg        rx_pop,
	input  wire       ctrl_c,    // a CTRL-C has entered the input queue

	output wire        mem_active,  // a memory request is pending (activity light)
	output reg         test_done,
	output reg  [62:0] test_code
);

	wire cpu_req, cpu_we, cpu_burst;
	wire [21:0] cpu_addr;
	wire [63:0] cpu_wdata;
	wire        cpu_ack;
	wire [63:0] cpu_rdata;

	reg cpu_rst;
	reg con_int_en;
	reg con_int_req;

	cray_cpu #(
		.XMP(XMP)
	) cpu (
		.clk          (clk),
		.rst          (cpu_rst),
		.i_single_step(1'b0),
		.i_mcu_int    (con_int_req),
		.i_io_clear   (1'b0),

		.o_mem_req  (cpu_req),
		.o_mem_we   (cpu_we),
		.o_mem_burst(cpu_burst),
		.o_mem_addr (cpu_addr),
		.o_mem_wdata(cpu_wdata),
		.i_mem_ack  (cpu_ack),
		.i_mem_rdata(cpu_rdata),

		// no devices on the 6 Mbyte channels of the X-MP setting
		.i_ch_in_ready      (4'b0),
		.i_ch_in_data       (64'b0),
		.o_ch_in_resume     (),
		.i_ch_in_disconnect (4'b0),
		.o_ch_out_ready     (),
		.o_ch_out_data      (),
		.i_ch_out_resume    (4'b0),
		.o_ch_out_disconnect(),
		.o_ch_out_mc        ()
	);

	// ---- dead start ----
	localparam DS_HOLD = 2'd0, DS_READ = 2'd1, DS_WRITE = 2'd2, DS_RUN = 2'd3;

	reg  [       1:0] ds_state;
	reg  [ROM_AW-1:0] ds_addr;
	reg               ds_boot;
	reg               ds_req;
	wire [      63:0] rom_q;

	boot_rom #(
		.AW(ROM_AW)
	) rom (
		.clk (clk),
		.addr(ds_addr),
		.q   (rom_q)
	);

	always @(posedge clk) begin
		if (reset) begin
			ds_state <= DS_HOLD;
			ds_addr  <= 0;
			ds_boot  <= rom_boot;
			ds_req   <= 0;
			cpu_rst  <= 1;
		end else
			case (ds_state)
				DS_HOLD: ds_state <= ds_boot ? DS_READ : DS_RUN;
				DS_READ: begin  // the ROM word for ds_addr is ready at the end of this clock
					ds_req   <= 1;
					ds_state <= DS_WRITE;
				end
				DS_WRITE:
				if (mem_ack) begin
					ds_req   <= 0;
					ds_addr  <= ds_addr + 1'd1;
					ds_state <= (&ds_addr) ? DS_RUN : DS_READ;
				end
				DS_RUN:  cpu_rst <= 0;
			endcase
	end

	wire ds_owner = (ds_state != DS_RUN);

	// ---- address decode ----
	localparam [17:0] IO_PAGE = XMP ? 18'h3FFFF : 18'h0FFFF;  // word addresses 0xFFFF0-0xFFFFF, on the X-MP 0x3FFFF0-0x3FFFFF

	wire io_sel = cpu_req && (cpu_addr[21:4] == IO_PAGE);

	assign mem_req    = ds_owner ? ds_req : (cpu_req && !io_sel);
	assign mem_we     = ds_owner ? 1'b1 : cpu_we;
	assign mem_burst  = ds_owner ? 1'b0 : cpu_burst;
	assign mem_addr   = ds_owner ? {{(22 - ROM_AW) {1'b0}}, ds_addr} : cpu_addr;
	assign mem_wdata  = ds_owner ? rom_q : cpu_wdata;
	assign mem_active = ds_owner ? ds_req : cpu_req;

	reg        io_ack;
	reg [63:0] io_rdata;

	assign cpu_ack   = io_ack | (mem_ack & ~ds_owner);
	assign cpu_rdata = io_ack ? io_rdata : mem_rdata;

	// ---- I/O page ----
	reg [63:0] cycles;

	always @(posedge clk) begin
		io_ack <= 0;
		rx_pop <= 0;
		cycles <= cycles + 1'd1;
		if (tx_valid && tx_ready) tx_valid <= 0;
		if (con_int_en && ctrl_c) con_int_req <= 1;

		if (reset) begin
			tx_valid    <= 0;
			test_done   <= 0;
			test_code   <= 0;
			cycles      <= 0;
			con_int_en  <= 0;
			con_int_req <= 0;
		end else if (io_sel && !io_ack) begin
			io_ack   <= 1;
			io_rdata <= 64'd0;
			case (cpu_addr[3:0])
				4'h0: begin
					if (cpu_we) begin
						con_int_en  <= cpu_wdata[0];
						con_int_req <= 0;
					end else io_rdata <= {60'd0, con_int_req, con_int_en, tx_ready & ~tx_valid, rx_valid};
				end
				4'h1: begin
					if (cpu_we) begin
						// hold the store until the output queue has room
						if (tx_valid || !tx_ready) io_ack <= 0;
						else begin
							tx_data  <= cpu_wdata[7:0];
							tx_valid <= 1;
						end
					end else begin
						io_rdata <= {56'd0, rx_valid ? rx_data : 8'd0};
						rx_pop   <= rx_valid;
					end
				end
				4'h2: begin
					if (cpu_we) begin
						test_done <= 1;
						test_code <= cpu_wdata[62:0];
					end else io_rdata <= {test_done, test_code};
				end
				4'h3:    io_rdata <= cycles;
				default: ;
			endcase
		end
	end

endmodule

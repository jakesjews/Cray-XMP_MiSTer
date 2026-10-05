// Cray CPU: instruction buffers, functional units and the memory multiplexer.
//
// XMP = 0 builds the CRAY-1 of 1982.  XMP = 1 adds what a one-processor X-MP has
// that the operating system COS needs, with the 6 Mbyte channel pairs 10 to 17
// octal (xmp_channels.v); docs/CPU.md lists what each setting does.
//
// Memory port: req, we, burst, addr and wdata are held until the last ack of a
// request.  ack is a one-clock pulse per word with rdata valid in that clock.
// burst is a 16-word read with addr[3:0] = 0.  Bit 63 is Cray bit 0, so parcel 0
// of a word is rdata[63:48].

module cray_cpu #(
	parameter XMP = 0
) (
	input wire clk,
	input wire rst,  // released to dead start from the exchange package at address 0
	input wire i_single_step,  // issue one instruction at a time (test aid)
	input wire i_mcu_int,  // request of the maintenance control unit: sets the MCU interrupt flag outside monitor mode
	input wire i_io_clear,  // X-MP: I/O Master Clear, which stops the 6 Mbyte channels

	output wire        o_mem_req,
	output wire        o_mem_we,
	output wire        o_mem_burst,
	output wire [21:0] o_mem_addr,
	output wire [63:0] o_mem_wdata,
	input  wire        i_mem_ack,
	input  wire [63:0] i_mem_rdata,

	// X-MP: the devices of the 6 Mbyte channels.  Element n is the pair of input
	// channel 10 + 2n and output channel 11 + 2n octal.  Ready, Resume and
	// Disconnect are one-clock pulses; data stays until the Resume that answers it.
	input  wire [ 3:0] i_ch_in_ready,
	input  wire [63:0] i_ch_in_data,
	output wire [ 3:0] o_ch_in_resume,
	input  wire [ 3:0] i_ch_in_disconnect,
	output wire [ 3:0] o_ch_out_ready,
	output wire [63:0] o_ch_out_data,
	input  wire [ 3:0] i_ch_out_resume,
	output wire [ 3:0] o_ch_out_disconnect,
	output wire [ 3:0] o_ch_out_mc           // Master Clear lines of the output channels
);

	// instruction buffer signals
	wire        instr_buf_mem_ce;
	wire        instr_buf_mem_burst;
	wire [21:0] instr_buf_mem_addr;
	wire        instr_buf_mem_vld;

	// functional unit signals
	wire [21:0] fu_mem_addr;
	wire [63:0] fu_mem_wr_data;
	wire        fu_mem_wr_en;
	wire        fu_mem_ce;
	wire        fu_mem_burst;
	wire        fu_mem_seq;
	wire        fu_mem_ack;

	wire [63:0] mem_read_data;

	wire [23:0] p_addr;
	wire [15:0] nip_nxt;
	wire [63:0] word_nxt;
	wire        nip_vld;
	wire        clear_ibufs;
	wire ibuf_busy, ibuf_hold;

	// 6 Mbyte channels: orders from the program and what 033 reads
	wire ch_set_ca, ch_set_cl, ch_clear, ch_k1;
	wire [ 2:0] ch_num;
	wire [21:0] ch_addr;
	wire [21:0] ch_ca;
	wire        ch_err;
	wire [ 3:0] ch_int_num;
	wire        ch_int;

	//////////////////////////////////////////////////
	//               Memory multiplexer             //
	//////////////////////////////////////////////////
	// requester 0: instruction buffers, 1: memory functional unit,
	// 2 (X-MP): the 6 Mbyte channels

	localparam NM = (XMP != 0) ? 3 : 2;

	wire [   NM-1:0] mux_req;
	wire [   NM-1:0] mux_we;
	wire [   NM-1:0] mux_burst;
	wire [   NM-1:0] mux_seq;
	wire [   NM-1:0] mux_take;
	wire [NM*22-1:0] mux_addr;
	wire [NM*64-1:0] mux_wdata;
	wire [   NM-1:0] mux_ack;

	assign mux_req[1:0]     = {fu_mem_ce, instr_buf_mem_ce};
	assign mux_we[1:0]      = {fu_mem_wr_en, 1'b0};
	assign mux_burst[1:0]   = {fu_mem_burst, instr_buf_mem_burst};
	assign mux_seq[1:0]     = {fu_mem_seq, 1'b0};
	assign mux_addr[43:0]   = {fu_mem_addr, instr_buf_mem_addr};
	assign mux_wdata[127:0] = {fu_mem_wr_data, 64'b0};

	cray_mem_mux #(
		.N(NM)
	) mem_mux (
		.clk(clk),
		.rst(rst),

		.i_req  (mux_req),
		.i_we   (mux_we),
		.i_burst(mux_burst),
		.i_seq  (mux_seq),
		.o_take (mux_take),
		.i_addr (mux_addr),
		.i_wdata(mux_wdata),
		.o_ack  (mux_ack),
		.o_rdata(mem_read_data),

		.o_mem_req  (o_mem_req),
		.o_mem_we   (o_mem_we),
		.o_mem_burst(o_mem_burst),
		.o_mem_addr (o_mem_addr),
		.o_mem_wdata(o_mem_wdata),
		.i_mem_ack  (i_mem_ack),
		.i_mem_rdata(i_mem_rdata)
	);

	assign instr_buf_mem_vld = mux_ack[0];
	assign fu_mem_ack        = mux_ack[1];

	//////////////////////////////////////////////////
	//             Instruction buffers              //
	//////////////////////////////////////////////////

	i_buf instr_buf (
		.clk        (clk),
		.rst        (rst || clear_ibufs),
		.i_p_addr   (p_addr),
		.o_nip_nxt  (nip_nxt),
		.o_word_nxt (word_nxt),
		.o_nip_vld  (nip_vld),
		.o_mem_ce   (instr_buf_mem_ce),
		.o_mem_burst(instr_buf_mem_burst),
		.o_mem_addr (instr_buf_mem_addr),
		.i_mem_data (mem_read_data),
		.i_mem_vld  (instr_buf_mem_vld),
		.i_hold     (ibuf_hold),
		.o_busy     (ibuf_busy)
	);

	//////////////////////////////////////////////////
	//       Registers and functional units         //
	//////////////////////////////////////////////////

	func_top #(
		.XMP(XMP)
	) cpu (
		.clk            (clk),
		.rst            (rst),
		// instruction buffer interface
		.i_nip_nxt      (nip_nxt),
		.i_word_nxt     (word_nxt),
		.i_nip_vld      (nip_vld),
		.o_clear_ibufs  (clear_ibufs),
		.o_p_addr       (p_addr),
		// memory interface
		.o_mem_addr     (fu_mem_addr),
		.i_data_from_mem(mem_read_data),
		.o_data_to_mem  (fu_mem_wr_data),
		.o_mem_wr_en    (fu_mem_wr_en),
		.o_mem_ce       (fu_mem_ce),
		.o_mem_burst    (fu_mem_burst),
		.o_mem_seq      (fu_mem_seq),
		.i_mem_take     (mux_take[1]),
		.i_mem_ack      (fu_mem_ack),
		.o_debug        (),
		.i_debug_full   (1'b0),
		.i_single_step  (i_single_step),
		.i_mcu_int      (i_mcu_int),
		.i_ibuf_busy    (ibuf_busy),
		.o_ibuf_hold    (ibuf_hold),
		.o_ch_set_ca    (ch_set_ca),
		.o_ch_set_cl    (ch_set_cl),
		.o_ch_clear     (ch_clear),
		.o_ch_k1        (ch_k1),
		.o_ch_num       (ch_num),
		.o_ch_addr      (ch_addr),
		.i_ch_ca        (ch_ca),
		.i_ch_err       (ch_err),
		.i_ch_int_num   (ch_int_num),
		.i_ch_int       (ch_int)
	);

	//////////////////////////////////////////////////
	//          6 Mbyte channels (X-MP)             //
	//////////////////////////////////////////////////

	generate
		if (XMP != 0) begin : g_chan
			xmp_channels channels (
				.clk       (clk),
				.rst       (rst),
				.i_io_clear(i_io_clear),

				.i_set_ca (ch_set_ca),
				.i_set_cl (ch_set_cl),
				.i_clear  (ch_clear),
				.i_k1     (ch_k1),
				.i_num    (ch_num),
				.i_addr   (ch_addr),
				.o_ca     (ch_ca),
				.o_err    (ch_err),
				.o_int_num(ch_int_num),
				.o_int    (ch_int),

				.o_mem_req  (mux_req[2]),
				.o_mem_we   (mux_we[2]),
				.o_mem_addr (mux_addr[65:44]),
				.o_mem_wdata(mux_wdata[191:128]),
				.i_mem_ack  (mux_ack[2]),
				.i_mem_rdata(mem_read_data),

				.i_in_ready      (i_ch_in_ready),
				.i_in_data       (i_ch_in_data),
				.o_in_resume     (o_ch_in_resume),
				.i_in_disconnect (i_ch_in_disconnect),
				.o_out_ready     (o_ch_out_ready),
				.o_out_data      (o_ch_out_data),
				.i_out_resume    (i_ch_out_resume),
				.o_out_disconnect(o_ch_out_disconnect),
				.o_out_mc        (o_ch_out_mc)
			);
			assign mux_burst[2] = 1'b0;
			assign mux_seq[2]   = 1'b0;
		end else begin : g_no_chan
			assign ch_ca               = 22'b0;
			assign ch_err              = 1'b0;
			assign ch_int_num          = 4'b0;
			assign ch_int              = 1'b0;
			assign o_ch_in_resume      = 4'b0;
			assign o_ch_out_ready      = 4'b0;
			assign o_ch_out_data       = 64'b0;
			assign o_ch_out_disconnect = 4'b0;
			assign o_ch_out_mc         = 4'b0;
		end
	endgenerate

endmodule

// The I/O Subsystem: the three I/O Processors of ios_core.v and the
// interfaces on their channels.
//
//   MIOP   17                            the Peripheral Expander with its
//                                        tape, disk and printer
//          20, 21                        the channel pair to the mainframe
//                                        and its master clear lines
//          40/41, 42/43, 44/45, 46/47    four consoles; the fourth is the
//                                        kernel's, the first the station's
//   BIOP   14, 15                        the 100 Mbyte channel pair into
//                                        central memory
//          20 to 22, 24 to 26, 30 to 32  nine DD-29 disk drives
//          42/43                         a console
//   XIOP   42/43                         a console
//
// The consoles are numbered 0 to 3 for the MIOP's, 4 for the BIOP's and 5
// for the XIOP's.  Every other channel has nothing on it.  The software has
// more on some of them (block multiplexer channels on the XIOP, an error log
// and a front-end concentrator on the MIOP) and works without.

module ios #(
	parameter CLOCKS_PER_MS  = 80000,
	parameter EXPANDER_DELAY = 82,     // clocks in a microsecond, or more
	parameter DISK_SETTLE    = 200     // clocks a drive takes to select a mode or to seek
) (
	input wire clk,
	input wire rst,

	// Buffer Memory
	output wire        o_bm_req,
	output wire        o_bm_we,
	output wire [23:0] o_bm_addr,
	output wire [63:0] o_bm_wdata,
	input  wire        i_bm_ack,
	input  wire [63:0] i_bm_rdata,

	// the consoles: a key is held out until it is taken; a character is
	// held out until it is taken
	input  wire [ 5:0] i_key_valid,
	input  wire [41:0] i_key,
	output wire [ 5:0] o_key_ready,
	output wire [ 5:0] o_char_valid,
	output wire [41:0] o_char,
	input  wire [ 5:0] i_char_ready,

	// the tape of the Peripheral Expander: a .tap file in 64-bit words, its
	// first byte in bits 63 to 56 of word 0
	output wire        o_tape_req,
	output wire [20:0] o_tape_addr,
	input  wire        i_tape_ack,
	input  wire [63:0] i_tape_data,
	input  wire [23:0] i_tape_bytes,

	// the disk of the Peripheral Expander: sectors of 512 bytes, as hps_io
	// moves them
	output wire [31:0] o_sd_lba,
	output wire        o_sd_rd,
	output wire        o_sd_wr,
	input  wire        i_sd_ack,
	input  wire [ 8:0] i_sd_buff_addr,
	input  wire [ 7:0] i_sd_buff_dout,
	output wire [ 7:0] o_sd_buff_din,
	input  wire        i_sd_buff_wr,

	// the mainframe: its output channel 11 octal to the MIOP's channel 20,
	// the MIOP's channel 21 to its input channel 10; a parcel with Ready,
	// answered by Resume, the end by Disconnect, each a pulse of one clock.
	// And the lines that hold the CPU and stop its channels.
	input  wire        i_cpu_ready,
	input  wire [15:0] i_cpu_parcel,
	output wire        o_cpu_resume,
	input  wire        i_cpu_disconnect,
	output wire        o_cpu_ready,
	output wire [15:0] o_cpu_parcel,
	input  wire        i_cpu_resume,
	output wire        o_cpu_disconnect,
	output wire        o_cpu_master_clear,
	output wire        o_io_master_clear,

	// central memory, for the BIOP's 100 Mbyte channel: one word a request,
	// held until its acknowledge
	output wire        o_cm_req,
	output wire        o_cm_we,
	output wire [21:0] o_cm_addr,
	output wire [63:0] o_cm_wdata,
	input  wire        i_cm_ack,
	input  wire [63:0] i_cm_rdata,

	// the nine DD-29 drives of the BIOP, in the order of their channels:
	// eight blocks of 512 bytes from block o_drive_lba of one of them
	output wire [31:0] o_drive_lba,
	output wire [ 8:0] o_drive_rd,
	output wire [ 8:0] o_drive_wr,
	input  wire [ 8:0] i_drive_ack,
	input  wire [11:0] i_drive_buff_addr,
	input  wire [ 7:0] i_drive_buff_dout,
	output wire [ 7:0] o_drive_buff_din,
	input  wire        i_drive_buff_wr,

	// for the simulation
	output wire [ 2:0] o_step,
	output wire [47:0] o_p
);

	// the channel buses of ios_core: element 0 the MIOP, 1 the BIOP, 2 the XIOP
	wire [ 2:0] strobe;
	wire [17:0] number;
	wire [11:0] fn;
	wire [47:0] a;
	reg  [47:0] data;
	reg [83:0] busy, done, ask;
	wire [ 2:0] master_clear;
	wire [ 2:0] dma_ack;
	wire [47:0] dma_rdata;

	// ---- the consoles
	wire [15:0] c_data[0:5];
	wire [5:0] c_key_done, c_busy, c_done;
	// console n is on IOP c_iop, and its keyboard on channel c_channel (decimal)
	function [1:0] c_iop(input integer n);
		c_iop = (n < 4) ? 2'd0 : (n == 4) ? 2'd1 : 2'd2;
	endfunction
	function [5:0] c_channel(input integer n);
		c_channel = (n < 4) ? (6'd32 + 6'd2 * n[5:0]) : 6'd34;
	endfunction

	// ---- the MIOP: the Peripheral Expander on channel 17 octal
	localparam EXB = 15, CIA = 16, COA = 17;  // and the pair to the mainframe on 20 and 21
	wire [15:0] x_data, x_dma_addr, x_dma_wdata;
	wire x_busy, x_done, x_ask, x_dma_req, x_dma_we;
	wire [15:0] l_data, l_dma_addr, l_dma_wdata;
	wire [1:0] l_busy, l_done;
	wire l_dma_req, l_dma_we;
	// the MIOP's Local Memory port: the pair to the mainframe goes first
	wire m_dma_req = l_dma_req || x_dma_req;
	wire l_dma_ack = dma_ack[0] && l_dma_req;
	wire x_dma_ack = dma_ack[0] && !l_dma_req;

	// ---- the BIOP: the 100 Mbyte channel pair on 14 and 15 octal, and the drives
	localparam HIA = 12, HOA = 13;
	wire [5:0] b_number = number[11:6];
	wire [1:0] h_busy, h_done;
	wire [15:0] h_dma_addr, h_dma_wdata;
	wire h_dma_req, h_dma_we;
	wire [15:0] k_data, k_dma_addr, k_dma_wdata;
	wire [8:0] k_busy, k_done;
	wire k_dma_req, k_dma_we;
	// the channel of each drive, and the drive of a channel (15 for none)
	function [5:0] k_channel(input integer n);
		k_channel = 6'd16 + n[5:0] + ((n >= 6) ? 6'd2 : (n >= 3) ? 6'd1 : 6'd0);
	endfunction
	reg [3:0] k_drive;
	always @(*)
		case (b_number)
			6'd16:   k_drive = 4'd0;
			6'd17:   k_drive = 4'd1;
			6'd18:   k_drive = 4'd2;
			6'd20:   k_drive = 4'd3;
			6'd21:   k_drive = 4'd4;
			6'd22:   k_drive = 4'd5;
			6'd24:   k_drive = 4'd6;
			6'd25:   k_drive = 4'd7;
			6'd26:   k_drive = 4'd8;
			default: k_drive = 4'd15;
		endcase
	// the BIOP's Local Memory port: the 100 Mbyte channel goes first
	wire b_dma_req = h_dma_req || k_dma_req;
	wire h_dma_ack = dma_ack[1] && h_dma_req;
	wire k_dma_ack = dma_ack[1] && !h_dma_req;

	integer k;
	always @(*) begin
		data            = 48'd0;
		busy            = 84'd0;
		done            = 84'd0;
		ask             = 84'd0;
		data[15:0]      = x_data | l_data;
		busy[EXB-12]    = x_busy;
		done[EXB-12]    = x_done;
		ask[EXB-12]     = x_ask;
		busy[CIA-12]    = l_busy[0];
		done[CIA-12]    = l_done[0];
		busy[COA-12]    = l_busy[1];
		done[COA-12]    = l_done[1];
		data[31:16]     = k_data;
		busy[28+HIA-12] = h_busy[0];
		done[28+HIA-12] = h_done[0];
		busy[28+HOA-12] = h_busy[1];
		done[28+HOA-12] = h_done[1];
		for (k = 0; k < 9; k = k + 1) begin
			busy[28+k_channel(k)-12] = k_busy[k];
			done[28+k_channel(k)-12] = k_done[k];
		end
		for (k = 0; k < 6; k = k + 1) begin
			data[16*c_iop(k)+:16]             = data[16*c_iop(k)+:16] | c_data[k];
			done[28*c_iop(k)+c_channel(k)-12] = c_key_done[k];
			busy[28*c_iop(k)+c_channel(k)-11] = c_busy[k];
			done[28*c_iop(k)+c_channel(k)-11] = c_done[k];
		end
	end

	ios_core #(
		.RAW_REQUEST_0(40'd1 << EXB),
		.CLOCKS_PER_MS(CLOCKS_PER_MS)
	) core (
		.clk           (clk),
		.rst           (rst),
		.o_bm_req      (o_bm_req),
		.o_bm_we       (o_bm_we),
		.o_bm_addr     (o_bm_addr),
		.o_bm_wdata    (o_bm_wdata),
		.i_bm_ack      (i_bm_ack),
		.i_bm_rdata    (i_bm_rdata),
		.o_master_clear(master_clear),
		.o_ch_strobe   (strobe),
		.o_ch_number   (number),
		.o_ch_function (fn),
		.o_ch_a        (a),
		.i_ch_data     (data),
		.i_ch_busy     (busy),
		.i_ch_done     (done),
		.i_ch_ask      (ask),
		.i_dma_req     ({1'b0, b_dma_req, m_dma_req}),
		.i_dma_we      ({1'b0, h_dma_req ? h_dma_we : k_dma_we, l_dma_req ? l_dma_we : x_dma_we}),
		.i_dma_addr    ({16'b0, h_dma_req ? h_dma_addr : k_dma_addr, l_dma_req ? l_dma_addr : x_dma_addr}),
		.i_dma_wdata   ({16'b0, h_dma_req ? h_dma_wdata : k_dma_wdata, l_dma_req ? l_dma_wdata : x_dma_wdata}),
		.o_dma_ack     (dma_ack),
		.o_dma_rdata   (dma_rdata),
		.o_step        (o_step),
		.o_p           (o_p)
	);

	ios_expander #(
		.DELAY(EXPANDER_DELAY)
	) expander (
		.clk           (clk),
		.rst           (master_clear[0]),
		.i_strobe      (strobe[0] && (number[5:0] == EXB)),
		.i_function    (fn[3:0]),
		.i_a           (a[15:0]),
		.o_data        (x_data),
		.o_busy        (x_busy),
		.o_done        (x_done),
		.o_ask         (x_ask),
		.o_dma_req     (x_dma_req),
		.o_dma_we      (x_dma_we),
		.o_dma_addr    (x_dma_addr),
		.o_dma_wdata   (x_dma_wdata),
		.i_dma_ack     (x_dma_ack),
		.i_dma_rdata   (dma_rdata[15:0]),
		.o_tape_req    (o_tape_req),
		.o_tape_addr   (o_tape_addr),
		.i_tape_ack    (i_tape_ack),
		.i_tape_data   (i_tape_data),
		.i_tape_bytes  (i_tape_bytes),
		.o_sd_lba      (o_sd_lba),
		.o_sd_rd       (o_sd_rd),
		.o_sd_wr       (o_sd_wr),
		.i_sd_ack      (i_sd_ack),
		.i_sd_buff_addr(i_sd_buff_addr),
		.i_sd_buff_dout(i_sd_buff_dout),
		.o_sd_buff_din (o_sd_buff_din),
		.i_sd_buff_wr  (i_sd_buff_wr)
	);

	ios_link mainframe (
		.clk               (clk),
		.rst               (master_clear[0]),
		.i_power_on        (rst),
		.i_in              (strobe[0] && (number[5:0] == CIA)),
		.i_out             (strobe[0] && (number[5:0] == COA)),
		.i_function        (fn[3:0]),
		.i_a               (a[15:0]),
		.o_data            (l_data),
		.o_busy            (l_busy),
		.o_done            (l_done),
		.o_dma_req         (l_dma_req),
		.o_dma_we          (l_dma_we),
		.o_dma_addr        (l_dma_addr),
		.o_dma_wdata       (l_dma_wdata),
		.i_dma_ack         (l_dma_ack),
		.i_dma_rdata       (dma_rdata[15:0]),
		.i_ready           (i_cpu_ready),
		.i_parcel          (i_cpu_parcel),
		.o_resume          (o_cpu_resume),
		.i_disconnect      (i_cpu_disconnect),
		.o_ready           (o_cpu_ready),
		.o_parcel          (o_cpu_parcel),
		.i_resume          (i_cpu_resume),
		.o_disconnect      (o_cpu_disconnect),
		.o_cpu_master_clear(o_cpu_master_clear),
		.o_io_master_clear (o_io_master_clear)
	);

	ios_hsp high_speed (
		.clk        (clk),
		.rst        (master_clear[1]),
		.i_in       (strobe[1] && (b_number == HIA)),
		.i_out      (strobe[1] && (b_number == HOA)),
		.i_function (fn[7:4]),
		.i_a        (a[31:16]),
		.o_busy     (h_busy),
		.o_done     (h_done),
		.o_dma_req  (h_dma_req),
		.o_dma_we   (h_dma_we),
		.o_dma_addr (h_dma_addr),
		.o_dma_wdata(h_dma_wdata),
		.i_dma_ack  (h_dma_ack),
		.i_dma_rdata(dma_rdata[31:16]),
		.o_cm_req   (o_cm_req),
		.o_cm_we    (o_cm_we),
		.o_cm_addr  (o_cm_addr),
		.o_cm_wdata (o_cm_wdata),
		.i_cm_ack   (i_cm_ack),
		.i_cm_rdata (i_cm_rdata)
	);

	ios_disks #(
		.DRIVES(9),
		.SETTLE(DISK_SETTLE)
	) drives (
		.clk           (clk),
		.rst           (master_clear[1]),
		.i_strobe      (strobe[1] && (k_drive != 4'd15)),
		.i_drive       (k_drive),
		.i_function    (fn[7:4]),
		.i_a           (a[31:16]),
		.o_data        (k_data),
		.o_busy        (k_busy),
		.o_done        (k_done),
		.o_dma_req     (k_dma_req),
		.o_dma_we      (k_dma_we),
		.o_dma_addr    (k_dma_addr),
		.o_dma_wdata   (k_dma_wdata),
		.i_dma_ack     (k_dma_ack),
		.i_dma_rdata   (dma_rdata[31:16]),
		.o_sd_lba      (o_drive_lba),
		.o_sd_rd       (o_drive_rd),
		.o_sd_wr       (o_drive_wr),
		.i_sd_ack      (i_drive_ack),
		.i_sd_buff_addr(i_drive_buff_addr),
		.i_sd_buff_dout(i_drive_buff_dout),
		.o_sd_buff_din (o_drive_buff_din),
		.i_sd_buff_wr  (i_drive_buff_wr)
	);

	genvar g;
	generate
		for (g = 0; g < 6; g = g + 1) begin : g_console
			localparam integer       IOP      = (g < 4) ? 0 : (g == 4) ? 1 : 2;
			localparam         [5:0] KEYBOARD = (g < 4) ? (32 + 2 * g) : 34;
			wire       here = strobe[IOP];
			wire [5:0] channel = number[6*IOP+:6];
			ios_console console (
				.clk           (clk),
				.rst           (master_clear[IOP]),
				.i_keyboard    (here && (channel == KEYBOARD)),
				.i_display     (here && (channel == KEYBOARD + 6'd1)),
				.i_function    (fn[4*IOP+:4]),
				.i_a           (a[16*IOP+:7]),
				.o_data        (c_data[g]),
				.o_key_done    (c_key_done[g]),
				.o_display_busy(c_busy[g]),
				.o_display_done(c_done[g]),
				.i_key_valid   (i_key_valid[g]),
				.i_key         (i_key[7*g+:7]),
				.o_key_ready   (o_key_ready[g]),
				.o_char_valid  (o_char_valid[g]),
				.o_char        (o_char[7*g+:7]),
				.i_char_ready  (i_char_ready[g])
			);
		end
	endgenerate

endmodule

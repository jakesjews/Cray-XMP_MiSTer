// The I/O Subsystem: the three I/O Processors of ios_core.v and the
// interfaces on their channels that exist in hardware so far.
//
//   MIOP   17                            the Peripheral Expander with its
//                                        tape, disk and printer
//          40/41, 42/43, 44/45, 46/47   four consoles; the fourth is the
//                                        kernel's, the first the station's
//   BIOP   42/43                         a console
//   XIOP   42/43                         a console
//
// The consoles are numbered 0 to 3 for the MIOP's, 4 for the BIOP's and 5
// for the XIOP's.  Every other channel from 14 octal up still leads out of
// this module, as it does out of ios_core; the MIOP's Local Memory port is
// the expander's.

module ios #(
	parameter CLOCKS_PER_MS  = 80000,
	parameter EXPANDER_DELAY = 82      // clocks in a microsecond, or more
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

	// the channels with nothing on them yet, as in ios_core
	output wire [  2:0] o_ch_strobe,
	output wire [ 17:0] o_ch_number,
	output wire [ 11:0] o_ch_function,
	output wire [ 47:0] o_ch_a,
	input  wire [ 47:0] i_ch_data,
	input  wire [ 83:0] i_ch_busy,
	input  wire [ 83:0] i_ch_done,
	input  wire [ 83:0] i_ch_ask,
	input  wire [  2:1] i_dma_req,      // the MIOP's port is the expander's
	input  wire [  2:1] i_dma_we,
	input  wire [47:16] i_dma_addr,
	input  wire [47:16] i_dma_wdata,
	output wire [  2:0] o_dma_ack,
	output wire [ 47:0] o_dma_rdata,

	output wire [ 2:0] o_step,
	output wire [47:0] o_p
);

	// what the consoles add to the buses of ios_core
	wire [15:0] c_data[0:5];
	wire [5:0] c_key_done, c_busy, c_done;
	reg [47:0] data;
	reg [83:0] busy, done;

	// console n is on IOP c_iop, and its keyboard on channel c_channel (decimal)
	function [1:0] c_iop(input integer n);
		c_iop = (n < 4) ? 2'd0 : (n == 4) ? 2'd1 : 2'd2;
	endfunction
	function [5:0] c_channel(input integer n);
		c_channel = (n < 4) ? (6'd32 + 6'd2 * n[5:0]) : 6'd34;
	endfunction

	// the Peripheral Expander: channel 17 octal of the MIOP
	localparam EXB = 15;
	wire [15:0] x_data, x_dma_addr, x_dma_wdata;
	wire x_busy, x_done, x_ask, x_dma_req, x_dma_we;
	reg [83:0] ask;

	integer k;
	always @(*) begin
		data         = i_ch_data;
		busy         = i_ch_busy;
		done         = i_ch_done;
		ask          = i_ch_ask;
		data[15:0]   = data[15:0] | x_data;
		busy[EXB-12] = x_busy;
		done[EXB-12] = x_done;
		ask[EXB-12]  = x_ask;
		for (k = 0; k < 6; k = k + 1) begin
			data[16*c_iop(k)+:16]             = data[16*c_iop(k)+:16] | c_data[k];
			done[28*c_iop(k)+c_channel(k)-12] = c_key_done[k];
			busy[28*c_iop(k)+c_channel(k)-11] = c_busy[k];
			done[28*c_iop(k)+c_channel(k)-11] = c_done[k];
		end
	end

	wire [2:0] master_clear;

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
		.o_ch_strobe   (o_ch_strobe),
		.o_ch_number   (o_ch_number),
		.o_ch_function (o_ch_function),
		.o_ch_a        (o_ch_a),
		.i_ch_data     (data),
		.i_ch_busy     (busy),
		.i_ch_done     (done),
		.i_ch_ask      (ask),
		.i_dma_req     ({i_dma_req, x_dma_req}),
		.i_dma_we      ({i_dma_we, x_dma_we}),
		.i_dma_addr    ({i_dma_addr, x_dma_addr}),
		.i_dma_wdata   ({i_dma_wdata, x_dma_wdata}),
		.o_dma_ack     (o_dma_ack),
		.o_dma_rdata   (o_dma_rdata),
		.o_step        (o_step),
		.o_p           (o_p)
	);

	ios_expander #(
		.DELAY(EXPANDER_DELAY)
	) expander (
		.clk           (clk),
		.rst           (master_clear[0]),
		.i_strobe      (o_ch_strobe[0] && (o_ch_number[5:0] == EXB)),
		.i_function    (o_ch_function[3:0]),
		.i_a           (o_ch_a[15:0]),
		.o_data        (x_data),
		.o_busy        (x_busy),
		.o_done        (x_done),
		.o_ask         (x_ask),
		.o_dma_req     (x_dma_req),
		.o_dma_we      (x_dma_we),
		.o_dma_addr    (x_dma_addr),
		.o_dma_wdata   (x_dma_wdata),
		.i_dma_ack     (o_dma_ack[0]),
		.i_dma_rdata   (o_dma_rdata[15:0]),
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

	genvar g;
	generate
		for (g = 0; g < 6; g = g + 1) begin : g_console
			localparam integer       IOP      = (g < 4) ? 0 : (g == 4) ? 1 : 2;
			localparam         [5:0] KEYBOARD = (g < 4) ? (32 + 2 * g) : 34;
			wire       here = o_ch_strobe[IOP];
			wire [5:0] number = o_ch_number[6*IOP+:6];
			ios_console console (
				.clk           (clk),
				.rst           (master_clear[IOP]),
				.i_keyboard    (here && (number == KEYBOARD)),
				.i_display     (here && (number == KEYBOARD + 6'd1)),
				.i_function    (o_ch_function[4*IOP+:4]),
				.i_a           (o_ch_a[16*IOP+:7]),
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

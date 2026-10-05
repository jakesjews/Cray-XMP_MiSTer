// The I/O Subsystem: the three I/O Processors of ios_core.v and the
// interfaces on their channels that exist in hardware so far.
//
//   MIOP   40/41, 42/43, 44/45, 46/47   four consoles; the fourth is the
//                                        kernel's, the first the station's
//   BIOP   42/43                         a console
//   XIOP   42/43                         a console
//
// The consoles are numbered 0 to 3 for the MIOP's, 4 for the BIOP's and 5
// for the XIOP's.  Every other channel from 14 octal up still leads out of
// this module, as it does out of ios_core.

module ios #(
	parameter [39:0] RAW_REQUEST_0 = 40'd0,
	parameter        CLOCKS_PER_MS = 80000
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

	// the channels with nothing on them yet, as in ios_core
	output wire [ 2:0] o_ch_strobe,
	output wire [17:0] o_ch_number,
	output wire [11:0] o_ch_function,
	output wire [47:0] o_ch_a,
	input  wire [47:0] i_ch_data,
	input  wire [83:0] i_ch_busy,
	input  wire [83:0] i_ch_done,
	input  wire [83:0] i_ch_ask,
	input  wire [ 2:0] i_dma_req,
	input  wire [ 2:0] i_dma_we,
	input  wire [47:0] i_dma_addr,
	input  wire [47:0] i_dma_wdata,
	output wire [ 2:0] o_dma_ack,
	output wire [47:0] o_dma_rdata,

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

	integer k;
	always @(*) begin
		data = i_ch_data;
		busy = i_ch_busy;
		done = i_ch_done;
		for (k = 0; k < 6; k = k + 1) begin
			data[16*c_iop(k)+:16]             = data[16*c_iop(k)+:16] | c_data[k];
			done[28*c_iop(k)+c_channel(k)-12] = c_key_done[k];
			busy[28*c_iop(k)+c_channel(k)-11] = c_busy[k];
			done[28*c_iop(k)+c_channel(k)-11] = c_done[k];
		end
	end

	wire [2:0] master_clear;

	ios_core #(
		.RAW_REQUEST_0(RAW_REQUEST_0),
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
		.i_ch_ask      (i_ch_ask),
		.i_dma_req     (i_dma_req),
		.i_dma_we      (i_dma_we),
		.i_dma_addr    (i_dma_addr),
		.i_dma_wdata   (i_dma_wdata),
		.o_dma_ack     (o_dma_ack),
		.o_dma_rdata   (o_dma_rdata),
		.o_step        (o_step),
		.o_p           (o_p)
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

// The tape drive of the Peripheral Expander (rtl/ios/ios_expander.v) with the
// reel on it (rtl/ios/ios_reel.v), for sim/harness/tape_main.cpp.
module ios_tape_tb (
	input wire clk,
	input wire rst,
	input wire i_real,

	// channel 17
	input  wire        i_strobe,
	input  wire [ 3:0] i_function,
	input  wire [15:0] i_a,
	output wire [15:0] o_data,
	output wire        o_ask,

	// Local Memory
	output wire        o_dma_req,
	output wire        o_dma_we,
	output wire [15:0] o_dma_addr,
	output wire [15:0] o_dma_wdata,
	input  wire        i_dma_ack,
	input  wire [15:0] i_dma_rdata,

	// the boot tape
	output wire        o_boot_req,
	output wire [20:0] o_boot_addr,
	input  wire        i_boot_ack,
	input  wire [63:0] i_boot_data,
	input  wire [23:0] i_boot_bytes,

	// the tape file
	input  wire        i_mounted,
	input  wire [54:0] i_blocks,
	input  wire        i_readonly,
	output wire [31:0] o_sd_lba,
	output wire        o_sd_rd,
	output wire        o_sd_wr,
	input  wire        i_sd_ack,
	input  wire [ 8:0] i_sd_buff_addr,
	input  wire [ 7:0] i_sd_buff_dout,
	output wire [ 7:0] o_sd_buff_din,
	input  wire        i_sd_buff_wr
);

	wire t_req, t_ack, t_locked, t_change, t_put, t_sync, t_wack;
	wire [20:0] t_addr;
	wire [63:0] t_data;
	wire [23:0] t_bytes, t_at;
	wire [7:0] t_wdata;

	ios_expander expander (
		.clk           (clk),
		.rst           (rst),
		.i_real        (i_real),
		.i_strobe      (i_strobe),
		.i_function    (i_function),
		.i_a           (i_a),
		.o_data        (o_data),
		.o_busy        (),
		.o_done        (),
		.o_ask         (o_ask),
		.o_dma_req     (o_dma_req),
		.o_dma_we      (o_dma_we),
		.o_dma_addr    (o_dma_addr),
		.o_dma_wdata   (o_dma_wdata),
		.i_dma_ack     (i_dma_ack),
		.i_dma_rdata   (i_dma_rdata),
		.o_tape_req    (t_req),
		.o_tape_addr   (t_addr),
		.i_tape_ack    (t_ack),
		.i_tape_data   (t_data),
		.i_tape_bytes  (t_bytes),
		.i_tape_locked (t_locked),
		.i_tape_change (t_change),
		.o_tape_put    (t_put),
		.o_tape_sync   (t_sync),
		.o_tape_at     (t_at),
		.o_tape_wdata  (t_wdata),
		.i_tape_wack   (t_wack),
		.o_sd_lba      (),
		.o_sd_rd       (),
		.o_sd_wr       (),
		.i_sd_ack      (1'b0),
		.i_sd_buff_addr(9'd0),
		.i_sd_buff_dout(8'd0),
		.o_sd_buff_din (),
		.i_sd_buff_wr  (1'b0),
		.o_print_valid (),
		.o_print       (),
		.i_print_ready (1'b1)
	);

	ios_reel reel (
		.clk           (clk),
		.rst           (rst),
		.i_req         (t_req),
		.i_addr        (t_addr),
		.o_ack         (t_ack),
		.o_data        (t_data),
		.i_put         (t_put),
		.i_sync        (t_sync),
		.i_at          (t_at),
		.i_wdata       (t_wdata),
		.o_wack        (t_wack),
		.o_bytes       (t_bytes),
		.o_locked      (t_locked),
		.o_change      (t_change),
		.o_boot_req    (o_boot_req),
		.o_boot_addr   (o_boot_addr),
		.i_boot_ack    (i_boot_ack),
		.i_boot_data   (i_boot_data),
		.i_boot_bytes  (i_boot_bytes),
		.i_mounted     (i_mounted),
		.i_blocks      (i_blocks),
		.i_readonly    (i_readonly),
		.o_sd_lba      (o_sd_lba),
		.o_sd_rd       (o_sd_rd),
		.o_sd_wr       (o_sd_wr),
		.i_sd_ack      (i_sd_ack),
		.i_sd_buff_addr(i_sd_buff_addr),
		.i_sd_buff_dout(i_sd_buff_dout),
		.o_sd_buff_din (o_sd_buff_din),
		.i_sd_buff_wr  (i_sd_buff_wr)
	);

endmodule

// The Ampex terminal with its screen memory, for sim/harness/ampex_main.cpp.
module term_ampex_tb (
	input wire clk,
	input wire reset,

	input  wire [6:0] din,
	input  wire       din_valid,
	output wire       din_ready,

	output wire [4:0] top_row,
	output wire [6:0] cur_x,
	output wire [4:0] cur_y
);

	reg  [ 7:0] screen[0:2047]  /* verilator public_flat_rd */;
	wire [10:0] addr;
	wire [ 7:0] wdata;
	wire        we;
	reg  [ 7:0] q;

	always @(posedge clk) begin
		if (we) screen[addr] <= wdata;
		q <= screen[addr];
`ifdef RW_POISON
		if (we) q <= ~wdata;
`endif
	end

	term_ampex ctrl (
		.clk      (clk),
		.reset    (reset),
		.din      (din),
		.din_valid(din_valid),
		.din_ready(din_ready),
		.addr     (addr),
		.wdata    (wdata),
		.we       (we),
		.rdata    (q),
		.top_row  (top_row),
		.cur_x    (cur_x),
		.cur_y    (cur_y)
	);

endmodule

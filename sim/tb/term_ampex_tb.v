// Two Ampex terminals with their screen memories, as rtl/terminal/xmp_terminal.v
// has them: one of two pages like the consoles, one of a single page like the
// printer's screen.  `one` chooses the terminal that gets the characters and
// the keys and that the outputs are of.  For sim/harness/ampex_main.cpp.
module term_ampex_tb (
	input wire clk,
	input wire reset,
	input wire one,
	input wire frame,

	input  wire [6:0] din,
	input  wire       din_valid,
	output wire       din_ready,

	input  wire [7:0] key,
	input  wire       key_valid,
	output wire       key_ready,

	output wire [6:0] dout,
	output wire       dout_valid,
	input  wire       dout_ready,

	output wire [4:0] top_row,
	output wire       page,
	output wire [6:0] cur_x,
	output wire [4:0] cur_y,
	output wire       cur_status,
	output wire       prog_mode,
	output wire       bell,

	// from inside the terminal: the last row of each page, and the bells rung
	output wire [ 4:0] bottom0,
	output wire [ 4:0] bottom1,
	output reg  [31:0] bells
);

	// the terminal of two pages
	reg [7:0] chars2[0:4095]  /* verilator public_flat_rd */;
	reg [3:0] attrs2[0:4095]  /* verilator public_flat_rd */;
	wire [11:0] addr2, wdata2;
	wire we_char2, we_attr2;
	reg [11:0] q2;
	always @(posedge clk) begin
		if (we_char2) chars2[addr2] <= wdata2[7:0];
		if (we_attr2) attrs2[addr2] <= wdata2[11:8];
		q2 <= {attrs2[addr2], chars2[addr2]};
`ifdef RW_POISON
		if (we_char2) q2[7:0] <= ~wdata2[7:0];
		if (we_attr2) q2[11:8] <= ~wdata2[11:8];
`endif
	end

	wire [6:0] dout2, cur_x2;
	wire [4:0] top_row2, cur_y2;
	wire din_ready2, key_ready2, dout_valid2, page2, cur_status2, prog_mode2, bell2;

	term_ampex #(
		.PAGES(2)
	) two (
		.clk       (clk),
		.reset     (reset),
		.frame     (frame),
		.din       (din),
		.din_valid (din_valid && !one),
		.din_ready (din_ready2),
		.key       (key),
		.key_valid (key_valid && !one),
		.key_ready (key_ready2),
		.dout      (dout2),
		.dout_valid(dout_valid2),
		.dout_ready(dout_ready),
		.addr      (addr2),
		.wdata     (wdata2),
		.we_char   (we_char2),
		.we_attr   (we_attr2),
		.rdata     (q2),
		.top_row   (top_row2),
		.page      (page2),
		.cur_x     (cur_x2),
		.cur_y     (cur_y2),
		.cur_status(cur_status2),
		.prog_mode (prog_mode2),
		.bell      (bell2)
	);

	// the terminal of one page
	reg [7:0] chars1[0:2047]  /* verilator public_flat_rd */;
	reg [3:0] attrs1[0:2047]  /* verilator public_flat_rd */;
	wire [11:0] addr1, wdata1;
	wire we_char1, we_attr1;
	reg [11:0] q1;
	always @(posedge clk) begin
		if (we_char1) chars1[addr1[10:0]] <= wdata1[7:0];
		if (we_attr1) attrs1[addr1[10:0]] <= wdata1[11:8];
		q1 <= {attrs1[addr1[10:0]], chars1[addr1[10:0]]};
`ifdef RW_POISON
		if (we_char1) q1[7:0] <= ~wdata1[7:0];
		if (we_attr1) q1[11:8] <= ~wdata1[11:8];
`endif
	end

	wire [6:0] dout1, cur_x1;
	wire [4:0] top_row1, cur_y1;
	wire din_ready1, key_ready1, dout_valid1, page1, cur_status1, prog_mode1, bell1;

	term_ampex #(
		.PAGES(1)
	) single (
		.clk       (clk),
		.reset     (reset),
		.frame     (frame),
		.din       (din),
		.din_valid (din_valid && one),
		.din_ready (din_ready1),
		.key       (key),
		.key_valid (key_valid && one),
		.key_ready (key_ready1),
		.dout      (dout1),
		.dout_valid(dout_valid1),
		.dout_ready(dout_ready),
		.addr      (addr1),
		.wdata     (wdata1),
		.we_char   (we_char1),
		.we_attr   (we_attr1),
		.rdata     (q1),
		.top_row   (top_row1),
		.page      (page1),
		.cur_x     (cur_x1),
		.cur_y     (cur_y1),
		.cur_status(cur_status1),
		.prog_mode (prog_mode1),
		.bell      (bell1)
	);

	assign din_ready  = one ? din_ready1 : din_ready2;
	assign key_ready  = one ? key_ready1 : key_ready2;
	assign dout       = one ? dout1 : dout2;
	assign dout_valid = one ? dout_valid1 : dout_valid2;
	assign top_row    = one ? top_row1 : top_row2;
	assign page       = one ? page1 : page2;
	assign cur_x      = one ? cur_x1 : cur_x2;
	assign cur_y      = one ? cur_y1 : cur_y2;
	assign cur_status = one ? cur_status1 : cur_status2;
	assign prog_mode  = one ? prog_mode1 : prog_mode2;
	assign bell       = one ? bell1 : bell2;
	assign bottom0    = one ? single.bot0 : two.bot0;
	assign bottom1    = one ? single.bot1 : two.bot1;

	always @(posedge clk)
		if (reset) bells <= 32'd0;
		else if (one ? single.ring : two.ring) bells <= bells + 32'd1;

endmodule

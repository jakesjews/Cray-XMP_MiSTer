// The two consoles of the CRAY X-MP core on one display: the operator's
// console of the I/O Subsystem and the station.  Each is an Ampex Dialogue 80
// screen of its own that is kept up to date all the time; `visible` chooses the
// one that is shown.  Video timing and fonts are those of the CRAY-1 core.

module xmp_terminal (
	input wire clk,
	input wire reset,

	input wire ce_pix,
	input wire font_8x8,
	input wire visible,   // 0 the operator's console, 1 the station

	// characters to display, one stream a screen
	input  wire [13:0] rx_data,
	input  wire [ 1:0] rx_valid,
	output wire [ 1:0] rx_ready,

	output wire hsync,
	output wire vsync,
	output wire hblank,
	output wire vblank,
	output wire video
);

	wire [10:0] raddr;
	wire [ 7:0] rdata  [0:1];
	wire [ 4:0] top_row[0:1]  /* verilator public_flat_rd */;
	wire [ 4:0] cur_y  [0:1]  /* verilator public_flat_rd */;
	wire [ 6:0] cur_x  [0:1]  /* verilator public_flat_rd */;
	wire [ 1:0] wrote;

	genvar g;
	generate
		for (g = 0; g < 2; g = g + 1) begin : g_screen
			// 24 rows of 80 columns; one port for the terminal, which never reads
			// a character in the clock it writes it in, and one for the video
			(* ramstyle = "no_rw_check" *)reg  [ 7:0] screen[0:2047]  /* verilator public_flat_rd */;
			wire [10:0] addr;
			wire [ 7:0] wdata;
			wire        we;
			reg [7:0] q, vq;

			always @(posedge clk) begin
				if (we) screen[addr] <= wdata;
				q <= screen[addr];
`ifdef RW_POISON
				if (we) q <= ~wdata;
`endif
			end
			always @(posedge clk) vq <= screen[raddr];

			term_ampex ctrl (
				.clk      (clk),
				.reset    (reset),
				.din      (rx_data[7*g+:7]),
				.din_valid(rx_valid[g]),
				.din_ready(rx_ready[g]),
				.addr     (addr),
				.wdata    (wdata),
				.we       (we),
				.rdata    (q),
				.top_row  (top_row[g]),
				.cur_x    (cur_x[g]),
				.cur_y    (cur_y[g])
			);

			assign rdata[g] = vq;
			assign wrote[g] = we;
		end
	endgenerate

	// cursor blink: about 1 Hz, restarted (cursor solid) by a write to the screen shown
	wire       frame_tick;
	reg  [5:0] blink;
	reg        shown;
	always @(posedge clk) begin
		shown <= visible;
		if (reset || wrote[visible] || shown != visible) blink <= 0;
		else if (frame_tick) blink <= blink + 1'd1;
	end

	term_video video_gen (
		.clk       (clk),
		.ce_pix    (ce_pix),
		.font_8x8  (font_8x8),
		.ram_addr  (raddr),
		.ram_data  (rdata[visible]),
		.top_row   (top_row[visible]),
		.cursor_x  (cur_x[visible]),
		.cursor_y  (cur_y[visible]),
		.cursor_on (~blink[5]),
		.hsync     (hsync),
		.vsync     (vsync),
		.hblank    (hblank),
		.vblank    (vblank),
		.video     (video),
		.frame_tick(frame_tick)
	);

endmodule

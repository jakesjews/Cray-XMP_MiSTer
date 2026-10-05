// Console terminal for the Cray core: 80x24 text screen and keyboard.
//
// Bytes written to the screen arrive on rx_*; keys leave on kbd_*.  Nothing is
// echoed locally.  The screen buffer is one 2K block RAM and both fonts are
// block ROMs.  Video timing and fonts come from the MiSTer VT52 core.

module cray_terminal (
	input wire clk,
	input wire reset,

	input wire ce_pix,
	input wire font_8x8,

	// bytes to display
	input  wire [7:0] rx_data,
	input  wire       rx_valid,
	output wire       rx_ready,

	// keys typed
	input  wire [10:0] ps2_key,
	output wire [ 7:0] kbd_data,
	output wire        kbd_valid,
	input  wire        kbd_ready,

	output wire hsync,
	output wire vsync,
	output wire hblank,
	output wire vblank,
	output wire video
);

	// screen buffer: 24 rows x 80 columns
	reg [7:0] screen[0:2047];
	wire [10:0] waddr, raddr;
	wire [7:0] wdata;
	wire       we;
	reg  [7:0] rdata;

	always @(posedge clk) begin
		if (we) screen[waddr] <= wdata;
		rdata <= screen[raddr];
	end

	wire [4:0] top_row, cur_y;
	wire [6:0] cur_x;

	term_ctrl ctrl (
		.clk      (clk),
		.reset    (reset),
		.din      (rx_data),
		.din_valid(rx_valid),
		.din_ready(rx_ready),
		.waddr    (waddr),
		.wdata    (wdata),
		.we       (we),
		.top_row  (top_row),
		.cur_x    (cur_x),
		.cur_y    (cur_y)
	);

	// cursor blink: about 1 Hz, restarted (cursor solid) by any screen write
	wire       frame_tick;
	reg  [5:0] blink;
	always @(posedge clk) begin
		if (reset || we) blink <= 0;
		else if (frame_tick) blink <= blink + 1'd1;
	end

	term_video video_gen (
		.clk       (clk),
		.ce_pix    (ce_pix),
		.font_8x8  (font_8x8),
		.ram_addr  (raddr),
		.ram_data  (rdata),
		.top_row   (top_row),
		.cursor_x  (cur_x),
		.cursor_y  (cur_y),
		.cursor_on (~blink[5]),
		.hsync     (hsync),
		.vsync     (vsync),
		.hblank    (hblank),
		.vblank    (vblank),
		.video     (video),
		.frame_tick(frame_tick)
	);

	term_keyboard keyboard (
		.clk       (clk),
		.reset     (reset),
		.ps2_key   (ps2_key),
		.dout      (kbd_data),
		.dout_valid(kbd_valid),
		.dout_ready(kbd_ready)
	);

endmodule

// The screens of the CRAY X-MP core on one display: the operator's console of
// the I/O Subsystem, the station, and what the printer prints.  The two
// consoles are Ampex Dialogue 80 terminals with their two pages of display
// memory; the printer's screen is a plain one of a single page that only
// takes characters and new lines.  Each is kept up to date all the time; `visible` chooses the one
// that is shown and that the keyboard belongs to.  Video timing and fonts come
// from the MiSTer VT52 core.
module xmp_terminal #(
	parameter SCREENS = 3
) (
	input wire clk,
	input wire reset,

	input wire       ce_pix,
	input wire       font_8x8,
	input wire [1:0] visible,   // the number of the screen that is shown

	// characters to display, one stream a screen
	input  wire [7*SCREENS-1:0] rx_data,
	input  wire [  SCREENS-1:0] rx_valid,
	output wire [  SCREENS-1:0] rx_ready,

	// the keyboard, for the screen that is shown: the code of the key (term_keyboard.v)
	input  wire [7:0] key_data,
	input  wire       key_valid,
	output wire       key_ready,

	// what a console's terminal sends: the keys typed at it and its answers
	output wire [13:0] tx_data,
	output wire [ 1:0] tx_valid,
	input  wire [ 1:0] tx_ready,

	output wire hsync,
	output wire vsync,
	output wire hblank,
	output wire vblank,
	output wire video,
	output wire half,    // the pixel is of a character at half intensity
	output wire bell     // the bell of the screen that is shown sounds
);

	wire [       11:0] raddr;
	wire [       11:0] rdata                                [0:SCREENS-1];
	wire [        4:0] top_row                              [0:SCREENS-1]  /* verilator public_flat_rd */;
	wire [        4:0] cur_y                                [0:SCREENS-1]  /* verilator public_flat_rd */;
	wire [        6:0] cur_x                                [0:SCREENS-1]  /* verilator public_flat_rd */;
	wire [SCREENS-1:0] page  /* verilator public_flat_rd */;
	wire [SCREENS-1:0] cur_status;
	wire [SCREENS-1:0] prog_mode;
	wire [SCREENS-1:0] bells;
	wire [SCREENS-1:0] wrote;
	wire [SCREENS-1:0] key_readys;
	wire               frame_tick;

	genvar g;
	generate
		for (g = 0; g < SCREENS; g = g + 1) begin : g_screen
			// the printer's screen has one page, the consoles two
			localparam PAGES = (g == 2) ? 1 : 2;
			localparam AW    = (PAGES == 2) ? 12 : 11;

			// A cell is the character with its protect bit, and four attribute
			// bits that are written on their own.  One port of each memory is the
			// terminal's, which never reads a cell in the clock it writes it in,
			// and one the video's.
			(* ramstyle = "no_rw_check" *)reg  [ 7:0] chars [0:(1<<AW)-1]  /* verilator public_flat_rd */;
			(* ramstyle = "no_rw_check" *)reg  [ 3:0] attrs [0:(1<<AW)-1]  /* verilator public_flat_rd */;
			wire [11:0] addr;
			wire [11:0] wdata;
			wire we_char, we_attr;
			reg [11:0] vq;
			always @(posedge clk) begin
				if (we_char) chars[addr[AW-1:0]] <= wdata[7:0];
				if (we_attr) attrs[addr[AW-1:0]] <= wdata[11:8];
			end
			always @(posedge clk) vq <= {attrs[raddr[AW-1:0]], chars[raddr[AW-1:0]]};

			if (PAGES == 2) begin : g_console
				reg [11:0] q;
				always @(posedge clk) begin
					q <= {attrs[addr], chars[addr]};
`ifdef RW_POISON
					if (we_char) q[7:0] <= ~wdata[7:0];
					if (we_attr) q[11:8] <= ~wdata[11:8];
`endif
				end
				term_ampex #(
					.PAGES(PAGES)
				) ctrl (
					.clk       (clk),
					.reset     (reset),
					.frame     (frame_tick),
					.din       (rx_data[7*g+:7]),
					.din_valid (rx_valid[g]),
					.din_ready (rx_ready[g]),
					.key       (key_data),
					.key_valid (key_valid && (visible == g)),
					.key_ready (key_readys[g]),
					.dout      (tx_data[7*g+:7]),
					.dout_valid(tx_valid[g]),
					.dout_ready(tx_ready[g]),
					.addr      (addr),
					.wdata     (wdata),
					.we_char   (we_char),
					.we_attr   (we_attr),
					.rdata     (q),
					.top_row   (top_row[g]),
					.page      (page[g]),
					.cur_x     (cur_x[g]),
					.cur_y     (cur_y[g]),
					.cur_status(cur_status[g]),
					.prog_mode (prog_mode[g]),
					.bell      (bells[g])
				);
				// a new character makes the cursor solid; the status line does not
				assign wrote[g] = we_char && (addr[10:0] < 11'd1920);
			end else begin : g_printer
				wire we;
				term_plain ctrl (
					.clk      (clk),
					.reset    (reset),
					.din      (rx_data[7*g+:7]),
					.din_valid(rx_valid[g]),
					.din_ready(rx_ready[g]),
					.addr     (addr),
					.wdata    (wdata),
					.we       (we),
					.top_row  (top_row[g]),
					.cur_x    (cur_x[g]),
					.cur_y    (cur_y[g])
				);
				assign we_char       = we;
				assign we_attr       = we;
				assign page[g]       = 1'b0;
				assign cur_status[g] = 1'b0;
				assign prog_mode[g]  = 1'b0;
				assign bells[g]      = 1'b0;
				assign key_readys[g] = 1'b1;
				assign wrote[g]      = we;
			end

			assign rdata[g] = vq;
		end
	endgenerate

	assign key_ready = key_readys[visible];
	assign bell      = bells[visible];

	// The cursor blinks about once a second and is solid again after a write to
	// the screen shown.  The flashing characters change every nine frames, as
	// the terminal's firmware has it.
	reg [5:0] blink;
	reg [3:0] flash_n;
	reg       flash_on;
	reg [1:0] shown;
	always @(posedge clk) begin
		shown <= visible;
		if (reset || wrote[visible] || shown != visible) blink <= 0;
		else if (frame_tick) blink <= blink + 1'd1;
		if (reset) begin
			flash_n  <= 4'd0;
			flash_on <= 1'b1;
		end else if (frame_tick) begin
			if (flash_n == 4'd8) begin
				flash_n  <= 4'd0;
				flash_on <= ~flash_on;
			end else flash_n <= flash_n + 4'd1;
		end
	end

	term_video video_gen (
		.clk          (clk),
		.ce_pix       (ce_pix),
		.font_8x8     (font_8x8),
		.ram_addr     (raddr),
		.ram_data     (rdata[visible]),
		.page         (page[visible]),
		.top_row      (top_row[visible]),
		.cursor_x     (cur_x[visible]),
		.cursor_y     (cur_y[visible]),
		.cursor_status(cur_status[visible]),
		.cursor_on    (~blink[5]),
		.flash_on     (flash_on),
		.prog_mode    (prog_mode[visible]),
		.hsync        (hsync),
		.vsync        (vsync),
		.hblank       (hblank),
		.vblank       (vblank),
		.video        (video),
		.half         (half),
		.frame_tick   (frame_tick)
	);

endmodule

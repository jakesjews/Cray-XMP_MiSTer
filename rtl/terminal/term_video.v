// The raster of an Ampex Dialogue 80 screen: the status line and 24 lines of 80
// characters.
//
// Timing is that of the MiSTer VT52 core, so direct video works unchanged:
//   936 pixel line = 96 back porch + 640 visible + 72 front porch + 128 sync
//   8x16 font: 524 lines (73 + 400 + 49 + 2), ce_pix at 29.4 MHz -> 31.41 kHz, 59.94 Hz
//   8x8  font: 262 lines (12 + 200 + 48 + 2), ce_pix at 14.7 MHz -> 15.71 kHz, 59.94 Hz
//
// A cell of the screen memory is twelve bits: the character, the protect bit,
// which shows as half intensity, and the attributes reverse, blank, flash and
// underline.  The status line is the first row of the display, as the
// terminal's firmware sets its display controller up; the text lines follow
// from the memory row top_row on.
//
// The letters, digits and signs come from the fonts of the VT52 core.  The
// codes below 20 hex are drawn here: 01 to 0B are the eleven line drawing
// characters, strokes from the middle of the cell to its edges; the others
// are the pictures of the control characters that program mode puts on the
// screen, two small letters each, as the terminal's character generator has
// them.  Code 00 is one of those and shows in program mode only: a screen
// that has been cleared is full of it.
//
// Character cells are fetched one cell (8 pixels) ahead of where they are drawn:
// phase 0 presents the cell's address, phase 2 the font address, and the row of
// dots is loaded into the shift register as the cell starts.  Both memories
// are synchronous, so the same sequence works with ce_pix every clock or every
// other clock.

module term_video (
	input wire clk,
	input wire ce_pix,
	input wire font_8x8,

	// screen memory read port, data valid the clock after the address: the page,
	// then memory row * 80 + column, or 1920 + column for the status line
	output reg  [11:0] ram_addr,
	input  wire [11:0] ram_data,  // underline, flash, blank, reverse, protect, character

	input wire       page,           // the page shown
	input wire [4:0] top_row,        // memory row displayed on the first text line
	input wire [6:0] cursor_x,
	input wire [4:0] cursor_y,       // memory row of the cursor
	input wire       cursor_status,  // the cursor is in the status line instead
	input wire       cursor_on,
	input wire       flash_on,       // the phase of the flashing characters
	input wire       prog_mode,      // program mode: code 00 has a picture

	output reg  hsync,
	output reg  vsync,
	output reg  hblank,
	output reg  vblank,
	output reg  video,
	output reg  half,       // the pixel is of a cell at half intensity
	output wire frame_tick  // one clock pulse per frame, for the cursor and the flashing
);

	localparam H_TOTAL = 10'd936;
	localparam H_BP    = 10'd96;
	localparam H_VIS   = 10'd640;
	localparam H_SYNC  = 10'd808;  // back porch + visible + front porch

	wire [9:0] v_total = font_8x8 ? 10'd262 : 10'd524;
	wire [9:0] v_bp = font_8x8 ? 10'd12 : 10'd73;
	wire [9:0] v_vis = font_8x8 ? 10'd200 : 10'd400;
	wire [9:0] v_sync = v_total - 10'd2;

	reg [9:0] hc = 0;
	reg [9:0] vc = 0;

	wire       h_last = (hc == H_TOTAL - 1'd1);
	wire [9:0] hc_nxt = h_last ? 10'd0 : hc + 1'd1;
	wire [9:0] vc_nxt = !h_last ? vc : (vc >= v_total - 1'd1) ? 10'd0 : vc + 1'd1;

	assign frame_tick = ce_pix & h_last & (vc >= v_total - 1'd1);

	// fonts
	reg [7:0] font16[0:4095];
	reg [7:0] font8 [0:1023];
	initial begin
		$readmemb("font/terminus_816_latin1.bin", font16);
		$readmemb("font/vt52_rom.bin", font8);
	end

	reg [11:0] font_addr;
	reg [7:0] font16_q, font8_q;
	always @(posedge clk) begin
		font16_q <= font16[font_addr];
		font8_q  <= font8[font_addr[9:0]];
	end
	wire [7:0] font_q = font_8x8 ? font8_q : font16_q;

	// position of the line being drawn: row 0 is the status line
	wire [9:0] ly = vc - v_bp;
	wire       v_active = (vc >= v_bp) && (vc < v_bp + v_vis);
	wire [4:0] row = font_8x8 ? ly[7:3] : ly[8:4];
	wire [3:0] glyph_y = font_8x8 ? {1'b0, ly[2:0]} : ly[3:0];

	// cell being fetched: one cell ahead of the beam (H_BP is a multiple of 8)
	wire [9:0] fx = hc - (H_BP - 10'd8);
	wire [6:0] fetch_col = fx[9:3];
	wire       fetch_en = v_active && (hc >= H_BP - 10'd8) && (hc < H_BP + H_VIS - 10'd8);

	wire        status_row = (row == 5'd0);
	wire [ 5:0] row_sum = {1'b0, row} + {1'b0, top_row} - 6'd1;
	wire [ 4:0] phys_row = (row_sum >= 6'd24) ? row_sum[4:0] - 5'd24 : row_sum[4:0];
	wire [10:0] row_base = status_row ? 11'd1920 : {phys_row, 6'd0} + {2'd0, phys_row, 4'd0};  // * 80

	// cell being drawn
	wire h_active = (hc >= H_BP) && (hc < H_BP + H_VIS);

	// ---- the pictures of the codes below 20 hex
	// a letter of three dots by five rows
	localparam L_A = 5'd0, L_B = 5'd1, L_C = 5'd2, L_D = 5'd3, L_E = 5'd4, L_F = 5'd5, L_G = 5'd6, L_H = 5'd7, L_K = 5'd8,
		L_L = 5'd9, L_N = 5'd10, L_O = 5'd11, L_R = 5'd12, L_S = 5'd13, L_T = 5'd14, L_U = 5'd15, L_V = 5'd16, L_1 = 5'd17,
		L_3 = 5'd18;
	function [14:0] letter;
		input [4:0] l;
		case (l)
			L_A:     letter = 15'b010_101_111_101_101;
			L_B:     letter = 15'b110_101_110_101_110;
			L_C:     letter = 15'b011_100_100_100_011;
			L_D:     letter = 15'b110_101_101_101_110;
			L_E:     letter = 15'b111_100_110_100_111;
			L_F:     letter = 15'b111_100_110_100_100;
			L_G:     letter = 15'b011_100_101_101_011;
			L_H:     letter = 15'b101_101_111_101_101;
			L_K:     letter = 15'b101_101_110_101_101;
			L_L:     letter = 15'b100_100_100_100_111;
			L_N:     letter = 15'b101_111_111_101_101;
			L_O:     letter = 15'b010_101_101_101_010;
			L_R:     letter = 15'b110_101_110_101_101;
			L_S:     letter = 15'b011_100_010_001_110;
			L_T:     letter = 15'b111_010_010_010_010;
			L_U:     letter = 15'b101_101_101_101_111;
			L_V:     letter = 15'b101_101_101_101_010;
			L_1:     letter = 15'b010_110_010_010_111;
			L_3:     letter = 15'b110_001_010_001_110;
			default: letter = 15'd0;
		endcase
	endfunction
	// the two letters of a control character's picture
	function [9:0] picture;
		input [4:0] code;
		case (code)
			5'h00:   picture = {L_N, L_U};
			5'h0C:   picture = {L_F, L_F};
			5'h0D:   picture = {L_C, L_R};
			5'h0E:   picture = {L_S, L_O};
			5'h0F:   picture = {L_S, L_T};
			5'h10:   picture = {L_E, L_T};
			5'h11:   picture = {L_D, L_1};
			5'h12:   picture = {L_A, L_K};
			5'h13:   picture = {L_D, L_3};
			5'h14:   picture = {L_B, L_L};
			5'h15:   picture = {L_N, L_K};
			5'h16:   picture = {L_B, L_S};
			5'h17:   picture = {L_H, L_T};
			5'h18:   picture = {L_C, L_N};
			5'h19:   picture = {L_L, L_F};
			5'h1A:   picture = {L_V, L_T};
			5'h1B:   picture = {L_E, L_S};
			5'h1C:   picture = {L_F, L_S};
			5'h1D:   picture = {L_G, L_S};
			5'h1E:   picture = {L_R, L_S};
			5'h1F:   picture = {L_U, L_S};
			default: picture = {L_N, L_U};
		endcase
	endfunction
	// the strokes of a line drawing character: up, down, left, right
	function [3:0] strokes;
		input [3:0] code;
		case (code)
			4'h1:    strokes = 4'b0101;
			4'h2:    strokes = 4'b0110;
			4'h3:    strokes = 4'b1001;
			4'h4:    strokes = 4'b1010;
			4'h5:    strokes = 4'b0111;
			4'h6:    strokes = 4'b1110;
			4'h7:    strokes = 4'b1101;
			4'h8:    strokes = 4'b1011;
			4'h9:    strokes = 4'b0011;
			4'hA:    strokes = 4'b1100;
			4'hB:    strokes = 4'b1111;
			default: strokes = 4'b0000;
		endcase
	endfunction

	reg  [11:0] cell_q;  // the cell that was fetched
	wire [ 6:0] code = cell_q[6:0];
	wire        low_code = (code[6:5] == 2'b00);
	wire        line_code = low_code && (code[4:0] >= 5'h01) && (code[4:0] <= 5'h0B);

	// the row of a letter that falls on this scan line, if one does: the first
	// letter is at the top left, the second lower and to the right
	wire [ 3:0] mid = font_8x8 ? 4'd3 : 4'd7;  // the scan line of a horizontal stroke
	wire [ 3:0] first_y = font_8x8 ? glyph_y : {1'b0, glyph_y[3:1]};  // row of the first letter
	wire [ 3:0] second_y = font_8x8 ? (glyph_y - 4'd3) : ({1'b0, glyph_y[3:1]} - 4'd2);
	wire [ 9:0] pic = picture(code[4:0]);
	wire [14:0] one = letter(pic[9:5]);
	wire [14:0] two = letter(pic[4:0]);
	function [2:0] letter_row;
		input [14:0] l;
		input [3:0] r;
		case (r)
			4'd0:    letter_row = l[14:12];
			4'd1:    letter_row = l[11:9];
			4'd2:    letter_row = l[8:6];
			4'd3:    letter_row = l[5:3];
			4'd4:    letter_row = l[2:0];
			default: letter_row = 3'b000;
		endcase
	endfunction
	wire [3:0] st = strokes(code[3:0]);
	reg  [7:0] drawn;
	always @* begin
		if (line_code) begin
			if (glyph_y < mid) drawn = st[3] ? 8'h10 : 8'h00;
			else if (glyph_y > mid) drawn = st[2] ? 8'h10 : 8'h00;
			else drawn = (st[1] ? 8'hF0 : 8'h00) | (st[0] ? 8'h1F : 8'h00) | ((st[3] || st[2]) ? 8'h10 : 8'h00);
		end else if ((code == 7'h00) && !prog_mode) drawn = 8'h00;
		else drawn = {letter_row(one, first_y), 1'b0, letter_row(two, second_y), 1'b0};
	end

	// the cell's row of dots, with its attributes
	wire       underline_y = font_8x8 ? (glyph_y == 4'd7) : (glyph_y == 4'd14);
	wire [7:0] dots = low_code ? drawn : font_q;
	wire       hidden = cell_q[9] || (cell_q[10] && !flash_on);  // blank, or flashing and off
	wire [7:0] shown = (hidden ? 8'h00 : dots) | {8{cell_q[11] && underline_y && !cell_q[9]}};

	reg [7:0] shift;
	reg       cell_fetched;  // the shift register will get a real cell
	reg       invert_cell;
	reg       half_cell;

	wire cursor_here = cursor_on && (fetch_col == cursor_x) && (cursor_status ? status_row : (!status_row && (phys_row == cursor_y)));

	always @(posedge clk) begin
		if (ce_pix) begin
			hc <= hc_nxt;
			vc <= vc_nxt;

			// fetch sequence, phase = hc[2:0]
			if (hc[2:0] == 3'd0) begin
				ram_addr     <= {page && !status_row, row_base + {4'd0, fetch_col}};
				cell_fetched <= fetch_en;
			end
			if (hc[2:0] == 3'd2) begin
				cell_q    <= ram_data;
				font_addr <= font_8x8 ? {2'b00, ram_data[6:0], glyph_y[2:0]} : {1'b0, ram_data[6:0], glyph_y};
			end

			if (hc[2:0] == 3'd7) begin
				shift       <= cell_fetched ? shown : 8'h00;
				// the cell about to be drawn is the one just fetched
				invert_cell <= cell_fetched && (cell_q[8] ^ cursor_here);
				half_cell   <= cell_q[7];
			end else begin
				shift <= {shift[6:0], 1'b0};
			end

			// outputs, all delayed by the same single pixel
			video  <= h_active && v_active && (shift[7] ^ invert_cell);
			half   <= half_cell;
			hblank <= ~h_active;
			vblank <= ~v_active;
			hsync  <= ~(hc >= H_SYNC);  // active low
			if (hc == H_SYNC) vsync <= ~(vc >= v_sync);  // changes only at the start of hsync
		end
	end

endmodule

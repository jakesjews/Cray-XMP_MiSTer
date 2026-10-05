// 80x24 text raster for the Cray console.
//
// Timing matches the MiSTer VT52 core so direct video works unchanged:
//   936 pixel line = 96 back porch + 640 visible + 72 front porch + 128 sync
//   8x16 font: 524 lines (81 + 384 + 57 + 2), ce_pix at 29.4 MHz -> 31.41 kHz, 59.94 Hz
//   8x8  font: 262 lines (16 + 192 + 52 + 2), ce_pix at 14.7 MHz -> 15.71 kHz, 59.94 Hz
//
// Character cells are fetched one cell (8 pixels) ahead of where they are drawn:
// phase 0 presents the character RAM address, phase 2 the font address, and the
// font row is loaded into the shift register as the cell starts.  Both memories
// are synchronous, so the same sequence works with ce_pix every clock or every
// other clock.

module term_video (
	input wire clk,
	input wire ce_pix,
	input wire font_8x8,

	// character RAM read port, data valid the clock after the address
	output reg  [10:0] ram_addr,
	input  wire [ 7:0] ram_data,

	input wire [4:0] top_row,   // physical row displayed on the top line
	input wire [6:0] cursor_x,
	input wire [4:0] cursor_y,
	input wire       cursor_on,

	output reg  hsync,
	output reg  vsync,
	output reg  hblank,
	output reg  vblank,
	output reg  video,
	output wire frame_tick  // one clock pulse per frame, for cursor blink
);

	localparam H_TOTAL = 10'd936;
	localparam H_BP    = 10'd96;
	localparam H_VIS   = 10'd640;
	localparam H_SYNC  = 10'd808;  // back porch + visible + front porch

	wire [9:0] v_total = font_8x8 ? 10'd262 : 10'd524;
	wire [9:0] v_bp = font_8x8 ? 10'd16 : 10'd81;
	wire [9:0] v_vis = font_8x8 ? 10'd192 : 10'd384;
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

	// position of the line being drawn
	wire [9:0] ly = vc - v_bp;
	wire       v_active = (vc >= v_bp) && (vc < v_bp + v_vis);
	wire [4:0] row = font_8x8 ? ly[7:3] : ly[8:4];
	wire [3:0] glyph_y = font_8x8 ? {1'b0, ly[2:0]} : ly[3:0];

	// cell being fetched: one cell ahead of the beam (H_BP is a multiple of 8)
	wire [9:0] fx = hc - (H_BP - 10'd8);
	wire [6:0] fetch_col = fx[9:3];
	wire       fetch_en = v_active && (hc >= H_BP - 10'd8) && (hc < H_BP + H_VIS - 10'd8);

	wire [ 5:0] row_sum = {1'b0, row} + {1'b0, top_row};
	wire [ 4:0] phys_row = (row_sum >= 6'd24) ? row_sum[4:0] - 5'd24 : row_sum[4:0];
	wire [10:0] row_base = {phys_row, 6'd0} + {2'd0, phys_row, 4'd0};  // * 80

	// cell being drawn
	wire h_active = (hc >= H_BP) && (hc < H_BP + H_VIS);

	reg [7:0] shift;
	reg       cell_fetched;  // the shift register will get a real cell
	reg       cursor_cell;

	always @(posedge clk) begin
		if (ce_pix) begin
			hc <= hc_nxt;
			vc <= vc_nxt;

			// fetch sequence, phase = hc[2:0]
			if (hc[2:0] == 3'd0) begin
				ram_addr     <= row_base + {4'd0, fetch_col};
				cell_fetched <= fetch_en;
			end
			if (hc[2:0] == 3'd2) begin
				font_addr <= font_8x8 ? {2'b00, ram_data[6:0], glyph_y[2:0]} : {ram_data, glyph_y};
			end

			if (hc[2:0] == 3'd7) begin
				shift       <= cell_fetched ? font_q : 8'h00;
				// the cell about to be drawn is the one just fetched
				cursor_cell <= cell_fetched && cursor_on && (fetch_col == cursor_x) && (row == cursor_y);
			end else begin
				shift <= {shift[6:0], 1'b0};
			end

			// outputs, all delayed by the same single pixel
			video  <= h_active && v_active && (shift[7] ^ cursor_cell);
			hblank <= ~h_active;
			vblank <= ~v_active;
			hsync  <= ~(hc >= H_SYNC);  // active low
			if (hc == H_SYNC) vsync <= ~(vc >= v_sync);  // changes only at the start of hsync
		end
	end

endmodule

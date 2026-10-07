// Simulation stand-ins for the framework's video modules.  The picture passes
// through a register, as it does in video_mixer with the scandoubler off, and
// video_freak neither crops nor scales.

module video_mixer #(
	parameter LINE_LENGTH = 768,
	parameter HALF_DEPTH  = 0,
	parameter GAMMA       = 0
) (
	input  wire CLK_VIDEO,
	output reg  CE_PIXEL,

	input wire ce_pix,

	input wire scandoubler,
	input wire hq2x,

	inout wire [21:0] gamma_bus,

	input wire [7:0] R,
	input wire [7:0] G,
	input wire [7:0] B,

	input wire HSync,
	input wire VSync,
	input wire HBlank,
	input wire VBlank,

	input  wire HDMI_FREEZE,
	output wire freeze_sync,

	output reg [7:0] VGA_R,
	output reg [7:0] VGA_G,
	output reg [7:0] VGA_B,
	output reg       VGA_VS,
	output reg       VGA_HS,
	output reg       VGA_DE
);

	assign freeze_sync = 1'b0;

	always @(posedge CLK_VIDEO) begin
		CE_PIXEL <= ce_pix;
		if (ce_pix) begin
			VGA_R  <= R;
			VGA_G  <= G;
			VGA_B  <= B;
			VGA_HS <= HSync;
			VGA_VS <= VSync;
			VGA_DE <= ~(HBlank | VBlank);
		end
	end

endmodule

module video_freak (
	input  wire        CLK_VIDEO,
	input  wire        CE_PIXEL,
	input  wire        VGA_VS,
	input  wire [11:0] HDMI_WIDTH,
	input  wire [11:0] HDMI_HEIGHT,
	output wire        VGA_DE,
	output reg  [12:0] VIDEO_ARX,
	output reg  [12:0] VIDEO_ARY,

	input wire        VGA_DE_IN,
	input wire [11:0] ARX,
	input wire [11:0] ARY,
	input wire [11:0] CROP_SIZE,
	input wire [ 4:0] CROP_OFF,
	input wire [ 2:0] SCALE
);

	assign VGA_DE = VGA_DE_IN;

	always @(posedge CLK_VIDEO) begin
		VIDEO_ARX <= {1'b0, ARX};
		VIDEO_ARY <= {1'b0, ARY};
	end

endmodule

// Simulation stand-in for sys/hps_io.sv.  The C++ harness pokes the sim_* registers.
module hps_io #(
	parameter CONF_STR      = "",
	parameter CONF_STR_BRAM = 0,
	parameter PS2DIV        = 0,
	parameter WIDE          = 0,
	parameter VDNUM         = 1,
	parameter BLKSZ         = 2,
	parameter PS2WE         = 0
) (
	input wire        clk_sys,
	inout wire [45:0] HPS_BUS,
	inout wire [35:0] EXT_BUS,
	inout wire [21:0] gamma_bus,

	output wire         forced_scandoubler,
	output wire         direct_video,
	output wire [  1:0] buttons,
	output wire [127:0] status,
	input  wire [ 15:0] status_menumask,

	output wire        ioctl_download,
	output wire [15:0] ioctl_index,
	output wire        ioctl_wr,
	output wire [26:0] ioctl_addr,
	output wire [ 7:0] ioctl_dout,
	input  wire        ioctl_wait,

	output wire [10:0] ps2_key,

	output wire [VDNUM-1:0] img_mounted,
	output wire [     63:0] img_size,
	output wire             img_readonly,

	input  wire [     31:0] sd_lba      [VDNUM],
	input  wire [      5:0] sd_blk_cnt  [VDNUM],
	input  wire [VDNUM-1:0] sd_rd,
	input  wire [VDNUM-1:0] sd_wr,
	output wire [VDNUM-1:0] sd_ack,
	output wire [     13:0] sd_buff_addr,
	output wire [      7:0] sd_buff_dout,
	input  wire [      7:0] sd_buff_din [VDNUM],
	output wire             sd_buff_wr
);

	reg [127:0] sim_status  /* verilator public_flat_rw */ = 0;
	reg [  1:0] sim_buttons  /* verilator public_flat_rw */ = 0;
	reg [ 10:0] sim_ps2_key  /* verilator public_flat_rw */ = 0;
	reg         sim_download  /* verilator public_flat_rw */ = 0;
	reg         sim_direct_video  /* verilator public_flat_rw */ = 0;

	assign status             = sim_status;
	assign buttons            = sim_buttons;
	assign ps2_key            = sim_ps2_key;
	assign ioctl_download     = sim_download;
	assign direct_video       = sim_direct_video;
	assign forced_scandoubler = 0;
	assign ioctl_index        = 0;
	assign ioctl_wr           = 0;
	assign ioctl_addr         = 0;
	assign ioctl_dout         = 0;

	// the disk requests: what the core asks for, one field a disk, and what the
	// harness answers
	wire [32*VDNUM-1:0] sim_sd_lba  /* verilator public_flat_rd */;
	wire [ 8*VDNUM-1:0] sim_sd_blk_cnt  /* verilator public_flat_rd */;
	wire [ 8*VDNUM-1:0] sim_sd_din  /* verilator public_flat_rd */;
	wire [   VDNUM-1:0] sim_sd_rd  /* verilator public_flat_rd */ = sd_rd;
	wire [   VDNUM-1:0] sim_sd_wr  /* verilator public_flat_rd */ = sd_wr;
	reg  [   VDNUM-1:0] sim_sd_ack  /* verilator public_flat_rw */ = 0;
	reg  [        13:0] sim_sd_buff_addr  /* verilator public_flat_rw */ = 0;
	reg  [         7:0] sim_sd_buff_dout  /* verilator public_flat_rw */ = 0;
	reg                 sim_sd_buff_wr  /* verilator public_flat_rw */ = 0;

	genvar g;
	generate
		for (g = 0; g < VDNUM; g = g + 1) begin : g_sd
			assign sim_sd_lba[32*g+:32]   = sd_lba[g];
			assign sim_sd_blk_cnt[8*g+:8] = {2'b00, sd_blk_cnt[g]};
			assign sim_sd_din[8*g+:8]     = sd_buff_din[g];
		end
	endgenerate

	// an image is mounted: its bit for a clock, with its length in bytes
	reg [VDNUM-1:0] sim_img_mounted  /* verilator public_flat_rw */ = 0;
	reg [     63:0] sim_img_size  /* verilator public_flat_rw */ = 0;
	assign img_mounted = sim_img_mounted;
	assign img_size    = sim_img_size;
	reg sim_img_readonly  /* verilator public_flat_rw */ = 0;
	assign img_readonly = sim_img_readonly;

	assign sd_ack       = sim_sd_ack;
	assign sd_buff_addr = sim_sd_buff_addr;
	assign sd_buff_dout = sim_sd_buff_dout;
	assign sd_buff_wr   = sim_sd_buff_wr;

	wire unused = &{1'b0, clk_sys, status_menumask, ioctl_wait};

endmodule

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

	output wire [10:0] ps2_key
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

	wire unused = &{1'b0, clk_sys, status_menumask, ioctl_wait};

endmodule

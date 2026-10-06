// The bridges between the CPU's clock and the I/O Subsystem's
// (rtl/xmp_bridge.v), for sim/harness/bridge_main.cpp: a pulse each way, a
// level, and a memory port.
module xmp_bridge_tb (
	input wire clk,
	input wire clk_ios,
	input wire rst,
	input wire rst_ios,

	// in clk
	input  wire i_pulse_f,
	output wire o_pulse_f,
	input  wire i_level_f,

	// in clk_ios
	input  wire i_pulse_s,
	output wire o_pulse_s,
	output wire o_level_s,

	input  wire        i_req,
	input  wire        i_we,
	input  wire [23:0] i_addr,
	input  wire [63:0] i_wdata,
	output wire        o_ack,
	output wire [63:0] o_rdata,

	output wire        o_req,
	output wire        o_we,
	output wire [23:0] o_addr,
	output wire [63:0] o_wdata,
	input  wire        i_ack,
	input  wire [63:0] i_rdata
);

	xmp_pulse to_ios (
		.clk_from(clk),
		.i_pulse (i_pulse_f),
		.clk_to  (clk_ios),
		.o_pulse (o_pulse_s)
	);

	xmp_pulse to_cpu (
		.clk_from(clk_ios),
		.i_pulse (i_pulse_s),
		.clk_to  (clk),
		.o_pulse (o_pulse_f)
	);

	xmp_sync level (
		.clk(clk_ios),
		.i_d(i_level_f),
		.o_q(o_level_s)
	);

	xmp_mem_bridge #(
		.AW(24)
	) mem (
		.clk    (clk),
		.clk_ios(clk_ios),
		.rst    (rst),
		.rst_ios(rst_ios),
		.i_req  (i_req),
		.i_we   (i_we),
		.i_addr (i_addr),
		.i_wdata(i_wdata),
		.o_ack  (o_ack),
		.o_rdata(o_rdata),
		.o_req  (o_req),
		.o_we   (o_we),
		.o_addr (o_addr),
		.o_wdata(o_wdata),
		.i_ack  (i_ack),
		.i_rdata(i_rdata)
	);

endmodule

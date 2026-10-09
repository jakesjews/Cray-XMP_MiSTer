// Simulation stand-ins for the Altera PLLs.  The video clock is the test bench
// clock; the CPU's clock and the I/O Subsystem's are toggled by the C++ harness
// through the sim_clk of their stand-ins.
module pll (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire locked
);
	assign outclk_0 = refclk;
	assign locked   = ~rst;
endmodule

module pll_cpu (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire locked
);
	reg sim_clk  /* verilator public_flat_rw */ = 0;
	assign outclk_0 = sim_clk;
	assign locked   = ~rst;
endmodule

module pll_ios (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire locked
);
	reg sim_clk  /* verilator public_flat_rw */ = 0;
	assign outclk_0 = sim_clk;
	assign locked   = ~rst;
endmodule

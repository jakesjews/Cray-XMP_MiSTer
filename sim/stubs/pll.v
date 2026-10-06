// Simulation stand-ins for the Altera PLLs.  The video clock is the test bench
// clock; the CPU's clock and the I/O Subsystem's are toggled by the C++ harness
// through sim_clk1 and sim_clk.
module pll (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire outclk_1,
	output wire locked
);
	reg sim_clk1  /* verilator public_flat_rw */ = 0;
	assign outclk_0 = refclk;
	assign outclk_1 = sim_clk1;
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

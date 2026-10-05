// Simulation stand-in for the Altera PLL.  The video clock is the test bench
// clock; the machine's clock is toggled by the C++ harness through sim_clk1.
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

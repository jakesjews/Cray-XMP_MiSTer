// Simulation stand-in for the Altera PLL: the system clock is the test bench clock.
module pll (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire locked
);
	assign outclk_0 = refclk;
	assign locked   = ~rst;
endmodule

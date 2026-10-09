// Lint-only interfaces matching rtl/pll.v, rtl/pll_cpu.v and rtl/pll_ios.v. Quartus uses the
// generated PLL; this intentionally has no implementation and must not be used for simulation.
module pll (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire locked
);
endmodule

module pll_cpu (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire locked
);
endmodule

module pll_ios (
	input  wire refclk,
	input  wire rst,
	output wire outclk_0,
	output wire locked
);
endmodule

// The clock buffer that puts the CPU PLL's reference on a global clock line.
module cyclonev_clkena #(
	parameter clock_type        = "Auto",
	parameter ena_register_mode = "always enabled"
) (
	input  wire inclk,
	input  wire ena,
	output wire enaout,
	output wire outclk
);
	assign outclk = inclk;
	assign enaout = ena;
endmodule

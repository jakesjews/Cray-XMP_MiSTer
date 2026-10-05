// The monitor program that dead start copies into memory.  monitor.mem is made
// from software/monitor/monitor.cal by tools/py/mkrom.py: one 64-bit word per
// line, word 0 first.
module boot_rom #(
	parameter AW = 12
) (
	input  wire          clk,
	input  wire [AW-1:0] addr,
	output reg  [  63:0] q
);

	reg [63:0] rom[0:(1<<AW)-1];

	initial begin
`ifdef VERILATOR
		$readmemh("../boot/monitor.mem", rom);  // simulations run in rtl/terminal
`else
		$readmemh("monitor.mem", rom);
`endif
	end

	always @(posedge clk) q <= rom[addr];

endmodule

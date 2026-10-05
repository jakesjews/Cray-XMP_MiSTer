//******************************************
//       Vector Add Unit
//******************************************
//
// 64-bit integer sums and differences of vector elements for instructions 154
// to 157.  A pure pipeline with a unit time of three clocks.  Operand a is (Sj)
// or the Vj element, operand b the Vk element; a difference is a minus b.  No
// overflow is detected (manual 3-17).

module vector_add (
	input  wire        clk,
	input  wire        i_sub,
	input  wire [63:0] i_a,
	input  wire [63:0] i_b,
	output reg  [63:0] o_result
);

	reg sub;
	reg [63:0] a, b, sum;

	always @(posedge clk) begin
		sub      <= i_sub;
		a        <= i_a;
		b        <= i_b;
		sum      <= sub ? (a - b) : (a + b);
		o_result <= sum;
	end

endmodule

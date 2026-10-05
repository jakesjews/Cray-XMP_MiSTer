//******************************************
//       Vector Logical Unit
//******************************************
//
// Bit-by-bit logic on 64-bit elements for instructions 140 to 147, and the
// element test of the vector mask instruction 175.  A pure pipeline with a unit
// time of two clocks: one operand pair in per clock, the result two clocks later.
//
//   i_op  0, 1   140, 141   logical product   a & b
//         2, 3   142, 143   logical sum       a | b
//         4, 5   144, 145   logical difference a ^ b
//         6, 7   146, 147   merge: a where the element's mask bit is one, else b
//
// Operand a is (Sj) for the even instructions and the Vj element for the odd
// ones; the caller chooses.  Operand b is the Vk element.
//
// o_test is the 175 test of operand a (manual 4-68): i_test 0 zero, 1 not zero,
// 2 positive (zero counts as positive), 3 negative.

module vector_logical (
	input  wire        clk,
	input  wire [ 2:0] i_op,
	input  wire [ 1:0] i_test,
	input  wire [63:0] i_a,
	input  wire [63:0] i_b,
	input  wire        i_vm_bit,
	output reg  [63:0] o_result,
	output reg         o_test
);

	reg [2:0] op;
	reg [1:0] test;
	reg [63:0] a, b;
	reg vm_bit;

	always @(posedge clk) begin
		op     <= i_op;
		test   <= i_test;
		a      <= i_a;
		b      <= i_b;
		vm_bit <= i_vm_bit;

		case (op[2:1])
			2'd0: o_result <= a & b;
			2'd1: o_result <= a | b;
			2'd2: o_result <= a ^ b;
			2'd3: o_result <= vm_bit ? a : b;
		endcase

		case (test)
			2'd0: o_test <= (a == 64'd0);
			2'd1: o_test <= (a != 64'd0);
			2'd2: o_test <= ~a[63];
			2'd3: o_test <= a[63];
		endcase
	end

endmodule

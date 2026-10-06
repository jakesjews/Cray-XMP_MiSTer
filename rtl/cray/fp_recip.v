//******************************************
//   Floating Point Reciprocal Approximation Unit
//******************************************
//
// Reciprocal approximation of a Cray floating point word (instructions 070 and
// 174).  A pure pipeline: the operand in during one clock, the result out
// fourteen clocks later, a new operation every clock.  The first of the fourteen
// clocks only takes the operand into a register of the unit's own.
//
// No Cray manual gives this unit at bit level.  It follows the reference model
// in tools/crates/fp/src/recip.rs bit for bit; that model's behaviour comes from
// the software model of the unit in the Cray X-MP diagnostics.  With B the
// coefficient as a fraction in [1/2, 1):
//
//   A0 = round(2^15 / (128.5 + i))      i = coefficient bits 46:40 (a 128-entry table)
//   A1 = 2*A0 - A0^2 * B                on the upper 24 coefficient bits, kept to 18 bits
//   A2 = 2*A1 - A1^2 * B                on the upper 37 coefficient bits, kept to 33 bits
//
// Both multiplies drop low-order bits row by row, in the pattern below.  The
// result exponent is 0100001 minus the operand exponent.  An operand exponent
// outside 020002 to 057777 is a range error: exponent 060000 and bit 47 of the
// coefficient cleared.  The operand is assumed normalised.

// The delay lines of this unit stay flip-flops: as memory blocks they were slow to
// read and sat away from the logic around them.
(* altera_attribute = "-name AUTO_SHIFT_REGISTER_RECOGNITION OFF" *)
module fp_recip (
	input  wire        clk,
	input  wire [63:0] i_a,
	output reg  [63:0] o_result,
	output reg         o_range_err,
	output wire [63:0] o_early       // the result during the clock before o_result has it
);

	localparam [14:0] EXP_OVERFLOW = 15'o60000;

	// ---- stage 1: the operand ----
	reg [63:0] a;
	always @(posedge clk) a <= i_a;

	// ---- stage 2: first approximation from the table ----
	reg [7:0] seed[0:127];
	integer n, q;
	initial
		for (n = 0; n < 128; n = n + 1) begin
			q       = ((1 << 17) + (257 + 2 * n)) / (2 * (257 + 2 * n));
			seed[n] = q[7:0];
		end

	reg [ 7:0] s2_a0;
	reg [47:0] s2_coef;
	always @(posedge clk) begin
		s2_a0   <= seed[a[46:40]];
		s2_coef <= a[47:0];
	end

	// ---- stage 3: A0 squared ----
	reg [ 7:0] s3_a0;
	reg [15:0] s3_a0sq;
	reg [47:0] s3_coef;
	always @(posedge clk) begin
		s3_a0   <= s2_a0;
		s3_a0sq <= s2_a0 * s2_a0;
		s3_coef <= s2_coef;
	end

	// ---- stages 4 and 5: A0^2 * B on the upper 24 bits; row k is cut to b24 >> (15-k) ----
	wire [23:0] b24 = s3_coef[47:24];

	function [27:0] row1;
		input [23:0] b;
		input [15:0] sq;
		input integer k;
		begin
			row1 = sq[k] ? {4'd0, b >> (15 - k)} : 28'd0;
		end
	endfunction

	reg     [27:0] s4_part [0:3];
	reg     [ 7:0] s4_a0;
	reg     [47:0] s4_coef;
	integer        g;
	always @(posedge clk) begin
		for (g = 0; g < 4; g = g + 1)
		s4_part[g] <= row1(
			b24, s3_a0sq, 4 * g
		) + row1(
			b24, s3_a0sq, 4 * g + 1
		) + row1(
			b24, s3_a0sq, 4 * g + 2
		) + row1(
			b24, s3_a0sq, 4 * g + 3
		);
		s4_a0   <= s3_a0;
		s4_coef <= s3_coef;
	end

	reg [27:0] s5_prod;
	reg [ 7:0] s5_a0;
	reg [47:0] s5_coef;
	always @(posedge clk) begin
		s5_prod <= s4_part[0] + s4_part[1] + s4_part[2] + s4_part[3];
		s5_a0   <= s4_a0;
		s5_coef <= s4_coef;
	end

	// ---- stage 6: A1 = (A0 * (2^17 + 1) - product) >> 6, 18 bits ----
	wire [27:0] a0_term = {3'd0, s5_a0, 17'd0} + {20'd0, s5_a0};
	wire [27:0] a1_wide = a0_term - s5_prod;
	reg  [17:0] s6_a1;
	reg  [47:0] s6_coef;
	always @(posedge clk) begin
		s6_a1   <= a1_wide[23:6];
		s6_coef <= s5_coef;
	end

	// ---- stage 7: A1 squared ----
	reg [17:0] s7_a1;
	reg [35:0] s7_a1sq;
	reg [47:0] s7_coef;
	always @(posedge clk) begin
		s7_a1   <= s6_a1;
		s7_a1sq <= s6_a1 * s6_a1;
		s7_coef <= s6_coef;
	end

	// ---- stages 8 to 10: A1^2 * B on the upper 37 bits (bit 47 taken as set) ----
	// Row n is (wide >> (35 - n)) with its low "drop" bits cleared; the rows are
	// summed in seven groups and each group sum has low bits cleared again.
	//   group  rows    bits dropped per row       bits dropped from the group sum
	//     0    0-7     2 2 - 2 2 2 2 2  (row 2 is not formed)       3
	//     1    8-11    1 0 0 0                                      2
	//     2    12-15   2 2 3 4                                      0
	//     3    16-23   2 3 2 2 2 2 2 2                              3
	//     4    24-27   1 0 0 0                                      2
	//     5    28-31   2 2 3 4                                      0
	//     6    32-35   2 2 2 2                                      3
	// No stage adds more than four numbers: stage 8 sums the rows by fours (the two
	// groups of eight rows as two halves each), stage 9 joins the halves and adds
	// the groups in pairs, stage 10 adds what is left.
	wire [39:0] wide = {1'b1, s7_coef[46:11], 3'd0};

	function [45:0] row2;
		input [39:0] w;
		input [35:0] sq;
		input integer bitn;
		input integer drop;
		reg [39:0] shifted;
		begin
			shifted = w >> (35 - bitn);
			row2    = sq[bitn] ? {6'd0, (shifted >> drop) << drop} : 46'd0;
		end
	endfunction

	function [45:0] cut;
		input [45:0] v;
		input integer drop;
		begin
			cut = (v >> drop) << drop;
		end
	endfunction

	reg [45:0] s8_g0a, s8_g0b, s8_g1, s8_g2, s8_g3a, s8_g3b, s8_g4, s8_g5, s8_g6;
	reg [17:0] s8_a1;
	always @(posedge clk) begin
		s8_g0a <= row2(
			wide, s7_a1sq, 0, 2
		) + row2(
			wide, s7_a1sq, 1, 2
		) + row2(
			wide, s7_a1sq, 3, 2
		) + row2(
			wide, s7_a1sq, 4, 2
		);
		s8_g0b <= row2(wide, s7_a1sq, 5, 2) + row2(wide, s7_a1sq, 6, 2) + row2(wide, s7_a1sq, 7, 2);
		s8_g1 <= cut(
			row2(
				wide, s7_a1sq, 8, 1
			) + row2(
				wide, s7_a1sq, 9, 0
			) + row2(
				wide, s7_a1sq, 10, 0
			) + row2(
				wide, s7_a1sq, 11, 0
			),
			2
		);
		s8_g2 <= cut(
			row2(
				wide, s7_a1sq, 12, 2
			) + row2(
				wide, s7_a1sq, 13, 2
			) + row2(
				wide, s7_a1sq, 14, 3
			) + row2(
				wide, s7_a1sq, 15, 4
			),
			0
		);
		s8_g3a <= row2(
			wide, s7_a1sq, 16, 2
		) + row2(
			wide, s7_a1sq, 17, 3
		) + row2(
			wide, s7_a1sq, 18, 2
		) + row2(
			wide, s7_a1sq, 19, 2
		);
		s8_g3b <= row2(
			wide, s7_a1sq, 20, 2
		) + row2(
			wide, s7_a1sq, 21, 2
		) + row2(
			wide, s7_a1sq, 22, 2
		) + row2(
			wide, s7_a1sq, 23, 2
		);
		s8_g4 <= cut(
			row2(
				wide, s7_a1sq, 24, 1
			) + row2(
				wide, s7_a1sq, 25, 0
			) + row2(
				wide, s7_a1sq, 26, 0
			) + row2(
				wide, s7_a1sq, 27, 0
			),
			2
		);
		s8_g5 <= cut(
			row2(
				wide, s7_a1sq, 28, 2
			) + row2(
				wide, s7_a1sq, 29, 2
			) + row2(
				wide, s7_a1sq, 30, 3
			) + row2(
				wide, s7_a1sq, 31, 4
			),
			0
		);
		s8_g6 <= cut(
			row2(
				wide, s7_a1sq, 32, 2
			) + row2(
				wide, s7_a1sq, 33, 2
			) + row2(
				wide, s7_a1sq, 34, 2
			) + row2(
				wide, s7_a1sq, 35, 2
			),
			3
		);
		s8_a1 <= s7_a1;
	end

	reg [45:0] s9_low, s9_high, s9_g6;  // groups 0 to 2, groups 3 to 5
	reg [17:0] s9_a1;
	always @(posedge clk) begin
		s9_low  <= cut(s8_g0a + s8_g0b, 3) + (s8_g1 + s8_g2);
		s9_high <= cut(s8_g3a + s8_g3b, 3) + (s8_g4 + s8_g5);
		s9_g6   <= s8_g6;
		s9_a1   <= s8_a1;
	end

	reg [45:0] s10_sum;
	reg [17:0] s10_a1;
	always @(posedge clk) begin
		s10_sum <= s9_low + s9_high + s9_g6;
		s10_a1  <= s9_a1;
	end

	// ---- stage 11: round at bit 3, then drop six bits ----
	wire [41:0] rounded = s10_sum[45:4] + {41'd0, s10_sum[3]};
	reg  [39:0] s11_prod;
	reg  [17:0] s11_a1;
	always @(posedge clk) begin
		s11_prod <= rounded[41:2];
		s11_a1   <= s10_a1;
	end

	// ---- stage 12: A2 = (A1 << 31) - (product << 14) - 1; keep 33 bits with bit 47 forced ----
	wire [63:0] a2 = {15'd0, s11_a1, 31'd0} - {10'd0, s11_prod, 14'd0} - 64'd1;
	reg  [47:0] s12_coef;
	always @(posedge clk) s12_coef <= {1'b1, a2[46:15], 15'd0};

	// ---- sign and exponent travel beside the coefficient ----
	reg            se_sign[2:12];
	reg     [14:0] se_exp [2:12];
	integer        m;
	always @(posedge clk) begin
		se_sign[2] <= a[63];
		se_exp[2]  <= a[62:48];
		for (m = 3; m <= 12; m = m + 1) begin
			se_sign[m] <= se_sign[m-1];
			se_exp[m]  <= se_exp[m-1];
		end
	end

	// ---- stage 13: exponent and range ----
	reg  [63:0] s13_result;
	reg         s13_err;
	wire        range = (se_exp[12] <= 15'o20001) || (se_exp[12] >= EXP_OVERFLOW);
	wire [15:0] exp_out = 16'o100001 - {1'b0, se_exp[12]};
	always @(posedge clk) begin
		s13_err <= range;
		s13_result <= range ? {se_sign[12], EXP_OVERFLOW, 1'b0, s12_coef[46:0]}
						: {se_sign[12], exp_out[14:0], s12_coef};
	end

	// ---- stage 14: makes up the unit's fourteen clock periods ----
	always @(posedge clk) begin
		o_result    <= s13_result;
		o_range_err <= s13_err;
	end

	assign o_early = s13_result;

endmodule

//******************************************
//       Floating Point Multiply Unit
//******************************************
//
// Floating product of two Cray floating point words (instructions 064 to 067
// and 160 to 167).  A pure pipeline: operands in during one clock, the result
// out seven clocks later, a new operation every clock.
//
//   i_kind 0  064  full product
//          1  065  half-precision rounded product
//          2  066  rounded product
//          3  067  reciprocal iteration, 2 - a*b
//
// The multiplier is the truncated pyramid of the Cray manuals: the symmetric
// unit the CRAY-1 had from 1980 (change packet E-01 of its manual), which the
// X-MP manuals describe in the same words (HR-0097B figure 4-10 and page 4-30).
// The reference model is tools/crates/fp/src/mul.rs, which this unit matches
// bit for bit.  With the coefficients as fractions,
// the logical product of bit 2^-p of one and bit 2^-q of the other belongs to
// column p+q.  Only columns 2 to 56 are formed; everything to the right is
// never generated, so each row of the pyramid is cut off at 2^-56.  A constant
// of nine units of 2^-56 makes up for the average of what was cut, and the
// rounded forms add round bits in the same adder.
//
// Exponents: both zero is an integer multiply (no normalising shift, exponent
// zero); exactly one zero gives an all-zero word; an operand exponent of 060000
// or more, or an unshifted product exponent that reaches 060000, is a range
// error; a product exponent below 020000 gives an all-zero word.

module fp_mul (
	input  wire        clk,
	input  wire [63:0] i_a,
	input  wire [63:0] i_b,
	input  wire [ 1:0] i_kind,
	output reg  [63:0] o_result,
	output reg         o_range_err
);

	localparam [14:0] EXP_BIAS     = 15'o40000;
	localparam [14:0] EXP_MIN      = 15'o20000;
	localparam [14:0] EXP_OVERFLOW = 15'o60000;

	localparam [1:0] K_FULL = 2'd0, K_HALF = 2'd1, K_ROUND = 2'd2, K_2M = 2'd3;

	// ---- the pyramid: 48 rows in units of 2^-56, each cut off at the unit ----
	wire [47:0] a_coef = i_a[47:0];
	wire [47:0] b_coef = i_b[47:0];
	wire [54:0] a_wide = {a_coef, 7'd0};

	// row q (q = 1 is the most significant bit of b) is a_wide >> (q - 1)
	function [55:0] row;
		input [54:0] wide;
		input [47:0] b;
		input integer q;
		begin
			row = b[48-q] ? {1'b0, wide >> (q - 1)} : 56'd0;
		end
	endfunction

	// stage 1: 48 rows to 16 sums
	reg     [55:0] s1_sum[0:15];
	integer        g;
	always @(posedge clk)
		for (g = 0; g < 16; g = g + 1)
			s1_sum[g] <= row(
				a_wide, b_coef, 3 * g + 1
			) + row(
				a_wide, b_coef, 3 * g + 2
			) + row(
				a_wide, b_coef, 3 * g + 3
			);

	// stage 2: 16 sums to 6
	reg [55:0] s2_sum[0:5];
	always @(posedge clk) begin
		for (g = 0; g < 5; g = g + 1) s2_sum[g] <= s1_sum[3*g] + s1_sum[3*g+1] + s1_sum[3*g+2];
		s2_sum[5] <= s1_sum[15];
	end

	// stage 3: 6 sums to 2
	reg [55:0] s3_sum[0:1];
	always @(posedge clk) begin
		s3_sum[0] <= s2_sum[0] + s2_sum[1] + s2_sum[2];
		s3_sum[1] <= s2_sum[3] + s2_sum[4] + s2_sum[5];
	end

	// ---- control travelling beside the pyramid ----
	reg c1_neg, c2_neg, c3_neg, c4_neg, c5_neg, c6_neg;
	reg [14:0] c1_ea, c1_eb, c2_ea, c2_eb, c3_ea, c3_eb, c4_ea, c4_eb, c5_ea, c5_eb;
	reg [1:0] c1_kind, c2_kind, c3_kind, c4_kind;

	always @(posedge clk) begin
		c1_neg  <= i_a[63] ^ i_b[63];
		c2_neg  <= c1_neg;
		c3_neg  <= c2_neg;
		c4_neg  <= c3_neg;
		c5_neg  <= c4_neg;
		c6_neg  <= c5_neg;
		c1_ea   <= i_a[62:48];
		c2_ea   <= c1_ea;
		c3_ea   <= c2_ea;
		c4_ea   <= c3_ea;
		c5_ea   <= c4_ea;
		c1_eb   <= i_b[62:48];
		c2_eb   <= c1_eb;
		c3_eb   <= c2_eb;
		c4_eb   <= c3_eb;
		c5_eb   <= c4_eb;
		c1_kind <= i_kind;
		c2_kind <= c1_kind;
		c3_kind <= c2_kind;
		c4_kind <= c3_kind;
	end

	// stage 4: the last add, with the constants.  The compensation is 9; the round
	// bits are 2^-50 and 2^-51 (066) or 2^-31 and 2^-32 (065).  For 067 the unit
	// forms 198 minus the pyramid sum, which is ~x + ~y + 200.  That is what
	// Cray's own simulation of the unit comes to (see TwoMinus::Cray in mul.rs):
	// it adds one bits at 2^-2 to 2^-48 and 2^-51 + 2^-52 to the sum and its
	// constant, then complements everything below 2^-1.
	reg [55:0] s4_out;
	reg [55:0] konst;
	always @(*) begin
		case (c3_kind)
			K_HALF:  konst = 56'd9 + (56'd1 << 25) + (56'd1 << 24);
			K_ROUND: konst = 56'd9 + (56'd1 << 6) + (56'd1 << 5);
			K_2M:    konst = 56'd200;
			K_FULL:  konst = 56'd9;
		endcase
	end
	always @(posedge clk)
		s4_out <= (c3_kind == K_2M) ? (~s3_sum[0] + ~s3_sum[1] + konst) : (s3_sum[0] + s3_sum[1] + konst);

	// stage 5: normalise by at most one place, and cut the half-precision result to 29 bits
	wire        integer_mul = (c4_ea == 15'd0) && (c4_eb == 15'd0);
	wire        no_shift = integer_mul || s4_out[55];
	wire [47:0] coef_full = no_shift ? s4_out[55:8] : s4_out[54:7];

	reg [47:0] s5_coef;
	reg        s5_no_shift;
	always @(posedge clk) begin
		s5_coef     <= (c4_kind == K_HALF) ? {coef_full[47:19], 19'd0} : coef_full;
		s5_no_shift <= no_shift;
	end

	// stage 6: exponent and its tests (made on the exponent before the normalising shift)
	wire signed [16:0] e = $signed({2'b00, c5_ea}) + $signed({2'b00, c5_eb}) - $signed({2'b00, EXP_BIAS});

	reg [47:0] s6_coef;
	reg [14:0] s6_exp;
	reg s6_integer, s6_one_zero, s6_ovf, s6_unf;
	always @(posedge clk) begin
		s6_coef     <= s5_coef;
		s6_integer  <= (c5_ea == 15'd0) && (c5_eb == 15'd0);
		s6_one_zero <= (c5_ea == 15'd0) != (c5_eb == 15'd0);
		s6_ovf      <= (c5_ea >= EXP_OVERFLOW) || (c5_eb >= EXP_OVERFLOW) || (e >= $signed({2'b00, EXP_OVERFLOW}));
		s6_unf      <= (e < $signed({2'b00, EXP_MIN}));
		s6_exp      <= s5_no_shift ? e[14:0] : (e[14:0] - 15'd1);
	end

	// stage 7: pick the result
	always @(posedge clk) begin
		if (s6_one_zero) begin
			o_result    <= 64'd0;
			o_range_err <= 1'b0;
		end else if (s6_integer) begin
			o_result    <= {c6_neg, 15'd0, s6_coef};
			o_range_err <= 1'b0;
		end else if (s6_ovf) begin
			o_result    <= {c6_neg, EXP_OVERFLOW, s6_coef};
			o_range_err <= 1'b1;
		end else if (s6_unf) begin
			o_result    <= 64'd0;
			o_range_err <= 1'b0;
		end else begin
			o_result    <= {c6_neg, s6_exp, s6_coef};
			o_range_err <= 1'b0;
		end
	end

endmodule

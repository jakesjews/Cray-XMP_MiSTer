//******************************************
//       Floating Point Multiply Unit
//******************************************
//
// Floating product of two Cray floating point words (instructions 064 to 067
// and 160 to 167).  A pure pipeline: operands in during one clock, the result
// out seven clocks later, a new operation every clock.  The first of the seven
// clocks only takes the operands into registers of the unit's own: where they
// come from is the far end of the longest way in the CPU, and nothing more
// should be asked of it.
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
	output reg         o_range_err,
	output wire [63:0] o_early       // the result during the clock before o_result has it
);

	localparam [14:0] EXP_BIAS     = 15'o40000;
	localparam [14:0] EXP_MIN      = 15'o20000;
	localparam [14:0] EXP_OVERFLOW = 15'o60000;

	localparam [1:0] K_FULL = 2'd0, K_HALF = 2'd1, K_ROUND = 2'd2, K_2M = 2'd3;

	// ---- stage 1: the operands ----
	reg [63:0] a, b;
	reg [1:0] kind;
	always @(posedge clk) begin
		a    <= i_a;
		b    <= i_b;
		kind <= i_kind;
	end

	// ---- the pyramid: 48 rows in units of 2^-56, each cut off at the unit ----
	wire [47:0] a_coef = a[47:0];
	wire [47:0] b_coef = b[47:0];
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

	// stage 2: 48 rows to 16 sums
	reg     [55:0] s2_sum[0:15];
	integer        g;
	always @(posedge clk)
		for (g = 0; g < 16; g = g + 1)
			s2_sum[g] <= row(
				a_wide, b_coef, 3 * g + 1
			) + row(
				a_wide, b_coef, 3 * g + 2
			) + row(
				a_wide, b_coef, 3 * g + 3
			);

	// stage 3: 16 sums to 6
	reg [55:0] s3_sum[0:5];
	always @(posedge clk) begin
		for (g = 0; g < 5; g = g + 1) s3_sum[g] <= s2_sum[3*g] + s2_sum[3*g+1] + s2_sum[3*g+2];
		s3_sum[5] <= s2_sum[15];
	end

	// stage 4: 6 sums to 2
	reg [55:0] s4_sum[0:1];
	always @(posedge clk) begin
		s4_sum[0] <= s3_sum[0] + s3_sum[1] + s3_sum[2];
		s4_sum[1] <= s3_sum[3] + s3_sum[4] + s3_sum[5];
	end

	// ---- sign, kind and exponent beside the pyramid ----
	// The exponent of the product and its tests are formed here, in the clocks the
	// pyramid takes, so that nothing is left for the end but to choose.  The tests
	// are made on the exponent before the normalising shift.
	wire        [14:0] ea = a[62:48];
	wire        [14:0] eb = b[62:48];
	wire signed [16:0] e = $signed({2'b00, ea}) + $signed({2'b00, eb}) - $signed({2'b00, EXP_BIAS});

	reg c2_neg, c3_neg, c4_neg, c5_neg, c6_neg;
	reg [1:0] c2_kind, c3_kind, c4_kind, c5_kind;
	reg c2_integer, c3_integer, c4_integer, c5_integer, c6_integer;
	reg c2_one_zero, c3_one_zero, c4_one_zero, c5_one_zero, c6_one_zero;
	reg        c2_operand_ovf;
	reg [16:0] c2_e;
	reg c3_ovf, c4_ovf, c5_ovf, c6_ovf;
	reg c3_unf, c4_unf, c5_unf, c6_unf;
	reg [14:0] c3_exp, c4_exp, c5_exp;  // the exponent if the product needs no shift
	reg [14:0] c3_exp_m1, c4_exp_m1, c5_exp_m1;  // and if it is shifted one place
	always @(posedge clk) begin
		c2_neg         <= a[63] ^ b[63];
		c2_kind        <= kind;
		c2_integer     <= (ea == 15'd0) && (eb == 15'd0);
		c2_one_zero    <= (ea == 15'd0) != (eb == 15'd0);
		c2_operand_ovf <= (ea >= EXP_OVERFLOW) || (eb >= EXP_OVERFLOW);
		c2_e           <= e;

		c3_neg      <= c2_neg;
		c3_kind     <= c2_kind;
		c3_integer  <= c2_integer;
		c3_one_zero <= c2_one_zero;
		c3_ovf      <= c2_operand_ovf || ($signed(c2_e) >= $signed({2'b00, EXP_OVERFLOW}));
		c3_unf      <= ($signed(c2_e) < $signed({2'b00, EXP_MIN}));
		c3_exp      <= c2_e[14:0];
		c3_exp_m1   <= c2_e[14:0] - 15'd1;

		c4_neg      <= c3_neg;
		c4_kind     <= c3_kind;
		c4_integer  <= c3_integer;
		c4_one_zero <= c3_one_zero;
		c4_ovf      <= c3_ovf;
		c4_unf      <= c3_unf;
		c4_exp      <= c3_exp;
		c4_exp_m1   <= c3_exp_m1;

		c5_neg      <= c4_neg;
		c5_kind     <= c4_kind;
		c5_integer  <= c4_integer;
		c5_one_zero <= c4_one_zero;
		c5_ovf      <= c4_ovf;
		c5_unf      <= c4_unf;
		c5_exp      <= c4_exp;
		c5_exp_m1   <= c4_exp_m1;

		c6_neg      <= c5_neg;
		c6_integer  <= c5_integer;
		c6_one_zero <= c5_one_zero;
		c6_ovf      <= c5_ovf;
		c6_unf      <= c5_unf;
	end

	// stage 5: the last add, with the constants.  The compensation is 9; the round
	// bits are 2^-50 and 2^-51 (066) or 2^-31 and 2^-32 (065).  For 067 the unit
	// forms 198 minus the pyramid sum, which is ~x + ~y + 200.  That is what
	// Cray's own simulation of the unit comes to (see TwoMinus::Cray in mul.rs):
	// it adds one bits at 2^-2 to 2^-48 and 2^-51 + 2^-52 to the sum and its
	// constant, then complements everything below 2^-1.
	reg [55:0] s5_out;
	reg [55:0] konst;
	always @(*) begin
		case (c4_kind)
			K_HALF:  konst = 56'd9 + (56'd1 << 25) + (56'd1 << 24);
			K_ROUND: konst = 56'd9 + (56'd1 << 6) + (56'd1 << 5);
			K_2M:    konst = 56'd200;
			K_FULL:  konst = 56'd9;
		endcase
	end
	always @(posedge clk)
		s5_out <= (c4_kind == K_2M) ? (~s4_sum[0] + ~s4_sum[1] + konst) : (s4_sum[0] + s4_sum[1] + konst);

	// stage 6: normalise by at most one place, cut the half-precision result to 29
	// bits, and take the exponent that goes with the shift
	wire        no_shift = c5_integer || s5_out[55];
	wire [47:0] coef_full = no_shift ? s5_out[55:8] : s5_out[54:7];
	reg  [47:0] s6_coef;
	reg  [14:0] s6_exp;
	always @(posedge clk) begin
		s6_coef <= (c5_kind == K_HALF) ? {coef_full[47:19], 19'd0} : coef_full;
		s6_exp  <= no_shift ? c5_exp : c5_exp_m1;
	end

	// stage 7: pick the result
	reg [63:0] pick;
	reg        pick_err;
	always @* begin
		pick     = {c6_neg, s6_exp, s6_coef};
		pick_err = 1'b0;
		if (c6_one_zero) pick = 64'd0;
		else if (c6_integer) pick = {c6_neg, 15'd0, s6_coef};
		else if (c6_ovf) begin
			pick     = {c6_neg, EXP_OVERFLOW, s6_coef};
			pick_err = 1'b1;
		end else if (c6_unf) pick = 64'd0;
	end
	always @(posedge clk) begin
		o_result    <= pick;
		o_range_err <= pick_err;
	end

	assign o_early = pick;

endmodule

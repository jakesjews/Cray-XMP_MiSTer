//******************************************
//       Floating Point Add Unit
//******************************************
//
// Floating sum or difference of two Cray floating point words (instructions
// 062, 063 and 170 to 173).  A pure pipeline: operands in during one clock, the
// result out six clocks later, a new operation every clock.  The first of the
// six clocks only takes the operands into registers of the unit's own.
//
// Format: bit 63 sign, bits 62:48 exponent biased by 040000 octal, bits 47:0
// coefficient magnitude with the binary point to the left of bit 47.
//
// Algorithm (manual 2240004 pages 3-22 and 3-23; bit-exact with the reference
// model in tools/crates/fp/src/add.rs):
//   1. the operand with the smaller exponent is shifted right by the exponent
//      difference; bits shifted out are lost, there is no rounding
//   2. the magnitudes are added or subtracted in a 49-bit register
//   3. a carry out shifts the sum right one place and adds one to the
//      exponent; otherwise the sum is normalised to the left
//   4. a zero coefficient or an exponent below 020000 gives an all-zero word;
//      an operand exponent of 060000 or more, or a result exponent that reaches
//      060000, is a range error and forces the exponent to 060000

module fp_add (
	input  wire        clk,
	input  wire [63:0] i_a,
	input  wire [63:0] i_b,
	input  wire        i_sub,       // form a - b
	output reg  [63:0] o_result,
	output reg         o_range_err
);

	localparam [14:0] EXP_MIN      = 15'o20000;
	localparam [14:0] EXP_OVERFLOW = 15'o60000;

	// ---- stage 1: the operands ----
	reg [63:0] a, b;
	reg sub;
	always @(posedge clk) begin
		a   <= i_a;
		b   <= i_b;
		sub <= i_sub;
	end

	// ---- stage 2: order the operands by exponent ----
	wire        a_sign = a[63];
	wire [14:0] a_exp = a[62:48];
	wire [47:0] a_coef = a[47:0];
	wire        b_sign = b[63] ^ sub;
	wire [14:0] b_exp = b[62:48];
	wire [47:0] b_coef = b[47:0];

	wire        a_big = (a_exp >= b_exp);
	wire [14:0] diff_ab = a_exp - b_exp;
	wire [14:0] diff_ba = b_exp - a_exp;
	wire [14:0] diff = a_big ? diff_ab : diff_ba;

	reg s2_big_sign, s2_small_sign;
	reg [14:0] s2_exp;
	reg [47:0] s2_big, s2_small;
	reg [5:0] s2_shift;
	reg       s2_gone;  // the smaller operand shifts out completely
	always @(posedge clk) begin
		s2_big_sign   <= a_big ? a_sign : b_sign;
		s2_small_sign <= a_big ? b_sign : a_sign;
		s2_exp        <= a_big ? a_exp : b_exp;
		s2_big        <= a_big ? a_coef : b_coef;
		s2_small      <= a_big ? b_coef : a_coef;
		s2_shift      <= diff[5:0];
		s2_gone       <= (diff >= 15'd48);
	end

	// ---- stage 3: align ----
	reg s3_big_sign, s3_small_sign;
	reg [14:0] s3_exp;
	reg [47:0] s3_big, s3_aligned;
	always @(posedge clk) begin
		s3_big_sign   <= s2_big_sign;
		s3_small_sign <= s2_small_sign;
		s3_exp        <= s2_exp;
		s3_big        <= s2_big;
		s3_aligned    <= s2_gone ? 48'd0 : (s2_small >> s2_shift);
	end

	// ---- stage 4: add or subtract the magnitudes ----
	reg         s4_neg;
	reg  [14:0] s4_exp;
	reg  [48:0] s4_mag;
	wire        same = (s3_big_sign == s3_small_sign);
	wire [48:0] big_less = {1'b0, s3_big} - {1'b0, s3_aligned};  // bit 48 set: the aligned one is the greater
	wire [47:0] aligned_less = s3_aligned - s3_big;
	always @(posedge clk) begin
		s4_exp <= s3_exp;
		if (same) begin
			s4_neg <= s3_big_sign;
			s4_mag <= {1'b0, s3_big} + {1'b0, s3_aligned};
		end else if (!big_less[48]) begin
			s4_neg <= s3_big_sign;
			s4_mag <= {1'b0, big_less[47:0]};
		end else begin
			s4_neg <= s3_small_sign;
			s4_mag <= {1'b0, aligned_less};
		end
	end

	// ---- stage 5: count leading zeros ----
	// The exponent the result gets is the one in hand less the count, or plus one
	// after a carry.  What the range tests of the last stage need is made ready
	// here: how far the exponent in hand is above the two limits.
	function [5:0] lzc48;
		input [47:0] v;
		integer n;
		begin
			lzc48 = 6'd48;
			for (n = 0; n < 48; n = n + 1) if (v[n]) lzc48 = 6'd47 - n[5:0];
		end
	endfunction

	reg               s5_neg;
	reg        [14:0] s5_exp;
	reg        [48:0] s5_mag;
	reg        [ 5:0] s5_lz;
	reg signed [16:0] s5_over_min;  // exponent in hand - 020000
	reg signed [16:0] s5_over_ovf;  // exponent in hand - 060000
	always @(posedge clk) begin
		s5_neg      <= s4_neg;
		s5_exp      <= s4_exp;
		s5_mag      <= s4_mag;
		s5_lz       <= lzc48(s4_mag[47:0]);
		s5_over_min <= $signed({2'b00, s4_exp}) - $signed({2'b00, EXP_MIN});
		s5_over_ovf <= $signed({2'b00, s4_exp}) - $signed({2'b00, EXP_OVERFLOW});
	end

	// ---- stage 6: normalise, range checks and packing ----
	wire               carry = s5_mag[48];
	wire               zero = (s5_mag == 49'd0);
	wire               operand_ovf = (s5_exp >= EXP_OVERFLOW);
	// the result exponent is the one in hand + 1 after a carry, less the count otherwise
	wire signed [16:0] step = carry ? -17'sd1 : $signed({11'd0, s5_lz});
	wire               result_ovf = operand_ovf || (s5_over_ovf >= step);
	wire               result_unf = (s5_over_min < step);
	wire        [14:0] exp_out = carry ? (s5_exp + 15'd1) : (s5_exp - {9'd0, s5_lz});
	wire        [47:0] coef_out = carry ? s5_mag[48:1] : (s5_mag[47:0] << s5_lz);
	always @(posedge clk) begin
		if (zero) begin
			// no unit produces a negative zero
			o_result    <= operand_ovf ? {1'b0, EXP_OVERFLOW, 48'd0} : 64'd0;
			o_range_err <= operand_ovf;
		end else if (result_ovf) begin
			// The error is the incoming exponent's (manual 3-21).  A carry that takes
			// in-range operands to 060000 is delivered without it.
			o_result    <= {s5_neg, EXP_OVERFLOW, coef_out};
			o_range_err <= operand_ovf;
		end else if (result_unf) begin
			o_result    <= 64'd0;
			o_range_err <= 1'b0;
		end else begin
			o_result    <= {s5_neg, exp_out, coef_out};
			o_range_err <= 1'b0;
		end
	end

endmodule

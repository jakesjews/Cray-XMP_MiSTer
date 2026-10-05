//******************************************
//       Floating Point Add Unit
//******************************************
//
// Floating sum or difference of two Cray floating point words (instructions
// 062, 063 and 170 to 173).  A pure pipeline: operands in during one clock, the
// result out six clocks later, a new operation every clock.
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

	// ---- stage 1: order the operands by exponent ----
	wire        a_sign = i_a[63];
	wire [14:0] a_exp = i_a[62:48];
	wire [47:0] a_coef = i_a[47:0];
	wire        b_sign = i_b[63] ^ i_sub;
	wire [14:0] b_exp = i_b[62:48];
	wire [47:0] b_coef = i_b[47:0];

	wire        a_big = (a_exp >= b_exp);
	wire [14:0] diff = a_big ? (a_exp - b_exp) : (b_exp - a_exp);

	reg s1_big_sign, s1_small_sign;
	reg [14:0] s1_exp;
	reg [47:0] s1_big, s1_small;
	reg [5:0] s1_shift;
	reg       s1_gone;  // the smaller operand shifts out completely

	always @(posedge clk) begin
		s1_big_sign   <= a_big ? a_sign : b_sign;
		s1_small_sign <= a_big ? b_sign : a_sign;
		s1_exp        <= a_big ? a_exp : b_exp;
		s1_big        <= a_big ? a_coef : b_coef;
		s1_small      <= a_big ? b_coef : a_coef;
		s1_shift      <= diff[5:0];
		s1_gone       <= (diff >= 15'd48);
	end

	// ---- stage 2: align ----
	reg s2_big_sign, s2_small_sign;
	reg [14:0] s2_exp;
	reg [47:0] s2_big, s2_aligned;

	always @(posedge clk) begin
		s2_big_sign   <= s1_big_sign;
		s2_small_sign <= s1_small_sign;
		s2_exp        <= s1_exp;
		s2_big        <= s1_big;
		s2_aligned    <= s1_gone ? 48'd0 : (s1_small >> s1_shift);
	end

	// ---- stage 3: add or subtract the magnitudes ----
	reg        s3_neg;
	reg [14:0] s3_exp;
	reg [48:0] s3_mag;

	wire same = (s2_big_sign == s2_small_sign);
	wire big_ge = (s2_big >= s2_aligned);

	always @(posedge clk) begin
		s3_exp <= s2_exp;
		if (same) begin
			s3_neg <= s2_big_sign;
			s3_mag <= {1'b0, s2_big} + {1'b0, s2_aligned};
		end else if (big_ge) begin
			s3_neg <= s2_big_sign;
			s3_mag <= {1'b0, s2_big - s2_aligned};
		end else begin
			s3_neg <= s2_small_sign;
			s3_mag <= {1'b0, s2_aligned - s2_big};
		end
	end

	// ---- stage 4: count leading zeros ----
	function [5:0] lzc48;
		input [47:0] v;
		integer n;
		begin
			lzc48 = 6'd48;
			for (n = 0; n < 48; n = n + 1) if (v[n]) lzc48 = 6'd47 - n[5:0];
		end
	endfunction

	reg        s4_neg;
	reg [14:0] s4_exp;
	reg [48:0] s4_mag;
	reg [ 5:0] s4_lz;

	always @(posedge clk) begin
		s4_neg <= s3_neg;
		s4_exp <= s3_exp;
		s4_mag <= s3_mag;
		s4_lz  <= lzc48(s3_mag[47:0]);
	end

	// ---- stage 5: normalise ----
	reg s5_neg, s5_zero, s5_operand_ovf;
	reg signed [16:0] s5_exp;
	reg        [47:0] s5_coef;

	always @(posedge clk) begin
		s5_neg         <= s4_neg;
		s5_zero        <= (s4_mag == 49'd0);
		s5_operand_ovf <= (s4_exp >= EXP_OVERFLOW);
		if (s4_mag[48]) begin
			s5_coef <= s4_mag[48:1];
			s5_exp  <= $signed({2'b00, s4_exp}) + 17'sd1;
		end else begin
			s5_coef <= s4_mag[47:0] << s4_lz;
			s5_exp  <= $signed({2'b00, s4_exp}) - $signed({11'd0, s4_lz});
		end
	end

	// ---- stage 6: range checks and packing ----
	wire result_ovf = s5_operand_ovf || (s5_exp >= $signed({2'b00, EXP_OVERFLOW}));
	wire result_unf = (s5_exp < $signed({2'b00, EXP_MIN}));

	always @(posedge clk) begin
		if (s5_zero) begin
			// no unit produces a negative zero
			o_result    <= s5_operand_ovf ? {1'b0, EXP_OVERFLOW, 48'd0} : 64'd0;
			o_range_err <= s5_operand_ovf;
		end else if (result_ovf) begin
			// The error is the incoming exponent's (manual 3-21).  A carry that takes
			// in-range operands to 060000 is delivered without it.
			o_result    <= {s5_neg, EXP_OVERFLOW, s5_coef};
			o_range_err <= s5_operand_ovf;
		end else if (result_unf) begin
			o_result    <= 64'd0;
			o_range_err <= 1'b0;
		end else begin
			o_result    <= {s5_neg, s5_exp[14:0], s5_coef};
			o_range_err <= 1'b0;
		end
	end

endmodule

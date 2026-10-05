// Bench wrapper for the floating point units: one set of operands in, each
// unit's result out after that unit's latency.
module fp_tb (
	input  wire        clk,
	input  wire [63:0] i_a,
	input  wire [63:0] i_b,
	input  wire        i_sub,
	input  wire [ 1:0] i_kind,
	output wire [63:0] o_add,
	output wire        o_add_err,
	output wire [63:0] o_mul,
	output wire        o_mul_err,
	output wire [63:0] o_recip,
	output wire        o_recip_err
);

	fp_add add (
		.clk        (clk),
		.i_a        (i_a),
		.i_b        (i_b),
		.i_sub      (i_sub),
		.o_result   (o_add),
		.o_range_err(o_add_err)
	);
	fp_mul mul (
		.clk        (clk),
		.i_a        (i_a),
		.i_b        (i_b),
		.i_kind     (i_kind),
		.o_result   (o_mul),
		.o_range_err(o_mul_err)
	);
	fp_recip rcp (
		.clk        (clk),
		.i_a        (i_a),
		.o_result   (o_recip),
		.o_range_err(o_recip_err)
	);

endmodule

//******************************************
//       Vector Shift Unit
//******************************************
//
// Shifts of vector elements by (Ak) places for instructions 150 to 153.  A pure
// pipeline with the X-MP's unit times: four clocks for 152, three for 150, 151
// and 153 (CSM-0111000 page 4-17).  All shifts are end-off with zero fill
// (manual 4-53 to 4-56).
//
//   i_op 0  150  single left shift of each element; zero if the count is over 63
//        1  151  single right shift
//        2  152  double left shift: element n joined with element n+1 on its
//                right, shifted left, upper 64 bits kept; the last element is
//                joined with zeros
//        3  153  double right shift: element n-1 joined with element n on its
//                right, shifted right, lower 64 bits kept; element 0 is joined
//                with zeros
//
// The double shifts need a neighbouring element.  Elements arrive one per
// clock.  For 153 the previous one is still in hand when an element has been
// taken in.  For 152 the unit holds the element a clock longer, until the next
// one is at the input: that is its fourth clock.

module vector_shift (
	input  wire        clk,
	input  wire [ 1:0] i_op,
	input  wire [23:0] i_cnt,
	input  wire [63:0] i_d,      // the Vj element
	input  wire        i_valid,  // an element is at the input
	input  wire        i_first,  // it is element 0
	input  wire        i_last,   // it is the last element of the operation
	output reg  [63:0] o_result
);

	// stage 1: take the element in
	reg [63:0] cur, prev;
	reg cur_first, cur_last;
	reg [ 1:0] op1;
	reg [23:0] cnt1;
	always @(posedge clk) begin
		cur       <= i_d;
		prev      <= cur;
		cur_first <= i_first;
		cur_last  <= i_last;
		op1       <= i_op;
		cnt1      <= i_cnt;
	end

	// stage 2, for 152 only: join the element with the next one.  While cur holds
	// element n the input carries element n+1, unless cur is the last one.
	// The operation and the count are a clock old here as well; they do not change
	// while an operation is in the unit, so the stage behind uses these for 150,
	// 151 and 153 too, whose element comes to it straight from stage 1.
	reg [127:0] pair;
	reg [  1:0] op2;
	reg [ 23:0] cnt2;
	always @(posedge clk) begin
		op2  <= op1;
		cnt2 <= cnt1;
		pair <= {cur, (cur_last || !i_valid) ? 64'd0 : i_d};
	end

	// the shift: of the pair for 152, of the element in hand for the others, for
	// 153 with the element before it on its left
	reg  [ 63:0] shifted;
	wire [127:0] in = (op2 == 2'd2) ? pair : {((op2 == 2'd3) && !cur_first) ? prev : 64'd0, cur};
	wire [127:0] dbl_l = in << cnt2[6:0];
	wire [127:0] dbl_r = in >> cnt2[6:0];
	always @(posedge clk) begin
		case (op2)
			2'd0: shifted <= (|cnt2[23:6]) ? 64'd0 : (in[63:0] << cnt2[5:0]);
			2'd1: shifted <= (|cnt2[23:6]) ? 64'd0 : (in[63:0] >> cnt2[5:0]);
			2'd2: shifted <= (|cnt2[23:7]) ? 64'd0 : dbl_l[127:64];
			2'd3: shifted <= (|cnt2[23:7]) ? 64'd0 : dbl_r[63:0];
		endcase
	end

	// the last stage
	always @(posedge clk) o_result <= shifted;

endmodule

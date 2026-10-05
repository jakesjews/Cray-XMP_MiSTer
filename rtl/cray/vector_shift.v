//******************************************
//       Vector Shift Unit
//******************************************
//
// Shifts of vector elements by (Ak) places for instructions 150 to 153.  A pure
// pipeline with a unit time of four clocks.  All shifts are end-off with zero
// fill (manual 4-53 to 4-56).
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
// clock, so the unit holds each for a clock: by then the next one is at the
// input (for 152) and the previous one is still in hand (for 153).

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

	// stage 2: join it with its neighbour.  While cur holds element n the input
	// carries element n+1, unless cur is the last one.
	reg [127:0] pair;
	reg [  1:0] op2;
	reg [ 23:0] cnt2;
	always @(posedge clk) begin
		op2  <= op1;
		cnt2 <= cnt1;
		case (op1)
			2'd2:    pair <= {cur, (cur_last || !i_valid) ? 64'd0 : i_d};
			2'd3:    pair <= {cur_first ? 64'd0 : prev, cur};
			default: pair <= {64'd0, cur};
		endcase
	end

	// stage 3: shift
	reg  [ 63:0] shifted;
	wire [127:0] dbl_l = pair << cnt2[6:0];
	wire [127:0] dbl_r = pair >> cnt2[6:0];
	always @(posedge clk) begin
		case (op2)
			2'd0: shifted <= (|cnt2[23:6]) ? 64'd0 : (pair[63:0] << cnt2[5:0]);
			2'd1: shifted <= (|cnt2[23:6]) ? 64'd0 : (pair[63:0] >> cnt2[5:0]);
			2'd2: shifted <= (|cnt2[23:7]) ? 64'd0 : dbl_l[127:64];
			2'd3: shifted <= (|cnt2[23:7]) ? 64'd0 : dbl_r[63:0];
		endcase
	end

	// stage 4
	always @(posedge clk) o_result <= shifted;

endmodule

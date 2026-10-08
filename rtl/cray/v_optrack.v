// One vector operation on its way through a functional unit.
//
// A vector instruction issues in some clock t.  Its operand registers present
// element 0 from t+1, and the unit takes it in during clock t+4; the elements
// behind it follow one a clock, so the operands of element n are at the unit's
// inputs during clock t+4+n, and with a unit time of L clocks the result of
// element n is at the unit's output during clock t+4+n+L.  This block says so:
// it marks which clocks carry operands (o_in_*) and results (o_out_*), numbers
// the elements, and carries the destination register beside the data.
//
// That is when every operand is in its register.  An operand register may
// still be receiving the result of an earlier instruction (chaining,
// CSM-0111000 page 4-12): the operation then takes each element when it is
// there, which i_ok says of the element o_ask, and the clocks without operands
// go through the unit as gaps.  Whether an operand register was receiving a
// result when the instruction issued is kept (o_chain_j, o_chain_k); one that
// was not has all its elements, and may be the operation's own result register.
//
// The unit is busy from the clock after issue until its last operand pair has
// gone in: (VL) + 4 clocks from issue when nothing was waited for, the X-MP's
// "unit ready" (CSM-0111000 section 5).  A new operation can then follow
// straight behind the old one in the pipeline, which is why the output side is
// a shift register and not a counter.
//
// An operation that issues with i_short takes one clock less than L: three of
// the four shifts do.  The first operands of an operation are at the unit at
// least three clocks after the last of the one before, so a short operation
// behind a long one does not catch up with it.
//
// It also keeps what the instruction supplied once at issue and the unit needs
// for every element: the instruction itself and the scalar operands (Sj) and
// (Ak), which the program may change straight after issue (manual 3-5).

module v_optrack #(
	parameter L = 2
) (
	input wire clk,
	input wire rst,

	input wire        i_start,  // a vector operation issues to this unit
	input wire [ 6:0] i_len,    // number of elements, 1 to 64
	input wire [15:0] i_cip,
	input wire [63:0] i_sj,
	input wire [23:0] i_ak,
	input wire        i_wr_v,   // the instruction in CIP sends results to a V register (175 does not)
	input wire        i_short,  // the unit takes L - 1 clocks for it
	input wire [ 7:0] i_vj,     // the V registers the instruction in CIP reads, one bit a register
	input wire [ 7:0] i_vk,
	input wire [ 7:0] i_result, // the V registers reserved as a result now

	output wire o_busy,

	output reg [15:0] o_instr,
	output reg [63:0] o_sj,
	output reg [23:0] o_ak,

	// the element the operand registers are asked for
	output wire [5:0] o_ask,
	output wire       o_ask_last,  // it is the last of the operation
	output reg        o_chain_j,   // Vj was receiving a result when the instruction issued
	output reg        o_chain_k,   // Vk was
	input  wire       i_ok,        // the element is in the operand registers
	output wire [7:0] o_step,      // it is taken: those registers go on to the next

	output reg        o_in_valid,
	output reg  [5:0] o_in_idx,
	output wire       o_in_first,
	output reg        o_in_last,

	output wire       o_out_valid,
	output wire [5:0] o_out_idx,
	output wire       o_out_last,
	output wire [2:0] o_out_dest,
	output wire       o_out_wr_v
);

	// LEAD: the two clocks before element 0 can be taken.  RUN: elements are taken
	// as they are there.  DONE: the last of them is at the unit.
	localparam IDLE = 2'd0, LEAD = 2'd1, RUN = 2'd2, DONE = 2'd3;

	reg [1:0] state;
	reg [5:0] ask;
	reg [6:0] len;
	reg [2:0] dest;
	reg       wr_v;
	reg       short;
	reg       lead;
	reg [7:0] reads;

	wire go = (state == RUN) && i_ok;

	assign o_busy     = (state != IDLE);
	assign o_ask      = ask;
	assign o_ask_last = ({1'b0, ask} == len - 7'd1);
	assign o_step     = {8{go}} & reads;
	assign o_in_first = (o_in_idx == 6'd0);

	always @(posedge clk) begin
		// an element taken from the registers in one clock is at the unit in the next
		o_in_valid <= !rst && go;
		o_in_idx   <= ask;
		o_in_last  <= o_ask_last;

		if (rst) state <= IDLE;
		else
			case (state)
				// While idle the unit takes in what the current instruction supplies,
				// every clock; the instruction that starts it leaves what it supplied.
				// So nothing wide waits for the decision to issue.
				IDLE: begin
					len       <= i_len;
					dest      <= i_cip[8:6];
					wr_v      <= i_wr_v;
					short     <= i_short;
					reads     <= i_vj | i_vk;
					o_chain_j <= |(i_vj & i_result);
					o_chain_k <= |(i_vk & i_result);
					o_instr   <= i_cip;
					o_sj      <= i_sj;
					o_ak      <= i_ak;
					ask       <= 6'd0;
					lead      <= 1'b1;
					if (i_start) state <= LEAD;
				end
				LEAD: begin
					lead <= 1'b0;
					if (!lead) state <= RUN;
				end
				RUN:
				if (go) begin
					ask <= ask + 6'd1;
					if (o_ask_last) state <= DONE;
				end
				DONE:    state <= IDLE;
				default: state <= IDLE;
			endcase
	end

	// the same marks, L clocks later, beside the results; those of a short operation
	// leave a stage early
	localparam W = 1 + 1 + 6 + 3 + 1;
	reg     [W:0] pipe[0:L-1];
	integer       n;
	always @(posedge clk) begin
		pipe[0] <= rst ? {(W + 1) {1'b0}} : {short, o_in_valid, o_in_last, o_in_idx, dest, wr_v};
		for (n = 1; n < L; n = n + 1) pipe[n] <= rst ? {(W + 1) {1'b0}} : pipe[n-1];
	end

	wire [W:0] early = pipe[L-2];
	wire [W:0] late = pipe[L-1];
	assign {o_out_valid, o_out_last, o_out_idx, o_out_dest, o_out_wr_v} =
		(early[W] && early[W-1]) ? early[W-1:0] : late[W] ? {W{1'b0}} : late[W-1:0];

endmodule

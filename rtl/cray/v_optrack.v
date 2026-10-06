// One vector operation on its way through a functional unit.
//
// A vector instruction issues in some clock t.  Its operand registers present
// element 0 during t+1, so the operands of element n are at the unit's inputs
// during clock t+2+n, and with a unit time of L clocks the result of element n
// is at the unit's output during clock t+2+n+L.  This block says so: it marks
// which clocks carry operands (o_in_*) and results (o_out_*), numbers the
// elements, and carries the destination register beside the data.
//
// The unit is busy from the clock after issue until its last operand pair has
// gone in.  A new operation can then follow straight behind the old one in the
// pipeline, which is why the output side is a shift register and not a counter.
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

	output wire o_busy,

	output reg [15:0] o_instr,
	output reg [63:0] o_sj,
	output reg [23:0] o_ak,

	output wire       o_in_valid,
	output wire [5:0] o_in_idx,
	output wire       o_in_first,
	output wire       o_in_last,

	output wire       o_out_valid,
	output wire [5:0] o_out_idx,
	output wire       o_out_last,
	output wire [2:0] o_out_dest,
	output wire       o_out_wr_v
);

	localparam IDLE = 2'd0, LEAD = 2'd1, RUN = 2'd2;

	reg [1:0] state;
	reg [5:0] idx;
	reg [6:0] len;
	reg [2:0] dest;
	reg       wr_v;

	assign o_busy     = (state != IDLE);
	assign o_in_valid = (state == RUN);
	assign o_in_idx   = idx;
	assign o_in_first = (idx == 6'd0);
	assign o_in_last  = ({1'b0, idx} == len - 7'd1);

	always @(posedge clk) begin
		if (rst) state <= IDLE;
		else
			case (state)
				// While idle the unit takes in what the current instruction supplies,
				// every clock; the instruction that starts it leaves what it supplied.
				// So nothing wide waits for the decision to issue.
				IDLE: begin
					len     <= i_len;
					dest    <= i_cip[8:6];
					wr_v    <= i_wr_v;
					o_instr <= i_cip;
					o_sj    <= i_sj;
					o_ak    <= i_ak;
					if (i_start) state <= LEAD;
				end
				LEAD: begin
					idx   <= 6'd0;
					state <= RUN;
				end
				RUN: begin
					idx <= idx + 6'd1;
					if (o_in_last) state <= IDLE;
				end
				default: state <= IDLE;
			endcase
	end

	// the same marks, L clocks later, beside the results
	localparam W = 1 + 1 + 6 + 3 + 1;
	reg     [W-1:0] pipe[0:L-1];
	integer         n;
	always @(posedge clk) begin
		pipe[0] <= rst ? {W{1'b0}} : {o_in_valid, o_in_last, idx, dest, wr_v};
		for (n = 1; n < L; n = n + 1) pipe[n] <= rst ? {W{1'b0}} : pipe[n-1];
	end

	assign {o_out_valid, o_out_last, o_out_idx, o_out_dest, o_out_wr_v} = pipe[L-1];

endmodule

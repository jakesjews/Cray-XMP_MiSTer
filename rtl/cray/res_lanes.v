// The results on their way to the A registers, or to the S registers.
//
// Every unit that delivers to the registers has a lane here: a delay line as
// long as the unit takes.  The register a result is for enters its lane when
// the instruction issues and is at the head, stage 0, in the clock the result
// is at the registers.  Two instructions that use the same unit issue in
// different clocks, so a lane never holds two results for one clock; results
// of different units can come in the same clock, each by its own lane, and
// none waits for another.  (The CRAY-1 has one way into each group of
// registers and holds an instruction whose result would come together with
// another's; the X-MP's manual knows no such hold.)
//
// A lane of delay 0 has no delay line: its results come when they come (a
// word from memory).  The register is reserved from the instruction's issue,
// and the unit says through i_ext_v and i_ext_d, a clock ahead, when its
// result is at the head.
//
// The two masks are what the issue logic holds an instruction against.  A
// register is in o_res_mask while a result is on its way to it or at the head,
// and in o_wait_mask while one is on its way: a register whose result is at
// the head is free for the instruction that issues in that clock, as an
// operand and as the place of its own result.  Both are registers, loaded with
// what holds after this clock.

module res_lanes #(
	parameter            NL    = 4,  // lanes
	parameter [4*NL-1:0] DELAY = 0   // clocks from issue to the result, four bits a lane, lane 0 lowest
) (
	input wire clk,
	input wire rst,

	input wire          i_issue,  // the pipeline moves on: the current instruction, if there is one, issues
	input wire [NL-1:0] i_lane,   // the lane its result takes; none if it has no result or there is no instruction
	input wire [   7:0] i_dest,   // the register the result is for, one bit a register
	input wire [   2:0] i_dnum,   // and as a number

	input wire [  NL-1:0] i_ext_v,  // lanes of delay 0: a result is at the head in the next clock
	input wire [3*NL-1:0] i_ext_d,

	output wire [NL-1:0] o_head_v,  // a result is at the head
	output wire [8*NL-1:0] o_head_hot,  // and the register it is for, one bit a register; none if there is no result
	output wire [NL-1:0] o_next_v,  // a result is at stage 1, at the head in the next clock (not for lanes of delay 1)
	output wire [3*NL-1:0] o_next_d,
	output wire [8*NL-1:0] o_next_hot,

	output wire [7:0] o_res_mask,
	output wire [7:0] o_wait_mask
);

	// lanes whose results are not at the head in the clock after issue
	function [NL-1:0] slow_lanes;
		input [4*NL-1:0] delays;
		integer n;
		begin
			for (n = 0; n < NL; n = n + 1) slow_lanes[n] = (delays[4*n+:4] != 4'd1);
		end
	endfunction
	localparam [NL-1:0] SLOW = slow_lanes(DELAY);

	genvar l;
	generate
		for (l = 0; l < NL; l = l + 1) begin : g_lane
			localparam integer D = {28'd0, DELAY[4*l+:4]};
			if (D == 0) begin : g_ext
				reg       hv;
				reg [7:0] hh;
				always @(posedge clk)
					if (rst) begin
						hv <= 1'b0;
						hh <= 8'd0;
					end else begin
						hv <= i_ext_v[l];
						hh <= i_ext_v[l] ? (8'd1 << i_ext_d[3*l+:3]) : 8'd0;
					end
				assign o_head_v[l]        = hv;
				assign o_head_hot[8*l+:8] = hh;
				assign o_next_v[l]        = i_ext_v[l];
				assign o_next_d[3*l+:3]   = i_ext_d[3*l+:3];
				assign o_next_hot[8*l+:8] = i_ext_v[l] ? (8'd1 << i_ext_d[3*l+:3]) : 8'd0;
			end else begin : g_line
				reg     [D-1:0] v;
				reg     [  2:0] d                           [0:D-1];
				reg     [  7:0] h                           [0:D-1];
				wire            load = i_issue && i_lane[l];
				integer         k;
				always @(posedge clk)
					for (k = 0; k < D; k = k + 1)
						if (rst) begin
							v[k] <= 1'b0;
							d[k] <= 3'd0;
							h[k] <= 8'd0;
						end else if (k == D - 1) begin
							v[k] <= load;
							d[k] <= i_dnum;
							h[k] <= load ? i_dest : 8'd0;
						end else begin
							v[k] <= v[k+1];
							d[k] <= d[k+1];
							h[k] <= h[k+1];
						end
				assign o_head_v[l]        = v[0];
				assign o_head_hot[8*l+:8] = h[0];
				if (D >= 2) begin : g_next
					assign o_next_v[l]        = v[1];
					assign o_next_d[3*l+:3]   = d[1];
					assign o_next_hot[8*l+:8] = h[1];
				end else begin : g_none
					assign o_next_v[l]        = 1'b0;
					assign o_next_d[3*l+:3]   = 3'd0;
					assign o_next_hot[8*l+:8] = 8'd0;
				end
			end
		end
	endgenerate

	// A register has at most one result on its way: an instruction that would
	// send it a second holds issue.  What is on its way stays so or comes to the
	// head; what issues now comes on top.
	reg     [7:0] to_head;
	integer       m;
	always @* begin
		to_head = 8'd0;
		for (m = 0; m < NL; m = m + 1) to_head = to_head | o_next_hot[8*m+:8];
	end

	wire load_any = i_issue && (|i_lane);
	wire load_slow = i_issue && (|(i_lane & SLOW));
	reg [7:0] res_mask, wait_mask;
	always @(posedge clk)
		if (rst) begin
			res_mask  <= 8'd0;
			wait_mask <= 8'd0;
		end else begin
			res_mask  <= wait_mask | ({8{load_any}} & i_dest);
			wait_mask <= (wait_mask & ~to_head) | ({8{load_slow}} & i_dest);
		end

	assign o_res_mask  = res_mask;
	assign o_wait_mask = wait_mask;

endmodule

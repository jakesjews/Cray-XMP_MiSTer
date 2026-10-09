// The eight A registers, or the eight S registers, and the ways into and out
// of them.
//
// In.  Every lane of results (res_lanes.v) has its own way into the registers:
// i_data is what each lane delivers, and i_wr, one bit a register, where it
// goes at the end of this clock.  Results for different registers can come in
// the same clock.  No two are ever for the same register.
//
// The first NB lanes deliver in the clock their result is due.  An instruction
// that issues in that clock and reads the register takes the result off the
// lane.  The other lanes have their result a clock before it is due and write
// it then: the register is reserved until the result is due, nobody reads it
// before, and in the clock it is due it is read from the register like any
// other.
//
// Out.  A read port gives the register i_addr names, or the result of a lane
// that is on its way into that register in this clock.  Which of them it is
// is not found out on the way of the data, which is the longest way in the
// machine: it is kept in a register for every port, formed in the clock before
// from what will then be at the heads of the lanes and from the register the
// port will then read.  That is the same register as now if the current
// instruction does not issue, and i_addr_nxt if it does.  A lane of one clock
// (those in ONE) has its result at the head in the next clock only if the
// current instruction is of that lane and issues (i_now_lane, i_now_d); the
// others say what is at stage 1 (i_next_v, i_next_d).  Whether the instruction
// issues is known last of all, so everything else is formed beside it and it
// only picks.
//
// A port in ZERO reads register 0 as nothing: all bits zero.

module res_regfile #(
	parameter          W    = 64,
	parameter          NL   = 4,   // lanes
	parameter          NB   = 2,   // the first NB of them are read off the lane
	parameter [NB-1:0] ONE  = 0,   // those of them that take one clock
	parameter          NP   = 3,   // read ports
	parameter [NP-1:0] ZERO = 0
) (
	input wire clk,
	input wire rst,

	input wire [W*NL-1:0] i_data,
	input wire [8*NL-1:0] i_wr,

	// the exchange sequence writes as well
	input wire         i_x_en,
	input wire [  2:0] i_x_addr,
	input wire [W-1:0] i_x_data,

	input  wire [3*NP-1:0] i_addr,
	input  wire [3*NP-1:0] i_addr_nxt,
	input  wire            i_issue,
	input  wire [  NB-1:0] i_next_v,
	input  wire [3*NB-1:0] i_next_d,
	input  wire [  NB-1:0] i_now_lane,  // the current instruction is one, and its result takes this lane (of ONE)
	input  wire [     2:0] i_now_d,
	output wire [W*NP-1:0] o_data,

	input  wire [  2:0] i_ex_addr,
	output wire [W-1:0] o_ex_data,
	output wire [W-1:0] o_r0        // register 0, for the branch tests
);

	reg [W-1:0] data[0:7];

	assign o_ex_data = data[i_ex_addr];
	assign o_r0      = data[0];

	// ---- in
	genvar r, p, b;
	generate
		for (r = 0; r < 8; r = r + 1) begin : g_reg
			reg     [W-1:0] din;
			reg             we;
			integer         n;
			always @* begin
				din = i_x_data & {W{i_x_en && (i_x_addr == r)}};
				we  = i_x_en && (i_x_addr == r);
				for (n = 0; n < NL; n = n + 1) begin
					din = din | (i_data[W*n+:W] & {W{i_wr[8*n+r]}});
					we  = we | i_wr[8*n+r];
				end
			end
			always @(posedge clk)
				if (rst) data[r] <= {W{1'b0}};
				else if (we) data[r] <= din;
		end
	endgenerate

`ifdef VERILATOR
	// two results for one register in one clock would mean the issue logic let
	// a second instruction at a register that was reserved
	function [7:0] writers;
		input [2:0] number;
		integer n;
		begin
			writers = 8'd0;
			for (n = 0; n < NL; n = n + 1) writers = writers + {7'd0, i_wr[8*n+{29'd0, number}]};
		end
	endfunction
	integer cr;
	always @(posedge clk)
		if (!rst)
			for (cr = 0; cr < 8; cr = cr + 1)
				if (writers(cr[2:0]) > 8'd1) begin
					$display("res_regfile: %0d results for register %0d in one clock", writers(cr[2:0]), cr);
					$finish;
				end
`endif

	// ---- out
	generate
		for (p = 0; p < NP; p = p + 1) begin : g_port
			wire [   2:0] cur = i_addr[3*p+:3];
			wire [   2:0] nxt = i_addr_nxt[3*p+:3];
			wire          cur_reg = !(ZERO[p] && (cur == 3'd0));  // the port reads a register, not nothing
			wire          nxt_reg = !(ZERO[p] && (nxt == 3'd0));
			reg  [NB-1:0] hit;  // the port reads the result of this lane
			reg           own;  // it reads the register itself
			wire [NB-1:0] if_issue, if_held;
			for (b = 0; b < NB; b = b + 1) begin : g_hit
				if (ONE[b]) begin : g_one
					assign if_issue[b] = nxt_reg && i_now_lane[b] && (i_now_d == nxt);
					assign if_held[b]  = 1'b0;
				end else begin : g_more
					assign if_issue[b] = nxt_reg && i_next_v[b] && (i_next_d[3*b+:3] == nxt);
					assign if_held[b]  = cur_reg && i_next_v[b] && (i_next_d[3*b+:3] == cur);
				end
			end
			wire own_issue = nxt_reg && !(|if_issue);
			wire own_held = cur_reg && !(|if_held);
			always @(posedge clk)
				if (rst) begin
					hit <= {NB{1'b0}};
					own <= 1'b1;
				end else begin
					hit <= i_issue ? if_issue : if_held;
					own <= i_issue ? own_issue : own_held;
				end

			reg     [W-1:0] q;
			integer         n;
			always @* begin
				q = data[cur] & {W{own}};
				for (n = 0; n < NB; n = n + 1) q = q | (i_data[W*n+:W] & {W{hit[n]}});
			end
			assign o_data[W*p+:W] = q;
		end
	endgenerate

endmodule

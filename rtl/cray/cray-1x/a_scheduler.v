//////////////////////////////////////////////////////////////////
//        Cray A-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling
//for instructions that utilize the address "A" Register file.
//
//What it has to know about the current instruction comes decoded, from the
//register func_top keeps beside CIP (cray_predecode, the fields PD_A_*).
//The results on their way are kept in res_lanes.v, a lane for every unit.

module a_scheduler (
	clk,
	rst,
	i_cip_vld,
	i_issue_vld,
	i_type,
	i_lane,
	i_dest,
	i_dnum,
	i_cmask,
	i_025,
	i_sconf,
	i_s_wait_mask,
	i_mem_v,
	i_mem_d,
	o_a_issue,
	o_a_type,
	o_head_v,
	o_head_hot,
	o_next_v,
	o_next_d,
	o_next_hot,
	o_a0_busy,
	o_a_res_mask,
	o_a_wait_mask
);
	`include "cray_types.vh"

	input wire clk;
	input wire rst;
	input wire i_cip_vld;
	input wire i_issue_vld;
	//the instruction in CIP
	input wire i_type;  // an A-type instruction
	input wire [AL_N-1:0] i_lane;  // the lane its result takes
	input wire [7:0] i_dest;  // the register the result goes to, one bit a register
	input wire [2:0] i_dnum;  // and as a number
	input wire [7:0] i_cmask;  // registers that must have no result on its way
	input wire i_025;  // Bjk Ai
	input wire [7:0] i_sconf;  // 023: the S register it reads
	input wire [7:0] i_s_wait_mask;
	//a word from memory is at the registers in the next clock, and the register it is for
	input wire i_mem_v;
	input wire [2:0] i_mem_d;

	output wire o_a_issue;
	output wire o_a_type;
	//the lanes, for the A registers (res_regfile.v)
	output wire [AL_N-1:0] o_head_v;
	output wire [8*AL_N-1:0] o_head_hot;
	output wire [AL_N-1:0] o_next_v;
	output wire [3*AL_N-1:0] o_next_d;
	output wire [8*AL_N-1:0] o_next_hot;
	output wire o_a0_busy;
	output wire [7:0] o_a_res_mask;  // registers with a result on its way or at the head of a lane
	output wire [7:0] o_a_wait_mask;  // registers with a result that is not at the head yet

	wire a_to_b_vld;  //for executing 7'o025
	wire s_conflict;

	//Let's figure out if it's okay to issue the special case of the 7'o025 instruction (Bjk <= Ai)
	assign a_to_b_vld = i_025 && !(|(i_dest & o_a_wait_mask));

	//o_a_type get asserted for 7'b0_01?_??? and 7'b1_000_??? instructions, except for 7'b0_011_1??
	// which translates to: 020-037, 100-107, except for 034-037; and not for 027??7
	assign o_a_type = i_cip_vld && i_type;

	res_lanes #(
		.NL   (AL_N),
		.DELAY(AL_DELAY)
	) lanes (
		.clk        (clk),
		.rst        (rst),
		.i_issue    (i_issue_vld),
		.i_lane     ({AL_N{i_cip_vld}} & i_lane),
		.i_dest     (i_dest),
		.i_dnum     (i_dnum),
		.i_ext_v    ({{(AL_N - AL_MEM - 1) {1'b0}}, i_mem_v, {AL_MEM{1'b0}}}),
		.i_ext_d    ({{(3 * (AL_N - AL_MEM - 1)) {1'b0}}, i_mem_d, {(3 * AL_MEM) {1'b0}}}),
		.o_head_v   (o_head_v),
		.o_head_hot (o_head_hot),
		.o_next_v   (o_next_v),
		.o_next_d   (o_next_d),
		.o_next_hot (o_next_hot),
		.o_res_mask (o_a_res_mask),
		.o_wait_mask(o_a_wait_mask)
	);

	assign o_a0_busy = o_a_res_mask[0];

	//check if it's free to issue
	assign s_conflict = |(i_sconf & i_s_wait_mask);
	assign o_a_issue  = o_a_type && !s_conflict && (~(|(i_cmask & o_a_wait_mask)) || a_to_b_vld);

endmodule

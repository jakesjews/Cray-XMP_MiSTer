//////////////////////////////////////////////////////////////////
//        Cray S-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling
//for instructions that utilize the scalar "S" Register file.
//
//What it has to know about the current instruction comes decoded, from the
//register func_top keeps beside CIP (cray_predecode, the fields PD_S_*).
//The results on their way are kept in res_lanes.v, a lane for every unit.

module s_scheduler (
	clk,
	rst,
	i_cip_vld,
	i_issue_vld,
	i_type,
	i_lane,
	i_dest,
	i_dnum,
	i_cmask,
	i_077,
	i_vw,
	i_mem_v,
	i_mem_d,
	o_s_issue,
	o_s_type,
	i_vreg_busy,
	o_vreg_write,
	o_head_v,
	o_head_hot,
	o_next_v,
	o_next_d,
	o_next_hot,
	o_s0_busy,
	o_s_res_mask,
	o_s_wait_mask
);
	`include "cray_types.vh"

	input wire clk;
	input wire rst;
	input wire i_cip_vld;
	input wire i_issue_vld;
	//the instruction in CIP
	input wire i_type;  // an S-type instruction
	input wire [SL_N-1:0] i_lane;  // the lane its result takes
	input wire [7:0] i_dest;  // the register the result goes to, one bit a register
	input wire [2:0] i_dnum;  // and as a number
	input wire [7:0] i_cmask;  // registers that must have no result on its way
	input wire i_077;  // Transmit (Sj) to Vi element (Ak)
	input wire [7:0] i_vw;  // the V register it writes
	//a word from memory is at the registers in the next clock, and the register it is for
	input wire i_mem_v;
	input wire [2:0] i_mem_d;

	output wire o_s_issue;
	output wire o_s_type;
	input wire [7:0] i_vreg_busy;
	output wire [7:0] o_vreg_write;
	//the lanes, for the S registers (res_regfile.v) and the units
	output wire [SL_N-1:0] o_head_v;
	output wire [8*SL_N-1:0] o_head_hot;
	output wire [SL_N-1:0] o_next_v;
	output wire [3*SL_N-1:0] o_next_d;
	output wire [8*SL_N-1:0] o_next_hot;
	output wire o_s0_busy;
	output wire [7:0] o_s_res_mask;  // registers with a result on its way or at the head of a lane
	output wire [7:0] o_s_wait_mask;  // registers with a result that is not at the head yet

	wire v_ok;  //it's okay to issue instruction 077

	//decoding for instruction 077: Transmit (Sj) to Vi element (Ak)
	assign v_ok         = |(i_vw & ~i_vreg_busy);
	assign o_vreg_write = {8{v_ok}} & i_vw;

	assign o_s_type = i_cip_vld && i_type;

	res_lanes #(
		.NL   (SL_N),
		.DELAY(SL_DELAY)
	) lanes (
		.clk        (clk),
		.rst        (rst),
		.i_issue    (i_issue_vld),
		.i_lane     ({SL_N{i_cip_vld}} & i_lane),
		.i_dest     (i_dest),
		.i_dnum     (i_dnum),
		.i_ext_v    ({{(SL_N - SL_MEM - 1) {1'b0}}, i_mem_v, {SL_MEM{1'b0}}}),
		.i_ext_d    ({{(3 * (SL_N - SL_MEM - 1)) {1'b0}}, i_mem_d, {(3 * SL_MEM) {1'b0}}}),
		.o_head_v   (o_head_v),
		.o_head_hot (o_head_hot),
		.o_next_v   (o_next_v),
		.o_next_d   (o_next_d),
		.o_next_hot (o_next_hot),
		.o_res_mask (o_s_res_mask),
		.o_wait_mask(o_s_wait_mask)
	);

	assign o_s0_busy = o_s_res_mask[0];

	//check if it's free to issue: "Si reserved" holds it, and 077 its V register
	assign o_s_issue = o_s_type && (v_ok || !i_077) && ~(|(i_cmask & o_s_wait_mask));

endmodule

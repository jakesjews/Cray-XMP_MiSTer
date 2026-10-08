//////////////////////////////////////////////////////////////////
//        Cray V-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling 
//for instructions that utilize the vector "V" Register Files,
//including all pipelining features
//
//What it has to know about the current instruction comes decoded, from the
//register func_top keeps beside CIP (cray_predecode, the fields PD_V_*).

module v_scheduler (
	i_cip_vld,
	i_v_type,
	i_vi,
	i_vj,
	i_vk,
	i_fu,
	o_vwrite_start,
	o_vread_start,
	o_vfu_start,
	o_v_issue,
	i_vreg_busy,
	i_vreg_reading,
	i_vfu_busy
);

	input wire i_cip_vld;  //CIP holds an instruction that may start
	input wire i_v_type;  //and it is a vector instruction, 140 to 177
	input wire [7:0] i_vi;  //the V register it writes
	input wire [7:0] i_vj;  //the V registers it reads
	input wire [7:0] i_vk;
	input wire [8:0] i_fu;  //the unit it uses
	output wire [7:0] o_vwrite_start;
	output wire [7:0] o_vread_start;
	output wire [8:0] o_vfu_start;
	output wire o_v_issue;
	input wire [7:0] i_vreg_busy;  //reserved as an operand or as a result
	input wire [7:0] i_vreg_reading;  //reserved as an operand
	input wire [8:0] i_vfu_busy;

	wire issue_vld;  //it's okay to issue the instruction

	//is it okay to issue the instruction? 
	//The result register must be free.  An operand register must not be the operand
	//of another instruction, but may be one that is still to receive a result, or is
	//receiving it (chaining, CSM-0111000 page 4-12): the operation takes the elements
	//as they come.  A vector transfer also waits for A0 and Ak, which is part of what
	//func_top holds every instruction for.
	wire vi_rdy = !(|(i_vi & i_vreg_busy));
	wire vj_rdy = !(|(i_vj & i_vreg_reading));
	wire vk_rdy = !(|(i_vk & i_vreg_reading));
	wire fu_rdy = |(~i_vfu_busy & i_fu);

	assign issue_vld = i_cip_vld && i_v_type && vi_rdy && vj_rdy && vk_rdy && fu_rdy;

	assign o_v_issue = issue_vld;

	//Let's figure out the actual 'vwrite_start', 'vread_start' and 'vfu_start' signals
	assign o_vwrite_start = {8{issue_vld}} & i_vi;
	assign o_vread_start  = {8{issue_vld}} & (i_vj | i_vk);
	assign o_vfu_start    = {9{issue_vld}} & i_fu;

endmodule

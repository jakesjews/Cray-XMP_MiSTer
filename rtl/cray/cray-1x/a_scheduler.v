//////////////////////////////////////////////////////////////////
//        Cray A-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling
//for instructions that utilize the address "A" Register file,
//including all pipelining features
//
//What it has to know about the current instruction comes decoded, from the
//register func_top keeps beside CIP (cray_predecode, the fields PD_A_*).

module a_scheduler (
	clk,
	rst,
	i_cip_vld,
	i_issue_vld,
	i_type,
	i_stage,
	i_src,
	i_dest,
	i_dnum,
	i_cmask,
	i_wpc,
	i_025,
	i_sconf,
	i_total_s_res_mask,
	o_a_issue,
	o_a_result_en,
	o_a_result_src,
	o_a_result_dest,
	o_a_type,
	o_a0_busy,
	o_a_res_mask
);

	input wire clk;
	input wire rst;
	input wire i_cip_vld;
	input wire i_issue_vld;
	//the instruction in CIP
	input wire i_type;  // an A-type instruction
	input wire [10:0] i_stage;  // the pipeline stage its result enters when it issues
	input wire [3:0] i_src;  // the unit the result comes from
	input wire [7:0] i_dest;  // the register the result goes to, one bit a register
	input wire [2:0] i_dnum;  // and as a number
	input wire [7:0] i_cmask;  // registers that must have no result on its way
	input wire [10:0] i_wpc;  // the stage that must be empty for the result to enter
	input wire i_025;  // Bjk <= Ai
	input wire [7:0] i_sconf;  // 023: the S register that must have no result on its way
	input wire [7:0] i_total_s_res_mask;
	output wire o_a_issue;
	output wire o_a_result_en;
	output wire [3:0] o_a_result_src;
	output wire [2:0] o_a_result_dest;
	output wire o_a_type;
	output wire o_a0_busy;
	output wire [7:0] o_a_res_mask;

	reg [10:0] a_result_pipe_en;  //the registers to pipeline the a_result_en signal
	reg [3:0] a_result_pipe_src[0:10];  //the unit the value comes from
	reg [7:0] a_result_pipe_dest[0:10];  //the a-register we're targeting
	reg [7:0] res_mask;
	wire a_to_b_vld;  //for executing 7'o025
	wire s_conflict;
	wire write_path_conflict;

	//Let's figure out if it's okay to issue the special case of the 7'o025 instruction (Bjk <= Ai)
	assign a_to_b_vld = i_025 && !(|(i_dest & res_mask));

	assign o_a_result_en  = a_result_pipe_en[0];
	assign o_a_result_src = a_result_pipe_src[0];

	//o_a_type get asserted for 7'b0_01?_??? and 7'b1_000_??? instructions, except for 7'b0_011_1??
	// which translates to: 020-037, 100-107, except for 034-037; and not for 027??7
	assign o_a_type = i_cip_vld && i_type;

	//the stage the result of the current instruction enters, and whether it does now
	wire [10:0] enters = {11{i_cip_vld}} & i_stage;
	wire [10:0] load = {11{i_issue_vld}} & enters;

	//Let's pipeline the A result_bus enable signals, the associated
	//'source' signals, and the destination signals.
	//We always want to advance the pipeline forward, even if there is a stall in cip_vld,
	//which happens every time there is a 2-parcel instruction. i_issue_vld is gated by a_type,
	//which looks at cip_vld, so it should be fine.
	genvar g;
	generate
		for (g = 0; g < 11; g = g + 1) begin : g_stage
			if (g == 10) begin : g_last
				always @(posedge clk)
					if (rst) begin
						a_result_pipe_en[g]   <= 1'b0;
						a_result_pipe_src[g]  <= 4'b0;
						a_result_pipe_dest[g] <= 8'b0;
					end else begin
						a_result_pipe_en[g]   <= load[g];
						a_result_pipe_src[g]  <= load[g] ? i_src : 4'b0;
						a_result_pipe_dest[g] <= load[g] ? i_dest : 8'b0;
					end
			end else begin : g_shift
				always @(posedge clk)
					if (rst) begin
						a_result_pipe_en[g]   <= 1'b0;
						a_result_pipe_src[g]  <= 4'b0;
						a_result_pipe_dest[g] <= 8'b0;
					end else begin
						a_result_pipe_en[g]   <= load[g] || a_result_pipe_en[g+1];
						a_result_pipe_src[g]  <= load[g] ? i_src : a_result_pipe_src[g+1];
						a_result_pipe_dest[g] <= load[g] ? i_dest : a_result_pipe_dest[g+1];
					end
			end
		end
	endgenerate

	//The destination at the head of the pipeline as a number, in a register of
	//its own beside the one-hot form, as in the S scheduler.
	function [2:0] number_of;
		input [7:0] one_hot;
		begin
			number_of = one_hot[0] ? 3'b000 :
					one_hot[1] ? 3'b001 :
					one_hot[2] ? 3'b010 :
					one_hot[3] ? 3'b011 :
					one_hot[4] ? 3'b100 :
					one_hot[5] ? 3'b101 :
					one_hot[6] ? 3'b110 :
					one_hot[7] ? 3'b111 : 3'b000;
		end
	endfunction

	reg [2:0] head_dest;
	always @(posedge clk)
		if (rst) head_dest <= 3'b000;
		else head_dest <= load[0] ? i_dnum : number_of(a_result_pipe_dest[1]);

	assign o_a_result_dest = head_dest;

	//All the registers that results are on their way to, in a register of its own,
	//loaded with what the pipeline holds after this clock, as in the S scheduler.
	reg     [7:0] mask_held;
	integer       m;
	always @* begin
		mask_held = 8'b0;
		for (m = 1; m < 11; m = m + 1) mask_held = mask_held | a_result_pipe_dest[m];
	end
	always @(posedge clk)
		if (rst) res_mask <= 8'b0;
		else res_mask <= mask_held | ({8{|load}} & i_dest);

	assign o_a_res_mask = res_mask;  //the memory unit needs to know if there is a conflict
	assign o_a0_busy    = res_mask[0];

	//check if it's free to issue
	assign write_path_conflict = |(i_wpc & a_result_pipe_en);

	assign s_conflict = |(i_sconf & i_total_s_res_mask);

	assign o_a_issue = !write_path_conflict && o_a_type && !s_conflict && (~(|(i_cmask & res_mask)) || a_to_b_vld);

endmodule

//////////////////////////////////////////////////////////////////
//        Cray S-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling
//for instructions that utilize the scalar "S" Register file,
//including all pipelining features
//
//What it has to know about the current instruction comes decoded, from the
//register func_top keeps beside CIP (cray_predecode, the fields PD_S_*).

module s_scheduler (
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
	i_077,
	i_vw,
	o_s_issue,
	o_s_result_en,
	o_s_result_src,
	o_s_result_dest,
	o_s_result_slot,
	o_s_next_en,
	o_s_next_src,
	o_s_type,
	i_vreg_busy,
	o_vreg_write,
	o_s0_busy,
	o_s_res_mask,
	o_s_wait_mask
);

	input wire clk;
	input wire rst;
	input wire i_cip_vld;
	input wire i_issue_vld;
	//the instruction in CIP
	input wire i_type;  // an S-type instruction
	input wire [13:0] i_stage;  // the pipeline stage its result enters when it issues
	input wire [4:0] i_src;  // the unit the result comes from
	input wire [7:0] i_dest;  // the register the result goes to, one bit a register
	input wire [2:0] i_dnum;  // and as a number
	input wire [7:0] i_cmask;  // registers that must have no result on its way
	input wire [13:0] i_wpc;  // the stage that must be empty for the result to enter
	input wire i_077;  // Transmit (Sj) to Vi element (Ak)
	input wire [7:0] i_vw;  // the V register it writes

	output wire o_s_issue;
	output wire o_s_result_en;
	output wire [4:0] o_s_result_src;
	output wire [2:0] o_s_result_dest;
	output wire [1:0] o_s_result_slot;  // which result register the head entry's value is in
	output wire o_s_next_en;  // the entry behind the head: its result is due in the next clock
	output wire [4:0] o_s_next_src;
	output wire o_s_type;
	input wire [7:0] i_vreg_busy;
	output wire [7:0] o_vreg_write;
	output wire o_s0_busy;
	output wire [7:0] o_s_res_mask;  // registers with a result on its way or on the bus
	output wire [7:0] o_s_wait_mask;  // registers with a result that is not on the bus yet

	reg [13:0] s_result_pipe_en;  //the registers to pipeline the s_result_en signal
	reg [4:0] s_result_pipe_src[0:13];  //the src of our value to write
	reg [7:0] s_result_pipe_dest[0:13];  //the s-register we're targeting
	wire v_ok;  //it's okay to issue instruction 077
	wire write_path_conflict;

	//decoding for instruction 077: Transmit (Sj) to Vi element (Ak)
	assign v_ok         = |(i_vw & ~i_vreg_busy);
	assign o_vreg_write = {8{v_ok}} & i_vw;

	//outputs to control instruction issue and the S regfile
	assign o_s_result_en  = s_result_pipe_en[0];
	assign o_s_result_src = s_result_pipe_src[0];
	assign o_s_type       = i_cip_vld && i_type;

	//the stage the result of the current instruction enters, and whether it does now
	wire [13:0] enters = {14{i_cip_vld}} & i_stage;
	wire [13:0] load = {14{i_issue_vld}} & enters;

	//Let's pipeline the S result_bus enable signals, the associated
	//'source' signals, and the destination signals.
	//We always want to advance the pipeline forward, even if there is a stall in cip_vld,
	//which happens every time there is a 2-parcel instruction. i_issue_vld is gated by s_type,
	//which looks at cip_vld, so it should be fine.
	genvar g;
	generate
		for (g = 0; g < 14; g = g + 1) begin : g_stage
			if (g == 13) begin : g_last
				always @(posedge clk)
					if (rst) begin
						s_result_pipe_en[g]   <= 1'b0;
						s_result_pipe_src[g]  <= 5'b0;
						s_result_pipe_dest[g] <= 8'b0;
					end else begin
						s_result_pipe_en[g]   <= load[g];
						s_result_pipe_src[g]  <= load[g] ? i_src : 5'b0;
						s_result_pipe_dest[g] <= load[g] ? i_dest : 8'b0;
					end
			end else begin : g_shift
				always @(posedge clk)
					if (rst) begin
						s_result_pipe_en[g]   <= 1'b0;
						s_result_pipe_src[g]  <= 5'b0;
						s_result_pipe_dest[g] <= 8'b0;
					end else begin
						s_result_pipe_en[g]   <= load[g] || s_result_pipe_en[g+1];
						s_result_pipe_src[g]  <= load[g] ? i_src : s_result_pipe_src[g+1];
						s_result_pipe_dest[g] <= load[g] ? i_dest : s_result_pipe_dest[g+1];
					end
			end
		end
	endgenerate

	//The destination at the head of the pipeline as a number.  The register file
	//compares it with the registers being read, which is on the way of every
	//result into every unit, so the number is kept in a register of its own
	//beside the one-hot form: it is formed a clock earlier, from the instruction
	//that issues or from the entry behind the head.
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
		else head_dest <= load[0] ? i_dnum : number_of(s_result_pipe_dest[1]);

	assign o_s_result_dest = head_dest;

	//Which result register holds the value of the head entry (the S result bus in
	//func_top), kept the same way.  The entry behind the head tells func_top which
	//unit to gather a result from during this clock.
	`include "cray_types.vh"

	function [1:0] slot_of;
		input [4:0] src;
		begin
			slot_of = (src == SBUS_S_LOG) ? SSLOT_LOG : (src == SBUS_S_SHIFT) ? SSLOT_SHIFT : (src == SBUS_FP_ADD) ? SSLOT_FADD : SSLOT_BUS;
		end
	endfunction

	reg [1:0] head_slot;
	always @(posedge clk) head_slot <= load[0] ? slot_of(i_src) : slot_of(s_result_pipe_src[1]);

	assign o_s_result_slot = head_slot;
	assign o_s_next_en     = s_result_pipe_en[1];
	assign o_s_next_src    = s_result_pipe_src[1];

	//All the registers that results are on their way to, in a register of its own.
	//It is loaded with what the pipeline holds after this clock: what moves up from
	//the stages behind the head, and the destination of the instruction that issues.
	//(The stage a result enters is empty, or the instruction would not issue.)  The
	//issue decision starts from this mask; gathering it from the fourteen stages there
	//cost two levels of logic at the head of the longest way in the machine.
	reg     [7:0] res_mask;
	reg     [7:0] mask_held;
	integer       m;
	always @* begin
		mask_held = 8'b0;
		for (m = 1; m < 14; m = m + 1) mask_held = mask_held | s_result_pipe_dest[m];
	end
	always @(posedge clk)
		if (rst) res_mask <= 8'b0;
		else res_mask <= mask_held | ({8{|load}} & i_dest);

	assign o_s_res_mask = res_mask;

	//The same without the head of the pipeline.  A register whose result is on the bus
	//in this clock is free for the instruction that issues in this clock, as an
	//operand (it takes the result off the bus) and as the place of its own result:
	//the register is reserved for the unit's time and no longer.
	reg [7:0] wait_mask;
	reg [7:0] wait_held;
	always @* begin
		wait_held = 8'b0;
		for (m = 2; m < 14; m = m + 1) wait_held = wait_held | s_result_pipe_dest[m];
	end
	always @(posedge clk)
		if (rst) wait_mask <= 8'b0;
		else wait_mask <= wait_held | ({8{|load[13:1]}} & i_dest);

	assign o_s_wait_mask = wait_mask;
	assign o_s0_busy     = res_mask[0];

	//check if it's free to issue
	//We currently catch register conflicts, but we need a way to check if an instruction
	//is going to complete at the same time as one already in-flight, since we can only
	//retire one instruction per cycle
	assign write_path_conflict = |(i_wpc & s_result_pipe_en);

	assign o_s_issue = o_s_type && !write_path_conflict && (v_ok || !i_077) && ~(|(i_cmask & wait_mask));

endmodule

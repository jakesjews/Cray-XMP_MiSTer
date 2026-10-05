//////////////////////////////////////////////////////////////////
//        Cray A-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling 
//for instructions that utilize the address "A" Register file,
//including all pipelining features

module a_scheduler (
	clk,
	rst,
	i_cip,
	i_cip_vld,
	i_lip_vld,
	i_issue_vld,
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
	input wire [15:0] i_cip;
	input wire i_cip_vld;
	input wire i_lip_vld;
	input wire i_issue_vld;
	input wire [7:0] i_total_s_res_mask;
	output wire o_a_issue;
	output wire o_a_result_en;
	output wire [3:0] o_a_result_src;
	output wire [2:0] o_a_result_dest;
	output wire o_a_type;
	output wire o_a0_busy;
	output wire [7:0] o_a_res_mask;

	wire [2:0] cip_i, cip_j, cip_k;
	wire [6:0] cip_instr;
	reg [7:0] cip_i_one_hot, cip_j_one_hot, cip_k_one_hot;
	wire [7:0] total_a_res_mask;
	wire [3:0] a_result_delay;
	wire a_result_en;
	reg [10:0] a_result_pipe_en;  //the registers to pipeline the a_result_en signal
	reg [43:0] a_result_pipe_src;  //the destination a-reg
	reg [7:0] a_result_pipe_dest[10:0];  //the a-register we're targeting
	wire [3:0] a_result_src;
	wire a_to_b_vld;  //for executing 7'o025
	wire s_conflict;
	wire write_path_conflict;

	assign o_a_res_mask = total_a_res_mask;  //the memory unit needs to know if there is a conflict

	//This look-up table provides info to the A-bus pipeline
	a_res_lut abus_res_lut (
		.i_cip      (i_cip),
		.o_delay    (a_result_delay),
		.o_src      (a_result_src),
		.o_a_dest_en(a_result_en)
	);


	assign cip_instr = i_cip[15:9];
	assign cip_i     = i_cip[8:6];
	assign cip_j     = i_cip[5:3];
	assign cip_k     = i_cip[2:0];

	//Let's figure out if it's okay to issue the special case of the 7'o025 instruction (Bjk <= Ai)
	assign a_to_b_vld = (cip_instr == 7'o025) && !total_a_res_mask[cip_i];

	assign o_a_result_en  = a_result_pipe_en[0];
	assign o_a_result_src = a_result_pipe_src[3:0];


	assign o_a_result_dest = a_result_pipe_dest[0][0] ? 3'b000 : 
						 a_result_pipe_dest[0][1] ? 3'b001 :
								 a_result_pipe_dest[0][2] ? 3'b010 :
								 a_result_pipe_dest[0][3] ? 3'b011 :
								 a_result_pipe_dest[0][4] ? 3'b100 :
								 a_result_pipe_dest[0][5] ? 3'b101 :
								 a_result_pipe_dest[0][6] ? 3'b110 :
								 a_result_pipe_dest[0][7] ? 3'b111 : 3'b000;

	//o_a_type get asserted for 7'b0_01?_??? and 7'b1_000_??? instructions, except for 7'b0_011_1??	
	// which translates to: 020-037, 100-107, except for 034-037
	assign o_a_type = i_cip_vld && (((cip_instr[6:4] == 3'b001) &&  //A-type instructions
		(cip_instr[6:2] != 5'b00111)) ||  //(except the memory ones)
		(cip_instr[6:3] == 4'b1000)) &&  //or when we write to A from mem
		!((i_cip[15:9] == 7'b0010111) && (i_cip[2:0] == 3'b111));  //And not 027??7

	//Let's pipeline the A result_bus enable signals, the associated
	//'source' signals, and the destination signals
	always @(posedge clk)
		if (rst) begin
			a_result_pipe_en       <= 11'b0;
			a_result_pipe_src      <= 44'b0;
			a_result_pipe_dest[0]  <= 0;
			a_result_pipe_dest[1]  <= 0;
			a_result_pipe_dest[2]  <= 0;
			a_result_pipe_dest[3]  <= 0;
			a_result_pipe_dest[4]  <= 0;
			a_result_pipe_dest[5]  <= 0;
			a_result_pipe_dest[6]  <= 0;
			a_result_pipe_dest[7]  <= 0;
			a_result_pipe_dest[8]  <= 0;
			a_result_pipe_dest[9]  <= 0;
			a_result_pipe_dest[10] <= 0;
		end  //We always want to advance the pipeline forward, even if there is a stall in cip_vld,
			 //which happens every time there is a 2-parcel instruction. i_issue_vld is gated by a_type,
			 //which looks at cip_vld, so it should be fine.
		else begin
			a_result_pipe_en[0]  <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd1)) ? 1'b1 : a_result_pipe_en[1];
			a_result_pipe_en[1]  <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd2)) ? 1'b1 : a_result_pipe_en[2];
			a_result_pipe_en[2]  <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd3)) ? 1'b1 : a_result_pipe_en[3];
			a_result_pipe_en[3]  <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd4)) ? 1'b1 : a_result_pipe_en[4];
			a_result_pipe_en[4] <= a_result_pipe_en[5];
			a_result_pipe_en[5]  <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd6)) ? 1'b1 : a_result_pipe_en[6];
			a_result_pipe_en[6] <= a_result_pipe_en[7];
			a_result_pipe_en[7] <= a_result_pipe_en[8];
			a_result_pipe_en[8] <= a_result_pipe_en[9];
			a_result_pipe_en[9] <= a_result_pipe_en[10];
			a_result_pipe_en[10] <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay == 4'd11)) ? 1'b1 : 1'b0;

			a_result_pipe_src[3:0]     <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd1)) ? a_result_src : a_result_pipe_src[7:4];
			a_result_pipe_src[7:4]     <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd2)) ? a_result_src : a_result_pipe_src[11:8];
			a_result_pipe_src[11:8]   <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd3)) ? a_result_src : a_result_pipe_src[15:12];
			a_result_pipe_src[15:12] <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd4)) ? a_result_src : a_result_pipe_src[19:16];
			a_result_pipe_src[19:16] <= a_result_pipe_src[23:20];
			a_result_pipe_src[23:20] <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd6)) ? a_result_src : a_result_pipe_src[27:24];
			a_result_pipe_src[27:24] <= a_result_pipe_src[31:28];
			a_result_pipe_src[31:28] <= a_result_pipe_src[35:32];
			a_result_pipe_src[35:32] <= a_result_pipe_src[39:36];
			a_result_pipe_src[39:36] <= a_result_pipe_src[43:40];
			a_result_pipe_src[43:40] <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd11)) ? a_result_src : 4'b0;

			a_result_pipe_dest[0] <= (o_a_type && a_result_en && i_cip_vld && i_issue_vld && (a_result_delay==4'd1)) ? cip_i_one_hot : a_result_pipe_dest[1];
			a_result_pipe_dest[1] <= (o_a_type && a_result_en && i_cip_vld && i_issue_vld && (a_result_delay==4'd2)) ? cip_i_one_hot : a_result_pipe_dest[2];
			a_result_pipe_dest[2] <= (o_a_type && a_result_en && i_cip_vld && i_issue_vld && (a_result_delay==4'd3)) ? cip_i_one_hot : a_result_pipe_dest[3];
			a_result_pipe_dest[3] <= (o_a_type && a_result_en && i_cip_vld && i_issue_vld && (a_result_delay==4'd4)) ? cip_i_one_hot : a_result_pipe_dest[4];
			a_result_pipe_dest[4] <= a_result_pipe_dest[5];
			a_result_pipe_dest[5] <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd6)) ? cip_i_one_hot : a_result_pipe_dest[6];
			a_result_pipe_dest[6] <= a_result_pipe_dest[7];
			a_result_pipe_dest[7] <= a_result_pipe_dest[8];
			a_result_pipe_dest[8] <= a_result_pipe_dest[9];
			a_result_pipe_dest[9] <= a_result_pipe_dest[10];
			a_result_pipe_dest[10] <= (o_a_type && a_result_en && i_issue_vld && (a_result_delay==4'd11)) ? cip_i_one_hot : 8'b0;
		end


	assign total_a_res_mask = (a_result_pipe_dest[0] |
						   a_result_pipe_dest[1] |
						   a_result_pipe_dest[2] |
						   a_result_pipe_dest[3] |
						   a_result_pipe_dest[4] |
						   a_result_pipe_dest[5] |
						   a_result_pipe_dest[6] |
						   a_result_pipe_dest[7] |
						   a_result_pipe_dest[8] |
						   a_result_pipe_dest[9] |
						   a_result_pipe_dest[10]);

	assign o_a0_busy = total_a_res_mask[0];


	always @* begin
		case (cip_i)
			3'b000: cip_i_one_hot = 8'b00000001;
			3'b001: cip_i_one_hot = 8'b00000010;
			3'b010: cip_i_one_hot = 8'b00000100;
			3'b011: cip_i_one_hot = 8'b00001000;
			3'b100: cip_i_one_hot = 8'b00010000;
			3'b101: cip_i_one_hot = 8'b00100000;
			3'b110: cip_i_one_hot = 8'b01000000;
			3'b111: cip_i_one_hot = 8'b10000000;
		endcase
	end

	always @* begin
		case (cip_j)
			3'b000: cip_j_one_hot = 8'b00000001;
			3'b001: cip_j_one_hot = 8'b00000010;
			3'b010: cip_j_one_hot = 8'b00000100;
			3'b011: cip_j_one_hot = 8'b00001000;
			3'b100: cip_j_one_hot = 8'b00010000;
			3'b101: cip_j_one_hot = 8'b00100000;
			3'b110: cip_j_one_hot = 8'b01000000;
			3'b111: cip_j_one_hot = 8'b10000000;
		endcase
	end

	always @* begin
		case (cip_k)
			3'b000: cip_k_one_hot = 8'b00000001;
			3'b001: cip_k_one_hot = 8'b00000010;
			3'b010: cip_k_one_hot = 8'b00000100;
			3'b011: cip_k_one_hot = 8'b00001000;
			3'b100: cip_k_one_hot = 8'b00010000;
			3'b101: cip_k_one_hot = 8'b00100000;
			3'b110: cip_k_one_hot = 8'b01000000;
			3'b111: cip_k_one_hot = 8'b10000000;
		endcase
	end


	//check if it's free to issue
	assign write_path_conflict = (a_result_delay != 4'd11) && a_result_pipe_en[a_result_delay[3:0]];

	assign s_conflict = (i_cip[15:9] == 7'b0010011) && (i_cip[2:0] == 3'b0) && i_total_s_res_mask[i_cip[5:3]];

	assign o_a_issue = !write_path_conflict && o_a_type && !s_conflict && (~(|((cip_i_one_hot|cip_j_one_hot|cip_k_one_hot) & total_a_res_mask)) || a_to_b_vld);

endmodule

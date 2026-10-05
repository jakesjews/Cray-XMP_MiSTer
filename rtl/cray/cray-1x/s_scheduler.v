//////////////////////////////////////////////////////////////////
//        Cray S-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling 
//for instructions that utilize the scalar "S" Register file,
//including all pipelining features

module s_scheduler (
	clk,
	rst,
	i_cip,
	i_cip_vld,
	i_issue_vld,
	o_s_issue,
	o_s_result_en,
	o_s_result_src,
	o_s_result_dest,
	o_s_type,
	i_vreg_busy,
	o_vreg_write,
	o_s0_busy,
	o_s_res_mask
);

	input wire clk;
	input wire rst;
	input wire [15:0] i_cip;
	input wire i_cip_vld;
	input wire i_issue_vld;

	output wire o_s_issue;
	output wire o_s_result_en;
	output wire [4:0] o_s_result_src;
	output wire [2:0] o_s_result_dest;
	output wire o_s_type;
	input wire [7:0] i_vreg_busy;
	output wire [7:0] o_vreg_write;
	output wire o_s0_busy;
	output wire [7:0] o_s_res_mask;

	wire [2:0] cip_i, cip_j, cip_k;
	wire [6:0] cip_instr;
	reg [7:0] cip_i_one_hot, cip_j_one_hot, cip_k_one_hot;
	wire [7:0] total_s_res_mask;
	wire [4:0] cip_src;
	wire [3:0] s_result_delay;
	wire s_result_en;
	reg [13:0] s_result_pipe_en;  //the registers to pipeline the s_result_en signal
	reg [69:0] s_result_pipe_src;  //the src of our value to write
	reg [7:0] s_result_pipe_dest[13:0];  //the s-register we're targeting
	wire v_ok;  //it's okay to issue instruction 077
	wire write_path_conflict;
	wire [7:0] dest_one_hot;  // the register the result goes to

	assign o_s_res_mask = total_s_res_mask;


	//This look-up table provides info to the S-bus pipeline
	s_res_lut sbus_res_lut (
		.i_cip      (i_cip),
		.o_delay    (s_result_delay),
		.o_src      (cip_src),
		.o_s_dest_en(s_result_en)
	);


	assign cip_instr = i_cip[15:9];
	assign cip_i     = i_cip[8:6];
	assign cip_j     = i_cip[5:3];
	assign cip_k     = i_cip[2:0];

	//decoding for instruction 077: Transmit (Sj) to Vi element (Ak)
	assign v_ok         = ((cip_instr == 7'b0111111) && |(cip_i_one_hot & ~i_vreg_busy));
	assign o_vreg_write = {8{v_ok}} & cip_i_one_hot;



	//outputs to control instruction issue and the S regfile
	assign o_s_result_en = s_result_pipe_en[0];
	assign o_s_result_src = s_result_pipe_src[4:0];
	assign o_s_type = i_cip_vld && (((cip_instr[6:5]==2'b01) && 
								!(cip_instr==7'b0111101) &&
										  !((i_cip[15:9]==7'o073) && ((i_cip[5:0]==6'o02) || (i_cip[2:0]==3'o3))))  || 
				   (cip_instr[6:3]==4'b1010));


	assign o_s_result_dest = s_result_pipe_dest[0][0] ? 3'b000 : 
						 s_result_pipe_dest[0][1] ? 3'b001 :
								 s_result_pipe_dest[0][2] ? 3'b010 :
								 s_result_pipe_dest[0][3] ? 3'b011 :
								 s_result_pipe_dest[0][4] ? 3'b100 :
								 s_result_pipe_dest[0][5] ? 3'b101 :
								 s_result_pipe_dest[0][6] ? 3'b110 :
								 s_result_pipe_dest[0][7] ? 3'b111 : 3'b000;
	/*						 
always@*
   begin
      case(s_result_pipe_dest[0])
	      8'b00000001: o_s_result_dest = 3'b000;
		   8'b00000010: o_s_result_dest = 3'b001;
		   8'b00000100: o_s_result_dest = 3'b010;
		   8'b00001000: o_s_result_dest = 3'b011;
		   8'b00010000: o_s_result_dest = 3'b100;
		   8'b00100000: o_s_result_dest = 3'b101;
		   8'b01000000: o_s_result_dest = 3'b110;
		   8'b10000000: o_s_result_dest = 3'b111;
		   default:     o_s_result_dest = 3'b000;
	   endcase
   end
*/

	//Let's pipeline the S result_bus enable signals, the associated
	//'source' signals, and the destination signals
	always @(posedge clk)
		if (rst) begin
			s_result_pipe_en        <= 14'b0;
			s_result_pipe_src[55:0] <= 56'b0;
			s_result_pipe_dest[0]   <= 0;
			s_result_pipe_dest[1]   <= 0;
			s_result_pipe_dest[2]   <= 0;
			s_result_pipe_dest[3]   <= 0;
			s_result_pipe_dest[4]   <= 0;
			s_result_pipe_dest[5]   <= 0;
			s_result_pipe_dest[6]   <= 0;
			s_result_pipe_dest[7]   <= 0;
			s_result_pipe_dest[8]   <= 0;
			s_result_pipe_dest[9]   <= 0;
			s_result_pipe_dest[10]  <= 0;
			s_result_pipe_dest[11]  <= 0;
			s_result_pipe_dest[12]  <= 0;
			s_result_pipe_dest[13]  <= 0;
		end  //We always want to advance the pipeline forward, even if there is a stall in cip_vld,
			 //which happens every time there is a 2-parcel instruction. i_issue_vld is gated by s_type,
			 //which looks at cip_vld, so it should be fine.	
		else  //if(i_cip_vld)
		begin
			s_result_pipe_en[0] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd1)) ? 1'b1 : s_result_pipe_en[1];
			s_result_pipe_en[1] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd2)) ? 1'b1 : s_result_pipe_en[2];
			s_result_pipe_en[2] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd3)) ? 1'b1 : s_result_pipe_en[3];
			s_result_pipe_en[3] <= s_result_pipe_en[4];
			s_result_pipe_en[4] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd5)) ? 1'b1 : s_result_pipe_en[5];
			s_result_pipe_en[5] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd6)) ? 1'b1 : s_result_pipe_en[6];
			s_result_pipe_en[6] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd7)) ? 1'b1 : s_result_pipe_en[7];
			s_result_pipe_en[7] <= s_result_pipe_en[8];
			s_result_pipe_en[8] <= s_result_pipe_en[9];
			s_result_pipe_en[9] <= s_result_pipe_en[10];
			s_result_pipe_en[10] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd11)) ? 1'b1 : s_result_pipe_en[11];
			s_result_pipe_en[11] <= s_result_pipe_en[12];
			s_result_pipe_en[12] <= s_result_pipe_en[13];
			s_result_pipe_en[13] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay == 4'd14)) ? 1'b1 : 1'b0;

			s_result_pipe_src[4:0]    <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd1)) ? cip_src : s_result_pipe_src[9:5];
			s_result_pipe_src[9:5]    <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd2)) ? cip_src : s_result_pipe_src[14:10];
			s_result_pipe_src[14:10] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd3)) ? cip_src : s_result_pipe_src[19:15];
			s_result_pipe_src[19:15] <= s_result_pipe_src[24:20];
			s_result_pipe_src[24:20] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd5)) ? cip_src : s_result_pipe_src[29:25];
			s_result_pipe_src[29:25] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd6)) ? cip_src : s_result_pipe_src[34:30];
			s_result_pipe_src[34:30] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd7)) ? cip_src : s_result_pipe_src[39:35];
			s_result_pipe_src[39:35] <= s_result_pipe_src[44:40];
			s_result_pipe_src[44:40] <= s_result_pipe_src[49:45];
			s_result_pipe_src[49:45] <= s_result_pipe_src[54:50];
			s_result_pipe_src[54:50] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd11)) ? cip_src : s_result_pipe_src[59:55];
			s_result_pipe_src[59:55] <= s_result_pipe_src[64:60];
			s_result_pipe_src[64:60] <= s_result_pipe_src[69:65];
			s_result_pipe_src[69:65] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd14)) ? cip_src : 5'b0;

			s_result_pipe_dest[0] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd1)) ? dest_one_hot : s_result_pipe_dest[1];
			s_result_pipe_dest[1] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd2)) ? dest_one_hot : s_result_pipe_dest[2];
			s_result_pipe_dest[2] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd3)) ? dest_one_hot : s_result_pipe_dest[3];
			s_result_pipe_dest[3] <= s_result_pipe_dest[4];
			s_result_pipe_dest[4] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd5)) ? dest_one_hot : s_result_pipe_dest[5];
			s_result_pipe_dest[5] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd6)) ? dest_one_hot : s_result_pipe_dest[6];
			s_result_pipe_dest[6] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd7)) ? dest_one_hot : s_result_pipe_dest[7];
			s_result_pipe_dest[7] <= s_result_pipe_dest[8];
			s_result_pipe_dest[8] <= s_result_pipe_dest[9];
			s_result_pipe_dest[9] <= s_result_pipe_dest[10];
			s_result_pipe_dest[10] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd11)) ? dest_one_hot : s_result_pipe_dest[11];
			s_result_pipe_dest[11] <= s_result_pipe_dest[12];
			s_result_pipe_dest[12] <= s_result_pipe_dest[13];
			s_result_pipe_dest[13] <= (o_s_type && s_result_en && i_issue_vld && (s_result_delay==4'd14)) ? dest_one_hot : 8'b0;
		end

	assign total_s_res_mask = (s_result_pipe_dest[0] | 
						   s_result_pipe_dest[1] |
						   s_result_pipe_dest[2] |
						   s_result_pipe_dest[3] |
						   s_result_pipe_dest[4] |
						   s_result_pipe_dest[5] |
						   s_result_pipe_dest[6] |
						   s_result_pipe_dest[7] |
						   s_result_pipe_dest[8] |
						   s_result_pipe_dest[9] |
						   s_result_pipe_dest[10]|
						   s_result_pipe_dest[11]|
						   s_result_pipe_dest[12]|
						   s_result_pipe_dest[13]);

	assign o_s0_busy = total_s_res_mask[0];

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
	//We currently catch register conflicts, but we need a way to check if an instruction
	//is going to complete at the same time as one already in-flight, since we can only
	//retire one instruction per cycle
	assign write_path_conflict = (s_result_delay != 4'd14) && s_result_pipe_en[s_result_delay[3:0]];

	assign o_s_issue = o_s_type && !write_path_conflict &&  
		((v_ok && (cip_instr==7'b0111111)) || (cip_instr!=7'b0111111)) && 
		  ~(|((cip_i_one_hot|cip_j_one_hot|cip_k_one_hot|dest_one_hot) & total_s_res_mask));

	//052 and 053 shift (Si) but put the result in S0
	assign dest_one_hot = ((cip_instr == 7'o052) || (cip_instr == 7'o053)) ? 8'b00000001 : cip_i_one_hot;

endmodule

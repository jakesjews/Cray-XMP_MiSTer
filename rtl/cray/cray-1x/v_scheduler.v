//////////////////////////////////////////////////////////////////
//        Cray V-register Scheduler                             //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block controls instruction-issue and scheduling 
//for instructions that utilize the vector "V" Register Files,
//including all pipelining features

module v_scheduler(i_cip,
                 i_cip_vld, 
					  i_a_res_mask,
					  o_fu_delay, 
					  o_fu, 
					  o_vwrite_start,
					  o_vread_start, 
					  o_vfu_start,
					  o_v_issue,
					  i_vreg_busy,
					  i_vreg_chain_n,
					  i_vfu_busy);
					  
	input wire [15:0] i_cip;
	input wire        i_cip_vld;
	input wire [7:0]  i_a_res_mask;
	output reg [3:0]  o_fu_delay;
   output reg [2:0]  o_fu;
	output wire[7:0]  o_vwrite_start;
	output wire[7:0]  o_vread_start;
	output wire[7:0]  o_vfu_start;
	output wire       o_v_issue;
	input  wire [7:0] i_vreg_busy;
	input  wire [7:0] i_vreg_chain_n;
	input  wire [7:0] i_vfu_busy;



reg  [7:0] vi_one_hot;
reg  [7:0] vj_one_hot;
reg  [7:0] vk_one_hot;
reg  [7:0] vfu_one_hot;
reg        vi_en, vj_en, vk_en;
wire       v_type;
wire       issue_vld;    //it's okay to issue the instruction
//On a CRAY-1 every 174ijx is the reciprocal; the X-MP uses k = 1 and 2 for population counts
parameter XMP = 0;
wire [15:0] dec_cip = (!XMP && (i_cip[15:9]==7'o174)) ? {i_cip[15:3],3'b000} : i_cip;



localparam VLOG      = 3'b000,   //vector logical
           VSHIFT    = 3'b001,	 //vector shift
			  VADD      = 3'b010,
	        FP_MUL    = 3'b011,   //FP multiply
	        FP_ADD    = 3'b100,   //FP adder 
	        FP_RA     = 3'b101,   //FP recip. approx.
	        VPOP      = 3'b110,   //vector pop count / parity
	         MEM      = 3'b111;

//is it okay to issue the instruction? 
wire reg_conflict;
wire vi_rdy;
wire vj_rdy;
wire vk_rdy;
wire fu_rdy;
assign vi_rdy = (vi_en && (|(vi_one_hot & ~i_vreg_busy)) || ~vi_en);
assign vj_rdy = (vj_en && (|(vj_one_hot & ~(i_vreg_busy & i_vreg_chain_n))) || ~vj_en);
assign vk_rdy = (vk_en && (|(vk_one_hot & ~(i_vreg_busy & i_vreg_chain_n))) || ~vk_en);
assign fu_rdy = |(~i_vfu_busy & vfu_one_hot);
assign v_type = i_cip[15:14]==2'b11;
assign reg_conflict = (i_cip_vld && (i_cip[15:10]==6'b111111)) ?  (i_a_res_mask[0] || i_a_res_mask[i_cip[2:0]]): 1'b0;
assign issue_vld = i_cip_vld && v_type && vi_rdy && vj_rdy && vk_rdy && fu_rdy && !reg_conflict;
assign o_v_issue = issue_vld;



//Let's figure out the actual 'vwrite_start', 'vread_start' and 'vfu_start' signals
assign o_vwrite_start = {8{(issue_vld & vi_en)}} & vi_one_hot;
assign o_vread_start  = ({8{(issue_vld && vj_en)}} & vj_one_hot) | ({8{(issue_vld && vk_en)}} & vk_one_hot);
assign o_vfu_start    = {8{issue_vld}} & vfu_one_hot; 



//Figure out if it's a vector instruction!



//figure out the delay
always@*
begin
   casez(dec_cip)
	   //140-147
      16'b1100????????????:begin
									  o_fu_delay = 4'd2; //vector logical
									  o_fu = VLOG;       //vector logical
									  vi_en = 1'b1;      //vi always enabled
									  vk_en = 1'b1;      //vk always enabled
									  vj_en = i_cip[9];  //vj enabled for odd instructions, S for even instructions
								  end
		//150-153
      16'b11010???????????:begin
                    		     o_fu_delay = 4'd4; //vector shift
									  o_fu = VSHIFT;
                             vi_en = 1'b1;
                             vj_en = 1'b1;
                             vk_en = 1'b0;		
								  end
		//154-157
		16'b11011???????????:begin
     		                    o_fu_delay = 4'd3; //vector add
									  o_fu = VADD; 
									  vi_en = 1'b1;
									  vk_en = 1'b1;
									  vj_en = i_cip[9];  //vj enabled for odd, sj for even
								  end
		//160-167
		16'b1110????????????:begin
                 		        o_fu_delay = 4'd7; //FP mul
									  o_fu = FP_MUL; 
									  vi_en = 1'b1;
									  vk_en = 1'b1;
									  vj_en = i_cip[9];  //vj enabled for odd, sj for even
								  end
		//170-173						  
		16'b11110???????????:begin
 		                       o_fu_delay = 4'd6; //FP add
									  o_fu = FP_ADD;
									  vi_en = 1'b1;
									  vk_en = 1'b1;
									  vj_en = i_cip[9];  //vj enabled for odd, sj for even
								  end
		//174ij0 - floating point reciprocal approximation						  
		16'b1111100??????000:begin
		                       o_fu_delay = 4'd14;
									  o_fu = FP_RA;
									  vi_en = 1'b1;
									  vk_en = 1'b0;
									  vj_en = 1'b1;
								  end
		//174ij1	- population count of (Vj elements) to Vi elements					  
		16'b1111100??????001:begin
  		                       o_fu_delay = 4'd6;
									  o_fu = VPOP;
									  vi_en = 1'b1;
									  vj_en = 1'b1;
									  vk_en = 1'b0;
								  end
		//174ij2 - population count parity of (Vj elements) to Vi elements						   
		16'b1111100??????010:begin 
		                       o_fu_delay = 4'd6;
									  o_fu = VPOP;
									  vi_en = 1'b1;
									  vj_en = 1'b1;
									  vk_en = 1'b0;
								  end
		//175xj0-175xj3 - create vector mask based on the results of testing the Vj register						  
		16'b1111101?????????:begin
           		              o_fu_delay = 4'd2; //vector logical
		                       o_fu = VLOG; //vector logical
									  vi_en = 1'b0;
									  vj_en = 1'b1;
									  vk_en = 1'b0;
								  end
		//176ixk-177xj0						  
		16'b111111??????????:begin
		                       o_fu_delay = 4'd2;
		                       o_fu = MEM;
									  vi_en = !i_cip[9]; //write to Vi for 176
									  vj_en = i_cip[9];  //read from Vj for 177
									  vk_en = 1'b0;
								  end
      default:begin 
		           o_fu= 3'b0;
					  o_fu_delay = 4'b0;
					  vi_en = 1'b0;
					  vj_en = 1'b0;
					  vk_en = 1'b0;
				  end
   endcase
end

//Now let's generate 1-hot signals for the i,j,k register selects
always@*
begin
   case(i_cip[8:6])
           3'b000:vi_one_hot = 8'b00000001;
           3'b001:vi_one_hot = 8'b00000010;
           3'b010:vi_one_hot = 8'b00000100;
           3'b011:vi_one_hot = 8'b00001000;
           3'b100:vi_one_hot = 8'b00010000;
           3'b101:vi_one_hot = 8'b00100000;
           3'b110:vi_one_hot = 8'b01000000;
           3'b111:vi_one_hot = 8'b10000000;
        endcase
end

always@*
begin
   case(i_cip[5:3])
           3'b000:vj_one_hot = 8'b00000001;
           3'b001:vj_one_hot = 8'b00000010;
           3'b010:vj_one_hot = 8'b00000100;
           3'b011:vj_one_hot = 8'b00001000;
           3'b100:vj_one_hot = 8'b00010000;
           3'b101:vj_one_hot = 8'b00100000;
           3'b110:vj_one_hot = 8'b01000000;
           3'b111:vj_one_hot = 8'b10000000;
        endcase
end

always@*
begin
   case(i_cip[2:0])
           3'b000:vk_one_hot = 8'b00000001;
           3'b001:vk_one_hot = 8'b00000010;
           3'b010:vk_one_hot = 8'b00000100;
           3'b011:vk_one_hot = 8'b00001000;
           3'b100:vk_one_hot = 8'b00010000;
           3'b101:vk_one_hot = 8'b00100000;
           3'b110:vk_one_hot = 8'b01000000;
           3'b111:vk_one_hot = 8'b10000000;
        endcase
end

always@*
begin
   case(o_fu)
	   VLOG:  vfu_one_hot = 8'b00000001;   //vector logical
      VSHIFT:vfu_one_hot = 8'b00000010;   //vector shift
		  VADD:vfu_one_hot = 8'b00000100;   //vector adder
	   FP_MUL:vfu_one_hot = 8'b00001000;   //FP multiply
	   FP_ADD:vfu_one_hot = 8'b00010000;   //FP adder 
	    FP_RA:vfu_one_hot = 8'b00100000;   //FP recip. approx.
	     VPOP:vfu_one_hot = 8'b01000000;   //vector pop count / parity
	      MEM:vfu_one_hot = 8'b10000000;   //Memory
	endcase
end
endmodule

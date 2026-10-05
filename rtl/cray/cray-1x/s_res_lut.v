////////////////////////////////////////////////////////////////////////
//        Cray S-Register Scheduler Look-up Table  //
//        Author: Christopher Fenton                     //
//        Date:  1/20/14                                       //
////////////////////////////////////////////////////////////////////////
//
//This block contains the look-up tables used to figure out how
//many cycles until the result will be available, and which
//functional unit the result is available from.

module s_res_lut (
	i_cip,
	o_delay,
	o_src,
	o_s_dest_en
);
	input wire [15:0] i_cip;
	output reg [3:0] o_delay;
	output reg [4:0] o_src;
	output wire o_s_dest_en;

	`include "cray_types.vh"


	assign o_s_dest_en = (i_cip[15:14]==2'b01) && 
					!(i_cip[15:9]==7'o075) &&
						  !(i_cip[15:9]==7'o077) && 
						  !((i_cip[15:9]==7'o073) && ((i_cip[5:0]==6'o02) || (i_cip[2:0]==3'o3))) ||
						   (i_cip[15:12]==4'b1010);

	always @* begin
		case (i_cip[15:9])
			7'o040:  o_delay = 4'd1;
			7'o041:  o_delay = 4'd1;
			7'o042:  o_delay = 4'd1;
			7'o043:  o_delay = 4'd1;
			7'o044:  o_delay = 4'd1;
			7'o045:  o_delay = 4'd1;
			7'o046:  o_delay = 4'd1;
			7'o047:  o_delay = 4'd1;
			7'o050:  o_delay = 4'd1;
			7'o051:  o_delay = 4'd1;
			7'o052:  o_delay = 4'd2;
			7'o053:  o_delay = 4'd2;
			7'o054:  o_delay = 4'd2;
			7'o055:  o_delay = 4'd2;
			7'o056:  o_delay = 4'd3;
			7'o057:  o_delay = 4'd3;
			7'o060:  o_delay = 4'd3;
			7'o061:  o_delay = 4'd3;
			7'o062:  o_delay = 4'd6;
			7'o063:  o_delay = 4'd6;
			7'o064:  o_delay = 4'd7;
			7'o065:  o_delay = 4'd7;
			7'o066:  o_delay = 4'd7;
			7'o067:  o_delay = 4'd7;
			7'o070:  o_delay = 4'd14;
			7'o071:  o_delay = 4'd2;
			7'o072:  o_delay = 4'd2;  //InterCPU communications (2-cycle delay, I believe)
			7'o073:  o_delay = 4'd1;
			7'o074:  o_delay = 4'd1;
			7'o076:  o_delay = 4'd2;
			7'o120:  o_delay = 4'd2;
			7'o121:  o_delay = 4'd2;
			7'o122:  o_delay = 4'd2;
			7'o123:  o_delay = 4'd2;
			7'o124:  o_delay = 4'd2;
			7'o125:  o_delay = 4'd2;
			7'o126:  o_delay = 4'd2;
			7'o127:  o_delay = 4'd2;
			default: o_delay = 4'b0;
		endcase
	end



	always @* begin
		case (i_cip[15:9])
			7'o040:  o_src = SBUS_IMM;
			7'o041:  o_src = SBUS_COMP_IMM;
			7'o042:  o_src = SBUS_S_LOG;
			7'o043:  o_src = SBUS_S_LOG;
			7'o044:  o_src = SBUS_S_LOG;
			7'o045:  o_src = SBUS_S_LOG;
			7'o046:  o_src = SBUS_S_LOG;
			7'o047:  o_src = SBUS_S_LOG;
			7'o050:  o_src = SBUS_S_LOG;
			7'o051:  o_src = SBUS_S_LOG;
			7'o052:  o_src = SBUS_S_SHIFT;
			7'o053:  o_src = SBUS_S_SHIFT;
			7'o054:  o_src = SBUS_S_SHIFT;
			7'o055:  o_src = SBUS_S_SHIFT;
			7'o056:  o_src = SBUS_S_SHIFT;
			7'o057:  o_src = SBUS_S_SHIFT;
			7'o060:  o_src = SBUS_S_ADD;
			7'o061:  o_src = SBUS_S_ADD;
			7'o062:  o_src = SBUS_FP_ADD;
			7'o063:  o_src = SBUS_FP_ADD;
			7'o064:  o_src = SBUS_FP_MULT;
			7'o065:  o_src = SBUS_FP_MULT;
			7'o066:  o_src = SBUS_FP_MULT;
			7'o067:  o_src = SBUS_FP_MULT;
			7'o070:  o_src = SBUS_FP_RA;
			7'o071:  o_src = SBUS_CONST_GEN;
			7'o072:  o_src = SBUS_INTERCPU;
			7'o073: begin
				case (i_cip[5:0])
					6'o00:   o_src = SBUS_V_MASK;
					6'o01:   o_src = SBUS_HI_SR;
					//FIXME - ADD support for semaphore registers and other misc.
					default: o_src = SBUS_V_MASK;
				endcase
			end
			7'o074:  o_src = SBUS_T_BUS;
			7'o076: begin
				case (i_cip[5:3])
					3'o0: o_src = SBUS_V0;
					3'o1: o_src = SBUS_V1;
					3'o2: o_src = SBUS_V2;
					3'o3: o_src = SBUS_V3;
					3'o4: o_src = SBUS_V4;
					3'o5: o_src = SBUS_V5;
					3'o6: o_src = SBUS_V6;
					3'o7: o_src = SBUS_V7;
				endcase
			end
			7'o120:  o_src = SBUS_MEM;
			7'o121:  o_src = SBUS_MEM;
			7'o122:  o_src = SBUS_MEM;
			7'o123:  o_src = SBUS_MEM;
			7'o124:  o_src = SBUS_MEM;
			7'o125:  o_src = SBUS_MEM;
			7'o126:  o_src = SBUS_MEM;
			7'o127:  o_src = SBUS_MEM;
			default: o_src = SBUS_NONE;
		endcase
	end
endmodule

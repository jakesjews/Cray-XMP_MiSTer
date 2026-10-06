////////////////////////////////////////////////////////////////////////
//        Cray A-Register Scheduler Look-up Table  //
//        Author: Christopher Fenton                     //
//        Date:  1/20/14                                       //
////////////////////////////////////////////////////////////////////////
//
//This block contains the look-up tables used to figure out how
//many cycles until the result will be available, and which
//functional unit the result is available from.
module a_res_lut (
	i_cip,
	o_delay,
	o_src,
	o_a_dest_en
);
	input wire [15:0] i_cip;
	output reg [3:0] o_delay;
	output reg [3:0] o_src;
	output wire o_a_dest_en;

	`include "cray_types.vh"


	//Check if we're writing something to an A register
	//This should occur for instructions: 020-027 (except for 025 and 027??7, 030-033, and 100-107
	//
	assign o_a_dest_en = ((i_cip[15:12]==4'b0010) && !((i_cip[11:9]==3'b101) || ((i_cip[11:9]==3'b111) && (i_cip[2:0]==3'b111)))) //020-027, except 025 and 027??7
		|| (i_cip[15:11] == 5'b00110) || (i_cip[15:12] == 4'b1000);  //030-033, 100-107


	always @* begin
		case (i_cip[15:9])
			7'o020:  o_delay = 4'd1;
			7'o021:  o_delay = 4'd1;
			7'o022:  o_delay = 4'd1;
			7'o023:  o_delay = 4'd1;
			7'o024:  o_delay = 4'd1;
			7'o026:  o_delay = (i_cip[2:0] == 3'd7) ? 4'd1 : 4'd4;  //k = 7: an X-MP shared register
			7'o027:  o_delay = 4'd3;
			7'o030:  o_delay = 4'd2;
			7'o031:  o_delay = 4'd2;
			7'o032:  o_delay = 4'd6;
			7'o033:  o_delay = 4'd4;
			7'o100:  o_delay = 4'd2;
			7'o101:  o_delay = 4'd2;
			7'o102:  o_delay = 4'd2;
			7'o103:  o_delay = 4'd2;
			7'o104:  o_delay = 4'd2;
			7'o105:  o_delay = 4'd2;
			7'o106:  o_delay = 4'd2;
			7'o107:  o_delay = 4'd2;
			default: o_delay = 4'd0;
		endcase
	end

	always @* begin
		case (i_cip[15:9])
			7'o020:  o_src = ABUS_IMM;
			7'o021:  o_src = ABUS_COMP_IMM;
			7'o022:  o_src = ABUS_SIMM;
			7'o023:  o_src = ABUS_S_BUS;
			7'o024:  o_src = ABUS_B_BUS;
			7'o026:  o_src = (i_cip[2:0] == 3'd7) ? ABUS_INTERCPU : ABUS_S_POP;
			7'o027:  o_src = ABUS_S_LZ;
			7'o030:  o_src = ABUS_A_ADD;
			7'o031:  o_src = ABUS_A_ADD;
			7'o032:  o_src = ABUS_A_MULT;
			7'o033:  o_src = ABUS_CHANNEL;
			7'o100:  o_src = ABUS_MEM;
			7'o101:  o_src = ABUS_MEM;
			7'o102:  o_src = ABUS_MEM;
			7'o103:  o_src = ABUS_MEM;
			7'o104:  o_src = ABUS_MEM;
			7'o105:  o_src = ABUS_MEM;
			7'o106:  o_src = ABUS_MEM;
			7'o107:  o_src = ABUS_MEM;
			default: o_src = ABUS_NONE;
		endcase
	end
endmodule

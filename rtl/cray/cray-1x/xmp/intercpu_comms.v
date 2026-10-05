module intercpu_comms(clk,
                      reset,
                      //CPU0
                      i_cln_0,
                      i_intercpu_instr_0,
							 i_intercpu_mon_mode_0,
							 i_intercpu_instr_vld_0,
							 i_intercpu_sj_0,
							 i_intercpu_si_0,
							 i_intercpu_ai_0,
							 o_intercpu_si_0,
							 o_intercpu_ai_0,
							 o_intercpu_issue_0,
							 //CPU1
							 i_cln_1,
                      i_intercpu_instr_1,
							 i_intercpu_mon_mode_1,
							 i_intercpu_instr_vld_1,
							 i_intercpu_sj_1,
							 i_intercpu_si_1,
							 i_intercpu_ai_1,
							 o_intercpu_si_1,
							 o_intercpu_ai_1,
							 o_intercpu_issue_1,
							 //CPU2
							 i_cln_2,
                      i_intercpu_instr_2,
							 i_intercpu_mon_mode_2,
							 i_intercpu_instr_vld_2,
							 i_intercpu_sj_2,
							 i_intercpu_si_2,
							 i_intercpu_ai_2,
							 o_intercpu_si_2,
							 o_intercpu_ai_2,
							 o_intercpu_issue_2,
							 //CPU3
							 i_cln_3,
                      i_intercpu_instr_3,
							 i_intercpu_mon_mode_3,
							 i_intercpu_instr_vld_3,
							 i_intercpu_sj_3,
							 i_intercpu_si_3,
							 i_intercpu_ai_3,
							 o_intercpu_si_3,
							 o_intercpu_ai_3,
							 o_intercpu_issue_3);

//System signals
input  wire        clk;
input  wire        reset;

//Inter-cpu signals
input  wire [2:0]  i_cln_0;
input  wire [15:0] i_intercpu_instr_0;
input  wire        i_intercpu_mon_mode_0;
input  wire        i_intercpu_instr_vld_0;
input  wire [63:0] i_intercpu_sj_0;
input  wire [63:0] i_intercpu_si_0;
input  wire [23:0] i_intercpu_ai_0;
output reg  [63:0] o_intercpu_si_0;
output reg  [23:0] o_intercpu_ai_0;
output wire        o_intercpu_issue_0;

input  wire [2:0]  i_cln_1;
input  wire [15:0] i_intercpu_instr_1;
input  wire        i_intercpu_mon_mode_1;
input  wire        i_intercpu_instr_vld_1;
input  wire [63:0] i_intercpu_sj_1;
input  wire [63:0] i_intercpu_si_1;
input  wire [23:0] i_intercpu_ai_1;
output reg  [63:0] o_intercpu_si_1;
output reg  [23:0] o_intercpu_ai_1;
output wire        o_intercpu_issue_1;

input  wire [2:0]  i_cln_2;
input  wire [15:0] i_intercpu_instr_2;
input  wire        i_intercpu_mon_mode_2;
input  wire        i_intercpu_instr_vld_2;
input  wire [63:0] i_intercpu_sj_2;
input  wire [63:0] i_intercpu_si_2;
input  wire [23:0] i_intercpu_ai_2;
output reg  [63:0] o_intercpu_si_2;
output reg  [23:0] o_intercpu_ai_2;
output wire        o_intercpu_issue_2;

input  wire [2:0]  i_cln_3;
input  wire [15:0] i_intercpu_instr_3;
input  wire        i_intercpu_mon_mode_3;
input  wire        i_intercpu_instr_vld_3;
input  wire [63:0] i_intercpu_sj_3;
input  wire [63:0] i_intercpu_si_3;
input  wire [23:0] i_intercpu_ai_3;
output reg  [63:0] o_intercpu_si_3;
output reg  [23:0] o_intercpu_ai_3;
output wire        o_intercpu_issue_3;

//64-bit cycle-counter
reg [63:0] rtc;

//Shared "SB" address registers
reg [24*8-1:0] sb_r_1;
reg [24*8-1:0] sb_r_2;
reg [24*8-1:0] sb_r_3;
reg [24*8-1:0] sb_r_4;
reg [24*8-1:0] sb_r_5;

//Shared "ST" scalar registers
reg [64*8-1:0] st_r_1;
reg [64*8-1:0] st_r_2;
reg [64*8-1:0] st_r_3;
reg [64*8-1:0] st_r_4;
reg [64*8-1:0] st_r_5;

//Shared "SM" semaphore registers
reg [31:0] sm_r_1;
reg [31:0] sm_r_2;
reg [31:0] sm_r_3;
reg [31:0] sm_r_4;
reg [31:0] sm_r_5;

localparam TS_INSTR = 10'b0000011100;

reg  [2:0] cln_0, cln_1, cln_2, cln_3;   //Flopped version of cluster number regs
reg [15:0] intercpu_instr_0, intercpu_instr_1, intercpu_instr_2, intercpu_instr_3;
reg        intercpu_mon_mode_0, intercpu_mon_mode_1, intercpu_mon_mode_2, intercpu_mon_mode_3;
reg        intercpu_instr_vld_0, intercpu_instr_vld_1, intercpu_instr_vld_2, intercpu_instr_vld_3;
reg [63:0] intercpu_sj_0, intercpu_sj_1, intercpu_sj_2, intercpu_sj_3;
reg [63:0] intercpu_si_0, intercpu_si_1, intercpu_si_2, intercpu_si_3;
reg [23:0] intercpu_ai_0, intercpu_ai_1, intercpu_ai_2, intercpu_ai_3;
wire [2:0] j_0, j_1, j_2, j_3;
wire [5:0] jk_0, jk_1, jk_2, jk_3;

wire [63:0] nxt_st_0, nxt_st_1, nxt_st_2, nxt_st_3;
wire [23:0] nxt_sb_0, nxt_sb_1, nxt_sb_2, nxt_sb_3;
wire [63:0] nxt_sm_0, nxt_sm_1, nxt_sm_2, nxt_sm_3;

wire [3:0] intercpu_type;
wire [3:0] write_rtc, read_rtc;
wire [3:0] write_sb, read_sb;
wire [3:0] write_st, read_st;
wire [3:0] clear_sm, set_sm, read_sm, write_sm;

wire [3:0] allow_ts_req;
wire [3:0] incoming_ts_req;
wire [3:0] hold_ts_req;

//Let's flop all of our inputs to improve timing
always@(posedge clk)
	begin
		cln_0 <= i_cln_0;
		cln_1 <= i_cln_1;
		cln_2 <= i_cln_2;
		cln_3 <= i_cln_3;
		intercpu_instr_0 <= i_intercpu_instr_0;
		intercpu_instr_1 <= i_intercpu_instr_1;
		intercpu_instr_2 <= i_intercpu_instr_2;
		intercpu_instr_3 <= i_intercpu_instr_3;
		intercpu_mon_mode_0 <= i_intercpu_mon_mode_0;
		intercpu_mon_mode_1 <= i_intercpu_mon_mode_1;
		intercpu_mon_mode_2 <= i_intercpu_mon_mode_2;
		intercpu_mon_mode_3 <= i_intercpu_mon_mode_3;
		intercpu_instr_vld_0 <=i_intercpu_instr_vld_0;
		intercpu_instr_vld_1 <=i_intercpu_instr_vld_1;
		intercpu_instr_vld_2 <=i_intercpu_instr_vld_2;
		intercpu_instr_vld_3 <=i_intercpu_instr_vld_3;
		intercpu_sj_0 <= i_intercpu_sj_0;
		intercpu_sj_1 <= i_intercpu_sj_1;
		intercpu_sj_2 <= i_intercpu_sj_2;
		intercpu_sj_3 <= i_intercpu_sj_3;
		intercpu_si_0 <= i_intercpu_si_0;
		intercpu_si_1 <= i_intercpu_si_1;
		intercpu_si_2 <= i_intercpu_si_2;
		intercpu_si_3 <= i_intercpu_si_3;
		intercpu_ai_0 <= i_intercpu_ai_0;
		intercpu_ai_1 <= i_intercpu_ai_1;
		intercpu_ai_2 <= i_intercpu_ai_2;
		intercpu_ai_3 <= i_intercpu_ai_3;
	end

assign j_0 = intercpu_instr_0[5:3];
assign j_1 = intercpu_instr_1[5:3];
assign j_2 = intercpu_instr_2[5:3];
assign j_3 = intercpu_instr_3[5:3];

assign jk_0 = intercpu_instr_0[5:0];
assign jk_1 = intercpu_instr_1[5:0];
assign jk_2 = intercpu_instr_2[5:0];
assign jk_3 = intercpu_instr_3[5:0];

////////////////////////////////////////////////////
//         Inter-CPU Communications Block         //
////////////////////////////////////////////////////
/* This block handles the following instructions:

0014j0	RT	Sj		Enter the RTC with (Sj)
072i00	Si RT		Transmit RTC to (Si)

026ij7	Ai SBj	Transmit (SBj) to Ai
027ij7	SBj Ai	Transmit (Ai) to SBj
072ij3	Si STj	Transmit (STj) to Si
073ij3	STj Si	Transmit (Si) to STj

0034jk	SMjk 1,TS	Test and Set SMjk
0036jk	SMjk 0		Clear SMjk
0037jk	SMjk 1		Set SMjk

072i02	Si SM			Transmit (SM) to Si
073i02	SM Si			Transmit (Si) to SM

*/


//Let's do all of the instruction decoding first - everything except Test & Set is straightforward

assign write_rtc[0] = o_intercpu_issue_0 && (intercpu_instr_0[15:6]==10'b0000001100) && (intercpu_instr_0[2:0]==3'b0);
assign read_rtc[0]  = o_intercpu_issue_0 && (intercpu_instr_0[15:9]==7'b0111010) && (intercpu_instr_0[5:0]==6'b0);
assign write_sb[0]  = o_intercpu_issue_0 && (intercpu_instr_0[15:9]==7'b0010111) && (intercpu_instr_0[2:0]==3'b111);
assign read_sb[0]   =(intercpu_instr_0[15:9]==7'b0010110) && (intercpu_instr_0[2:0]==3'b111);
assign write_st[0]  = o_intercpu_issue_0 && (intercpu_instr_0[15:9]==7'b0111011) && (intercpu_instr_0[2:0]==3'b011);
assign read_st[0]   =(intercpu_instr_0[15:9]==7'b0111010) && (intercpu_instr_0[2:0]==3'b011);
assign clear_sm[0]  = o_intercpu_issue_0 && (intercpu_instr_0[15:6]==10'b0000011110);
assign set_sm[0]    = o_intercpu_issue_0 && (intercpu_instr_0[15:6]==10'b0000011111);
assign read_sm[0]   =(intercpu_instr_0[15:9]==7'b0111010) && (intercpu_instr_0[5:0]==6'b000010);
assign write_sm[0]  = o_intercpu_issue_0 && (intercpu_instr_0[15:9]==7'b0111011) && (intercpu_instr_0[5:0]==6'b000010);
								
assign intercpu_type[0] = ((intercpu_instr_0[15:6]==10'b0000001100) && (intercpu_instr_0[2:0]==3'b0)) ||
                          ((intercpu_instr_0[15:9]==7'b0111010) && (intercpu_instr_0[5:0]==6'b0))     ||
                          ((intercpu_instr_0[15:9]==7'b0010111) && (intercpu_instr_0[2:0]==3'b111))   ||
                          ((intercpu_instr_0[15:9]==7'b0010110) && (intercpu_instr_0[2:0]==3'b111))   ||
                          ((intercpu_instr_0[15:9]==7'b0111011) && (intercpu_instr_0[2:0]==3'b011))   ||
                          ((intercpu_instr_0[15:9]==7'b0111010) && (intercpu_instr_0[2:0]==3'b011))   ||
                          ((intercpu_instr_0[15:6]==10'b0000011110))                                  ||
                          ((intercpu_instr_0[15:6]==10'b0000011111))                                  ||
                          ((intercpu_instr_0[15:9]==7'b0111010) && (intercpu_instr_0[5:0]==6'b000010))||
                          ((intercpu_instr_0[15:9]==7'b0111011) && (intercpu_instr_0[5:0]==6'b000010));
								  
						

assign write_rtc[1] = o_intercpu_issue_1 && (intercpu_instr_1[15:6]==10'b0000001100) && (intercpu_instr_1[2:0]==3'b0);
assign read_rtc[1]  =(intercpu_instr_1[15:9]==7'b0111010) && (intercpu_instr_1[5:0]==6'b0);
assign write_sb[1]  = o_intercpu_issue_1 && (intercpu_instr_1[15:9]==7'b0010111) && (intercpu_instr_1[2:0]==3'b111);
assign read_sb[1]   =(intercpu_instr_1[15:9]==7'b0010110) && (intercpu_instr_1[2:0]==3'b111);
assign write_st[1]  = o_intercpu_issue_1 && (intercpu_instr_1[15:9]==7'b0111011) && (intercpu_instr_1[2:0]==3'b011);
assign read_st[1]   =(intercpu_instr_1[15:9]==7'b0111010) && (intercpu_instr_1[2:0]==3'b011);
assign clear_sm[1]  = o_intercpu_issue_1 && (intercpu_instr_1[15:6]==10'b0000011110);
assign set_sm[1]    = o_intercpu_issue_1 && (intercpu_instr_1[15:6]==10'b0000011111);
assign read_sm[1]   =(intercpu_instr_1[15:9]==7'b0111010) && (intercpu_instr_1[5:0]==6'b000010);
assign write_sm[1]  = o_intercpu_issue_1 && (intercpu_instr_1[15:9]==7'b0111011) && (intercpu_instr_1[5:0]==6'b000010);

assign intercpu_type[1] = ((intercpu_instr_1[15:6]==10'b0000001100) && (intercpu_instr_1[2:0]==3'b0)) ||
                          ((intercpu_instr_1[15:9]==7'b0111010) && (intercpu_instr_1[5:0]==6'b0))     ||
                          ((intercpu_instr_1[15:9]==7'b0010111) && (intercpu_instr_1[2:0]==3'b111))   ||
                          ((intercpu_instr_1[15:9]==7'b0010110) && (intercpu_instr_1[2:0]==3'b111))   ||
                          ((intercpu_instr_1[15:9]==7'b0111011) && (intercpu_instr_1[2:0]==3'b011))   ||
                          ((intercpu_instr_1[15:9]==7'b0111010) && (intercpu_instr_1[2:0]==3'b011))   ||
                          ((intercpu_instr_1[15:6]==10'b0000011110))                                  ||
                          ((intercpu_instr_1[15:6]==10'b0000011111))                                  ||
                          ((intercpu_instr_1[15:9]==7'b0111010) && (intercpu_instr_1[5:0]==6'b000010))||
                          ((intercpu_instr_1[15:9]==7'b0111011) && (intercpu_instr_1[5:0]==6'b000010));								 
								 
								 
assign write_rtc[2] = o_intercpu_issue_2 && (intercpu_instr_2[15:6]==10'b0000001100) && (intercpu_instr_2[2:0]==3'b0);
assign read_rtc[2]  =(intercpu_instr_2[15:9]==7'b0111010) && (intercpu_instr_2[5:0]==6'b0);
assign write_sb[2]  = o_intercpu_issue_2 && (intercpu_instr_2[15:9]==7'b0010111) && (intercpu_instr_2[2:0]==3'b111);
assign read_sb[2]   =(intercpu_instr_2[15:9]==7'b0010110) && (intercpu_instr_2[2:0]==3'b111);
assign write_st[2]  = o_intercpu_issue_2 && (intercpu_instr_2[15:9]==7'b0111011) && (intercpu_instr_2[2:0]==3'b011);
assign read_st[2]   =(intercpu_instr_2[15:9]==7'b0111010) && (intercpu_instr_2[2:0]==3'b011);
assign clear_sm[2]  = o_intercpu_issue_2 && (intercpu_instr_2[15:6]==10'b0000011110);
assign set_sm[2]    = o_intercpu_issue_2 && (intercpu_instr_2[15:6]==10'b0000011111);
assign read_sm[2]   =(intercpu_instr_2[15:9]==7'b0111010) && (intercpu_instr_2[5:0]==6'b000010);
assign write_sm[2]  = o_intercpu_issue_2 && (intercpu_instr_2[15:9]==7'b0111011) && (intercpu_instr_2[5:0]==6'b000010);

assign intercpu_type[2] = ((intercpu_instr_2[15:6]==10'b0000001100) && (intercpu_instr_2[2:0]==3'b0)) ||
                          ((intercpu_instr_2[15:9]==7'b0111010) && (intercpu_instr_2[5:0]==6'b0))     ||
                          ((intercpu_instr_2[15:9]==7'b0010111) && (intercpu_instr_2[2:0]==3'b111))   ||
                          ((intercpu_instr_2[15:9]==7'b0010110) && (intercpu_instr_2[2:0]==3'b111))   ||
                          ((intercpu_instr_2[15:9]==7'b0111011) && (intercpu_instr_2[2:0]==3'b011))   ||
                          ((intercpu_instr_2[15:9]==7'b0111010) && (intercpu_instr_2[2:0]==3'b011))   ||
                          ((intercpu_instr_2[15:6]==10'b0000011110))                                  ||
                          ((intercpu_instr_2[15:6]==10'b0000011111))                                  ||
                          ((intercpu_instr_2[15:9]==7'b0111010) && (intercpu_instr_2[5:0]==6'b000010))||
                          ((intercpu_instr_2[15:9]==7'b0111011) && (intercpu_instr_2[5:0]==6'b000010));								 
								 
								 
assign write_rtc[3] = o_intercpu_issue_3 && (intercpu_instr_3[15:6]==10'b0000001100) && (intercpu_instr_3[2:0]==3'b0);
assign read_rtc[3]  =(intercpu_instr_3[15:9]==7'b0111010) && (intercpu_instr_3[5:0]==6'b0);
assign write_sb[3]  = o_intercpu_issue_3 && (intercpu_instr_3[15:9]==7'b0010111) && (intercpu_instr_3[2:0]==3'b111);
assign read_sb[3]   =(intercpu_instr_3[15:9]==7'b0010110) && (intercpu_instr_3[2:0]==3'b111);
assign write_st[3]  = o_intercpu_issue_3 && (intercpu_instr_3[15:9]==7'b0111011) && (intercpu_instr_3[2:0]==3'b011);
assign read_st[3]   =(intercpu_instr_3[15:9]==7'b0111010) && (intercpu_instr_3[2:0]==3'b011);
assign clear_sm[3]  = o_intercpu_issue_3 && (intercpu_instr_3[15:6]==10'b0000011110);
assign set_sm[3]    = o_intercpu_issue_3 && (intercpu_instr_3[15:6]==10'b0000011111);
assign read_sm[3]   =(intercpu_instr_3[15:9]==7'b0111010) && (intercpu_instr_3[5:0]==6'b000010);
assign write_sm[3]  = o_intercpu_issue_3 && (intercpu_instr_3[15:9]==7'b0111011) && (intercpu_instr_3[5:0]==6'b000010);

assign intercpu_type[3] = ((intercpu_instr_3[15:6]==10'b0000001100) && (intercpu_instr_3[2:0]==3'b0)) ||
                          ((intercpu_instr_3[15:9]==7'b0111010) && (intercpu_instr_3[5:0]==6'b0))     ||
                          ((intercpu_instr_3[15:9]==7'b0010111) && (intercpu_instr_3[2:0]==3'b111))   ||
                          ((intercpu_instr_3[15:9]==7'b0010110) && (intercpu_instr_3[2:0]==3'b111))   ||
                          ((intercpu_instr_3[15:9]==7'b0111011) && (intercpu_instr_3[2:0]==3'b011))   ||
                          ((intercpu_instr_3[15:9]==7'b0111010) && (intercpu_instr_3[2:0]==3'b011))   ||
                          ((intercpu_instr_3[15:6]==10'b0000011110))                                  ||
                          ((intercpu_instr_3[15:6]==10'b0000011111))                                  ||
                          ((intercpu_instr_3[15:9]==7'b0111010) && (intercpu_instr_3[5:0]==6'b000010))||
                          ((intercpu_instr_3[15:9]==7'b0111011) && (intercpu_instr_3[5:0]==6'b000010));
								 
//Test & Set is a sort-of complicated instruction, so we need to pay some special
//attention to how we decode it, and check if we need to stall.
//The only instruction we're going to hold 'instr_issue' for is the Test & Set
//instruction, where, if it's already set, we just stall the requesting CPU.
//We also need to detect simultaneous accesses. We're just going to add a simple
//fixed-priority arbiter that only lets one T&S instruction issue at a time

//Check which CPUs are doing a T&S request
assign incoming_ts_req[0]= (intercpu_instr_vld_0 && (intercpu_instr_0[15:6]==TS_INSTR));
assign incoming_ts_req[1]= (intercpu_instr_vld_1 && (intercpu_instr_1[15:6]==TS_INSTR));
assign incoming_ts_req[2]= (intercpu_instr_vld_2 && (intercpu_instr_2[15:6]==TS_INSTR));
assign incoming_ts_req[3]= (intercpu_instr_vld_3 && (intercpu_instr_3[15:6]==TS_INSTR));

//Check if that CPU is stalled regardless of other requests
assign hold_ts_req[0] = ((cln_0==3'h1) && sm_r_1[intercpu_instr_0[4:0]]) ||
                        ((cln_0==3'h2) && sm_r_2[intercpu_instr_0[4:0]]) ||
								((cln_0==3'h3) && sm_r_3[intercpu_instr_0[4:0]]) ||
								((cln_0==3'h4) && sm_r_4[intercpu_instr_0[4:0]]) ||
								((cln_0==3'h5) && sm_r_5[intercpu_instr_0[4:0]]);
								
assign hold_ts_req[1] = ((cln_1==3'h1) && sm_r_1[intercpu_instr_1[4:0]]) ||
                        ((cln_1==3'h2) && sm_r_2[intercpu_instr_1[4:0]]) ||
								((cln_1==3'h3) && sm_r_3[intercpu_instr_1[4:0]]) ||
								((cln_1==3'h4) && sm_r_4[intercpu_instr_1[4:0]]) ||
								((cln_1==3'h5) && sm_r_5[intercpu_instr_1[4:0]]);
								
assign hold_ts_req[2] = ((cln_2==3'h1) && sm_r_1[intercpu_instr_2[4:0]]) ||
                        ((cln_2==3'h2) && sm_r_2[intercpu_instr_2[4:0]]) ||
								((cln_2==3'h3) && sm_r_3[intercpu_instr_2[4:0]]) ||
								((cln_2==3'h4) && sm_r_4[intercpu_instr_2[4:0]]) ||
								((cln_2==3'h5) && sm_r_5[intercpu_instr_2[4:0]]);
								
assign hold_ts_req[3] = ((cln_3==3'h1) && sm_r_1[intercpu_instr_3[4:0]]) ||
                        ((cln_3==3'h2) && sm_r_2[intercpu_instr_3[4:0]]) ||
								((cln_3==3'h3) && sm_r_3[intercpu_instr_3[4:0]]) ||
								((cln_3==3'h4) && sm_r_4[intercpu_instr_3[4:0]]) ||
								((cln_3==3'h5) && sm_r_5[intercpu_instr_3[4:0]]);
  
//Now determine if that CPU is allowed to issue its T&S request
// - We use strict-priority to issue them (CPU0 > CPU1 > CPU2 > CPU3)
assign allow_ts_req[0] = incoming_ts_req[0] && !hold_ts_req[0];
assign allow_ts_req[1] = (incoming_ts_req[1] && !hold_ts_req[1]) &&
                         !(allow_ts_req[0]);
assign allow_ts_req[2] = (incoming_ts_req[2] && !hold_ts_req[2]) &&
                         !(|allow_ts_req[1:0]);
assign allow_ts_req[3] = (incoming_ts_req[3] && !hold_ts_req[3]) &&
                         !(|allow_ts_req[2:0]);

//Now decide if we can issue the incoming instruction - if it's a T&S,
//make sure it is kosher, otherwise just let it through (we'll just ignore
//lower CPUs with a fixed priority if there's a write-conflict, as that's
//supposed to be handled in software anyway)

assign o_intercpu_issue_0 = intercpu_type[0] && intercpu_instr_vld_0 && 
                               ((incoming_ts_req[0] && allow_ts_req[0]) ||
                                (!incoming_ts_req[0]));

assign o_intercpu_issue_1 = intercpu_type[1] && intercpu_instr_vld_1 && 
                               ((incoming_ts_req[1] && allow_ts_req[1]) ||
                                (!incoming_ts_req[1]));

assign o_intercpu_issue_2 = intercpu_type[2] && intercpu_instr_vld_2 && 
                               ((incoming_ts_req[2] && allow_ts_req[2]) ||
                                (!incoming_ts_req[2]));

assign o_intercpu_issue_3 = intercpu_type[2] && intercpu_instr_vld_3 &&
                               ((incoming_ts_req[3] && allow_ts_req[3]) ||
                                (!incoming_ts_req[3]));

//T&S is taken care of now, but we still need to do all of the read/write instructions

//First, let's take care of all of the read instructions

//mux structures to read the ST registers

intercpu_st_mux stmux0(.i_cln(cln_0), 
                       .i_j(j_0), 
							  .i_st_1(st_r_1),
							  .i_st_2(st_r_2),
							  .i_st_3(st_r_3), 
							  .i_st_4(st_r_4),
							  .i_st_5(st_r_5),
							  .o_si(nxt_st_0));

intercpu_st_mux stmux1(.i_cln(cln_1), 
                       .i_j(j_1), 
							  .i_st_1(st_r_1),
							  .i_st_2(st_r_2),
							  .i_st_3(st_r_3), 
							  .i_st_4(st_r_4),
							  .i_st_5(st_r_5),
							  .o_si(nxt_st_1));

intercpu_st_mux stmux2(.i_cln(cln_2), 
                       .i_j(j_2), 
							  .i_st_1(st_r_1),
							  .i_st_2(st_r_2),
							  .i_st_3(st_r_3), 
							  .i_st_4(st_r_4),
							  .i_st_5(st_r_5),
							  .o_si(nxt_st_2));

intercpu_st_mux stmux3(.i_cln(cln_3), 
                       .i_j(j_3), 
							  .i_st_1(st_r_1),
							  .i_st_2(st_r_2),
							  .i_st_3(st_r_3), 
							  .i_st_4(st_r_4),
							  .i_st_5(st_r_5),
							  .o_si(nxt_st_3));
							  
//Mux structures to read semaphore registers
intercpu_sm_mux smmux0(.i_cln(cln_0), 
							  .i_sm_1(sm_r_1),
							  .i_sm_2(sm_r_2),
							  .i_sm_3(sm_r_3), 
							  .i_sm_4(sm_r_4),
							  .i_sm_5(sm_r_5),
							  .o_si(nxt_sm_0));

intercpu_sm_mux smmux1(.i_cln(cln_1), 
							  .i_sm_1(sm_r_1),
							  .i_sm_2(sm_r_2),
							  .i_sm_3(sm_r_3), 
							  .i_sm_4(sm_r_4),
							  .i_sm_5(sm_r_5),
							  .o_si(nxt_sm_1));

intercpu_sm_mux smmux2(.i_cln(cln_2), 
							  .i_sm_1(sm_r_1),
							  .i_sm_2(sm_r_2),
							  .i_sm_3(sm_r_3), 
							  .i_sm_4(sm_r_4),
							  .i_sm_5(sm_r_5),
							  .o_si(nxt_sm_2));

intercpu_sm_mux smmux3(.i_cln(cln_3), 
							  .i_sm_1(sm_r_1),
							  .i_sm_2(sm_r_2),
							  .i_sm_3(sm_r_3), 
							  .i_sm_4(sm_r_4),
							  .i_sm_5(sm_r_5),
							  .o_si(nxt_sm_3));							  
//Choose which 64-bit register to send back to the CPUs for read operations
always@(posedge clk)
   begin
      o_intercpu_si_0 <= read_rtc[0] ? rtc[63:0] : 
		                   read_st[0]  ? nxt_st_0  :
								 read_sm[0]  ? nxt_sm_0  :
								 64'b0;
		o_intercpu_si_1 <= read_rtc[1] ? rtc[63:0] :
		                   read_st[1]  ? nxt_st_1  :
								 read_sm[1]  ? nxt_sm_1  :
								 64'b0;
		o_intercpu_si_2 <= read_rtc[2] ? rtc[63:0] :
		                   read_st[2]  ? nxt_st_2  :
								 read_sm[2]  ? nxt_sm_2  :
								 64'b0;
		o_intercpu_si_3 <= read_rtc[3] ? rtc[63:0] :
		                   read_st[3]  ? nxt_st_3  :
								 read_sm[3]  ? nxt_sm_3  :
								 64'b0;
	end


//Mux structures to read sb registers
intercpu_sb_mux sbmux0(.i_cln(cln_0), 
                       .i_j(j_0), 
							  .i_sb_1(sb_r_1),
							  .i_sb_2(sb_r_2),
							  .i_sb_3(sb_r_3), 
							  .i_sb_4(sb_r_4),
							  .i_sb_5(sb_r_5),
							  .o_ai(nxt_sb_0));

intercpu_sb_mux sbmux1(.i_cln(cln_1), 
                       .i_j(j_1), 
							  .i_sb_1(sb_r_1),
							  .i_sb_2(sb_r_2),
							  .i_sb_3(sb_r_3), 
							  .i_sb_4(sb_r_4),
							  .i_sb_5(sb_r_5),
							  .o_ai(nxt_sb_1));

intercpu_sb_mux sbmux2(.i_cln(cln_2), 
                       .i_j(j_2), 
							  .i_sb_1(sb_r_1),
							  .i_sb_2(sb_r_2),
							  .i_sb_3(sb_r_3), 
							  .i_sb_4(sb_r_4),
							  .i_sb_5(sb_r_5),
							  .o_ai(nxt_sb_2));

intercpu_sb_mux sbmux3(.i_cln(cln_3), 
                       .i_j(j_3), 
							  .i_sb_1(sb_r_1),
							  .i_sb_2(sb_r_2),
							  .i_sb_3(sb_r_3), 
							  .i_sb_4(sb_r_4),
							  .i_sb_5(sb_r_5),
							  .o_ai(nxt_sb_3));

//Choose which 24-bit register to send back to the CPUs for read operations
always@(posedge clk)
   begin
      o_intercpu_ai_0 <= read_sb[0] ? nxt_sb_0 : 24'b0;         
		o_intercpu_ai_1 <= read_sb[1] ? nxt_sb_1 : 24'b0;
		o_intercpu_ai_2 <= read_sb[2] ? nxt_sb_2 : 24'b0;
		o_intercpu_ai_3 <= read_sb[3] ? nxt_sb_3 : 24'b0;
	end

//That should take care of reading operations - now we need to handle
//writing operations. This has the added caveat that we need to detect
//simultaneous write conflicts (although I think it's okay to just use
//strict priority to resolve them, as the manual says it's software's job
//to make sure processors don't step on one anothers toes

//The real-time clock (cycle counter)
always@(posedge clk)
   if(reset)
	   rtc[63:0] <= 64'b0;
	else if(write_rtc[0])
		rtc[63:0] <= intercpu_sj_0;
	else if(write_rtc[1])
		rtc[63:0] <= intercpu_sj_1;
	else if(write_rtc[2])
		rtc[63:0] <= intercpu_sj_2;
	else if(write_rtc[3])
		rtc[63:0] <= intercpu_sj_3;
	else
		rtc[63:0] <= rtc[63:0] + 64'b1;
	

//Now, the shared SB and ST registers - this is somewhat messy, as it actually
//encompasses 40 registers (8-regs x 5 clusters) for both ST and SB

//Cluster 1
genvar i;
generate
	for (i=0; i < 8; i=i+1) begin : SHARED_SB_ST
		//Cluster 1
		always@(posedge clk)
			if(reset)
					sb_r_1[i*24+23:i*24] <= 24'b0;
			else if(write_sb[0] && (cln_0==3'h1) && (j_0==i))
					sb_r_1[i*24+23:i*24] <= intercpu_ai_0;
			else if(write_sb[1] && (cln_1==3'h1) && (j_1==i))
					sb_r_1[i*24+23:i*24] <= intercpu_ai_1;
			else if(write_sb[2] && (cln_2==3'h1) && (j_2==i))
					sb_r_1[i*24+23:i*24] <= intercpu_ai_2;
			else if(write_sb[3] && (cln_3==3'h1) && (j_3==i))
					sb_r_1[i*24+23:i*24] <= intercpu_ai_3;
			else
					sb_r_1[i*24+23:i*24] <= sb_r_1[i*24+23:i*24];
					
		always@(posedge clk)
			if(reset)
					st_r_1[i*64+63:i*64] <= 64'b0;
			else if(write_st[0] && (cln_0==3'h1) && (j_0==i))
					st_r_1[i*64+63:i*64] <= intercpu_si_0;
			else if(write_st[1] && (cln_1==3'h1) && (j_1==i))
					st_r_1[i*64+63:i*64] <= intercpu_si_1;
			else if(write_st[2] && (cln_2==3'h1) && (j_2==i))
					st_r_1[i*64+63:i*64] <= intercpu_si_2;
			else if(write_st[3] && (cln_3==3'h1) && (j_3==i))
					st_r_1[i*64+63:i*64] <= intercpu_si_3;
			else
					st_r_1[i*64+63:i*64] <= st_r_1[i*64+63:i*64];

		//Cluster 2
		always@(posedge clk)
			if(reset)
					sb_r_2[i*24+23:i*24] <= 24'b0;
			else if(write_sb[0] && (cln_0==3'h2) && (j_0==i))
					sb_r_2[i*24+23:i*24] <= intercpu_ai_0;
			else if(write_sb[1] && (cln_1==3'h2) && (j_1==i))
					sb_r_2[i*24+23:i*24] <= intercpu_ai_1;
			else if(write_sb[2] && (cln_2==3'h2) && (j_2==i))
					sb_r_2[i*24+23:i*24] <= intercpu_ai_2;
			else if(write_sb[3] && (cln_3==3'h2) && (j_3==i))
					sb_r_2[i*24+23:i*24] <= intercpu_ai_3;
			else
					sb_r_2[i*24+23:i*24] <= sb_r_2[i*24+23:i*24];

		always@(posedge clk)
			if(reset)
					st_r_2[i*64+63:i*64] <= 64'b0;
			else if(write_st[0] && (cln_0==3'h2) && (j_0==i))
					st_r_2[i*64+63:i*64] <= intercpu_si_0;
			else if(write_st[1] && (cln_1==3'h2) && (j_1==i))
					st_r_2[i*64+63:i*64] <= intercpu_si_1;
			else if(write_st[2] && (cln_2==3'h2) && (j_2==i))
					st_r_2[i*64+63:i*64] <= intercpu_si_2;
			else if(write_st[3] && (cln_3==3'h2) && (j_3==i))
					st_r_2[i*64+63:i*64] <= intercpu_si_3;
			else
					st_r_2[i*64+63:i*64] <= st_r_2[i*64+63:i*64];

		//Cluster 3
		always@(posedge clk)
			if(reset)
					sb_r_3[i*24+23:i*24] <= 24'b0;
			else if(write_sb[0] && (cln_0==3'h3) && (j_0==i))
					sb_r_3[i*24+23:i*24] <= intercpu_ai_0;
			else if(write_sb[1] && (cln_1==3'h3) && (j_1==i))
					sb_r_3[i*24+23:i*24] <= intercpu_ai_1;
			else if(write_sb[2] && (cln_2==3'h3) && (j_2==i))
					sb_r_3[i*24+23:i*24] <= intercpu_ai_2;
			else if(write_sb[3] && (cln_3==3'h3) && (j_3==i))
					sb_r_3[i*24+23:i*24] <= intercpu_ai_3;
			else
					sb_r_3[i*24+23:i*24] <= sb_r_3[i*24+23:i*24];

		always@(posedge clk)
			if(reset)
					st_r_3[i*64+63:i*64] <= 64'b0;
			else if(write_st[0] && (cln_0==3'h3) && (j_0==i))
					st_r_3[i*64+63:i*64] <= intercpu_si_0;
			else if(write_st[1] && (cln_1==3'h3) && (j_1==i))
					st_r_3[i*64+63:i*64] <= intercpu_si_1;
			else if(write_st[2] && (cln_2==3'h3) && (j_2==i))
					st_r_3[i*64+63:i*64] <= intercpu_si_2;
			else if(write_st[3] && (cln_3==3'h3) && (j_3==i))
					st_r_3[i*64+63:i*64] <= intercpu_si_3;
			else
					st_r_3[i*64+63:i*64] <= st_r_3[i*64+63:i*64];
					
					
		//Cluster 4
		always@(posedge clk)
			if(reset)
					sb_r_4[i*24+23:i*24] <= 24'b0;
			else if(write_sb[0] && (cln_0==3'h4) && (j_0==i))
					sb_r_4[i*24+23:i*24] <= intercpu_ai_0;
			else if(write_sb[1] && (cln_1==3'h4) && (j_1==i))
					sb_r_4[i*24+23:i*24] <= intercpu_ai_1;
			else if(write_sb[2] && (cln_2==3'h4) && (j_2==i))
					sb_r_4[i*24+23:i*24] <= intercpu_ai_2;
			else if(write_sb[3] && (cln_3==3'h4) && (j_3==i))
					sb_r_4[i*24+23:i*24] <= intercpu_ai_3;
			else
					sb_r_4[i*24+23:i*24] <= sb_r_4[i*24+23:i*24];

		always@(posedge clk)
			if(reset)
					st_r_4[i*64+63:i*64] <= 64'b0;
			else if(write_st[0] && (cln_0==3'h4) && (j_0==i))
					st_r_4[i*64+63:i*64] <= intercpu_si_0;
			else if(write_st[1] && (cln_1==3'h4) && (j_1==i))
					st_r_4[i*64+63:i*64] <= intercpu_si_1;
			else if(write_st[2] && (cln_2==3'h4) && (j_2==i))
					st_r_4[i*64+63:i*64] <= intercpu_si_2;
			else if(write_st[3] && (cln_3==3'h4) && (j_3==i))
					st_r_4[i*64+63:i*64] <= intercpu_si_3;
			else
					st_r_4[i*64+63:i*64] <= st_r_4[i*64+63:i*64];

		//Cluster 5
		always@(posedge clk)
			if(reset)
					sb_r_5[i*24+23:i*24] <= 24'b0;
			else if(write_sb[0] && (cln_0==3'h5) && (j_0==i))
					sb_r_5[i*24+23:i*24] <= intercpu_ai_0;
			else if(write_sb[1] && (cln_1==3'h5) && (j_1==i))
					sb_r_5[i*24+23:i*24] <= intercpu_ai_1;
			else if(write_sb[2] && (cln_2==3'h5) && (j_2==i))
					sb_r_5[i*24+23:i*24] <= intercpu_ai_2;
			else if(write_sb[3] && (cln_3==3'h5) && (j_3==i))
					sb_r_5[i*24+23:i*24] <= intercpu_ai_3;
			else
					sb_r_5[i*24+23:i*24] <= sb_r_5[i*24+23:i*24];

		always@(posedge clk)
			if(reset)
					st_r_5[i*64+63:i*64] <= 64'b0;
			else if(write_st[0] && (cln_0==3'h5) && (j_0==i))
					st_r_5[i*64+63:i*64] <= intercpu_si_0;
			else if(write_st[1] && (cln_1==3'h5) && (j_1==i))
					st_r_5[i*64+63:i*64] <= intercpu_si_1;
			else if(write_st[2] && (cln_2==3'h5) && (j_2==i))
					st_r_5[i*64+63:i*64] <= intercpu_si_2;
			else if(write_st[3] && (cln_3==3'h5) && (j_3==i))
					st_r_5[i*64+63:i*64] <= intercpu_si_3;
			else
					st_r_5[i*64+63:i*64] <= st_r_5[i*64+63:i*64];
		
	end
endgenerate


//Now we need to deal with the Semaphore registers.
//Each cluster has a 32-bit 'semaphore' register, and it can be read/written
//simultaneously by a 64-bit operation (I assume lower 32 bits get used and
//upper 32 bits get ignored for a write), or read/written bit-wise, all by 
//each CPU. Since the reference manual is a bit unclear, and there are all kinds
//of ways to shoot yourself in the foot with this mechanism, I will implement the
//following, *within* a given bit:
//1. Every instruction issues with fixed priority: CPU0 > CPU1 > CPU2 > CPU3
//2. Test & Set has priority over other instructions
//3. Then instructions that write the whole vector
//4. Then bitwise set operations
//5. Then bitwise clear operations
//
//If CPU0 tries to clear bit 13 of a register at the same time CPU 2 tries to set it,
//the bit will get set. 
//Also, if CPU0 issues a test&set on bit 0, while CPU1 writes all 0's to the same register, 
//everything will be 0'd except bit 0.
//
//Test & Sets to the same bit will still cause lesser-priority CPUs to stall, however.
genvar j;
generate
	for (j=0; j < 32; j=j+1) begin : SHARED_SM
		//Cluster 1
		always@(posedge clk)
			if(reset)
				sm_r_1[j] <= 1'b0;
			//Test & Set
			else if(allow_ts_req[0] && (cln_0==3'h1) && (jk_0==j))
					sm_r_1[j] <= 1'b1;
			else if(allow_ts_req[1] && (cln_1==3'h1) && (jk_1==j))
					sm_r_1[j] <= 1'b1;
			else if(allow_ts_req[2] && (cln_2==3'h1) && (jk_2==j))
					sm_r_1[j] <= 1'b1;
			else if(allow_ts_req[3] && (cln_3==3'h1) && (jk_3==j))
					sm_r_1[j] <= 1'b1;
			//Vector Set
			else if(write_sm[0] && (cln_0==3'h1))
					sm_r_1[j] <= intercpu_si_0[j];
			else if(write_sm[1] && (cln_1==3'h1))
					sm_r_1[j] <= intercpu_si_1[j];
			else if(write_sm[2] && (cln_2==3'h1))
					sm_r_1[j] <= intercpu_si_2[j];
			else if(write_sm[3] && (cln_3==3'h1))
					sm_r_1[j] <= intercpu_si_3[j];
			//Bit-wise Set
			else if(set_sm[0] && (cln_0==3'h1) && (jk_0==j))
					sm_r_1[j] <= 1'b1;
			else if(set_sm[1] && (cln_1==3'h1) && (jk_1==j))
					sm_r_1[j] <= 1'b1;
			else if(set_sm[2] && (cln_2==3'h1) && (jk_2==j))
					sm_r_1[j] <= 1'b1;
			else if(set_sm[3] && (cln_3==3'h1) && (jk_3==j))
					sm_r_1[j] <= 1'b1;
			//Bit-wise Clear
			else if(clear_sm[0] && (cln_0==3'h1) && (jk_0==j))
					sm_r_1[j] <= 1'b0;
			else if(clear_sm[1] && (cln_1==3'h1) && (jk_1==j))
					sm_r_1[j] <= 1'b0;
			else if(clear_sm[2] && (cln_2==3'h1) && (jk_2==j))
					sm_r_1[j] <= 1'b0;
			else if(clear_sm[3] && (cln_3==3'h1) && (jk_3==j))
					sm_r_1[j] <= 1'b0;
			else
					sm_r_1[j] <= sm_r_1[j];

		//Cluster 2
		always@(posedge clk)
			if(reset)
				sm_r_2[j] <= 1'b0;
			//Test & Set
			else if(allow_ts_req[0] && (cln_0==3'h2) && (jk_0==j))
					sm_r_2[j] <= 1'b1;
			else if(allow_ts_req[1] && (cln_1==3'h2) && (jk_1==j))
					sm_r_2[j] <= 1'b1;
			else if(allow_ts_req[2] && (cln_2==3'h2) && (jk_2==j))
					sm_r_2[j] <= 1'b1;
			else if(allow_ts_req[3] && (cln_3==3'h2) && (jk_3==j))
					sm_r_2[j] <= 1'b1;
			//Vector Set
			else if(write_sm[0] && (cln_0==3'h2))
					sm_r_2[j] <= intercpu_si_0[j];
			else if(write_sm[1] && (cln_1==3'h2))
					sm_r_2[j] <= intercpu_si_1[j];
			else if(write_sm[2] && (cln_2==3'h2))
					sm_r_2[j] <= intercpu_si_2[j];
			else if(write_sm[3] && (cln_3==3'h2))
					sm_r_2[j] <= intercpu_si_3[j];
			//Bit-wise Set
			else if(set_sm[0] && (cln_0==3'h2) && (jk_0==j))
					sm_r_2[j] <= 1'b1;
			else if(set_sm[1] && (cln_1==3'h2) && (jk_1==j))
					sm_r_2[j] <= 1'b1;
			else if(set_sm[2] && (cln_2==3'h2) && (jk_2==j))
					sm_r_2[j] <= 1'b1;
			else if(set_sm[3] && (cln_3==3'h2) && (jk_3==j))
					sm_r_2[j] <= 1'b1;
			//Bit-wise Clear
			else if(clear_sm[0] && (cln_0==3'h2) && (jk_0==j))
					sm_r_2[j] <= 1'b0;
			else if(clear_sm[1] && (cln_1==3'h2) && (jk_1==j))
					sm_r_2[j] <= 1'b0;
			else if(clear_sm[2] && (cln_2==3'h2) && (jk_2==j))
					sm_r_2[j] <= 1'b0;
			else if(clear_sm[3] && (cln_3==3'h2) && (jk_3==j))
					sm_r_2[j] <= 1'b0;
			else
					sm_r_2[j] <= sm_r_2[j];
					
		//Cluster 3
		always@(posedge clk)
			if(reset)
				sm_r_3[j] <= 1'b0;
			//Test & Set
			else if(allow_ts_req[0] && (cln_0==3'h3) && (jk_0==j))
					sm_r_3[j] <= 1'b1;
			else if(allow_ts_req[1] && (cln_1==3'h3) && (jk_1==j))
					sm_r_3[j] <= 1'b1;
			else if(allow_ts_req[2] && (cln_2==3'h3) && (jk_2==j))
					sm_r_3[j] <= 1'b1;
			else if(allow_ts_req[3] && (cln_3==3'h3) && (jk_3==j))
					sm_r_3[j] <= 1'b1;
			//Vector Set
			else if(write_sm[0] && (cln_0==3'h3))
					sm_r_3[j] <= intercpu_si_0[j];
			else if(write_sm[1] && (cln_1==3'h3))
					sm_r_3[j] <= intercpu_si_1[j];
			else if(write_sm[2] && (cln_2==3'h3))
					sm_r_3[j] <= intercpu_si_2[j];
			else if(write_sm[3] && (cln_3==3'h3))
					sm_r_3[j] <= intercpu_si_3[j];
			//Bit-wise Set
			else if(set_sm[0] && (cln_0==3'h3) && (jk_0==j))
					sm_r_3[j] <= 1'b1;
			else if(set_sm[1] && (cln_1==3'h3) && (jk_1==j))
					sm_r_3[j] <= 1'b1;
			else if(set_sm[2] && (cln_2==3'h3) && (jk_2==j))
					sm_r_3[j] <= 1'b1;
			else if(set_sm[3] && (cln_3==3'h3) && (jk_3==j))
					sm_r_3[j] <= 1'b1;
			//Bit-wise Clear
			else if(clear_sm[0] && (cln_0==3'h3) && (jk_0==j))
					sm_r_3[j] <= 1'b0;
			else if(clear_sm[1] && (cln_1==3'h3) && (jk_1==j))
					sm_r_3[j] <= 1'b0;
			else if(clear_sm[2] && (cln_2==3'h3) && (jk_2==j))
					sm_r_3[j] <= 1'b0;
			else if(clear_sm[3] && (cln_3==3'h3) && (jk_3==j))
					sm_r_3[j] <= 1'b0;
			else
					sm_r_3[j] <= sm_r_3[j];
					
		//Cluster 4
		always@(posedge clk)
			if(reset)
				sm_r_4[j] <= 1'b0;
			//Test & Set
			else if(allow_ts_req[0] && (cln_0==3'h4) && (jk_0==j))
					sm_r_4[j] <= 1'b1;
			else if(allow_ts_req[1] && (cln_1==3'h4) && (jk_1==j))
					sm_r_4[j] <= 1'b1;
			else if(allow_ts_req[2] && (cln_2==3'h4) && (jk_2==j))
					sm_r_4[j] <= 1'b1;
			else if(allow_ts_req[3] && (cln_3==3'h4) && (jk_3==j))
					sm_r_4[j] <= 1'b1;
			//Vector Set
			else if(write_sm[0] && (cln_0==3'h4))
					sm_r_4[j] <= intercpu_si_0[j];
			else if(write_sm[1] && (cln_1==3'h4))
					sm_r_4[j] <= intercpu_si_1[j];
			else if(write_sm[2] && (cln_2==3'h4))
					sm_r_4[j] <= intercpu_si_2[j];
			else if(write_sm[3] && (cln_3==3'h4))
					sm_r_4[j] <= intercpu_si_3[j];
			//Bit-wise Set
			else if(set_sm[0] && (cln_0==3'h4) && (jk_0==j))
					sm_r_4[j] <= 1'b1;
			else if(set_sm[1] && (cln_1==3'h4) && (jk_1==j))
					sm_r_4[j] <= 1'b1;
			else if(set_sm[2] && (cln_2==3'h4) && (jk_2==j))
					sm_r_4[j] <= 1'b1;
			else if(set_sm[3] && (cln_3==3'h4) && (jk_3==j))
					sm_r_4[j] <= 1'b1;
			//Bit-wise Clear
			else if(clear_sm[0] && (cln_0==3'h4) && (jk_0==j))
					sm_r_4[j] <= 1'b0;
			else if(clear_sm[1] && (cln_1==3'h4) && (jk_1==j))
					sm_r_4[j] <= 1'b0;
			else if(clear_sm[2] && (cln_2==3'h4) && (jk_2==j))
					sm_r_4[j] <= 1'b0;
			else if(clear_sm[3] && (cln_3==3'h4) && (jk_3==j))
					sm_r_4[j] <= 1'b0;
			else 
					sm_r_4[j] <= sm_r_4[j];
					
		//Cluster 5
		always@(posedge clk)
			if(reset)
				sm_r_5[j] <= 1'b0;
			//Test & Set
			else if(allow_ts_req[0] && (cln_0==3'h5) && (jk_0==j))
					sm_r_5[j] <= 1'b1;
			else if(allow_ts_req[1] && (cln_1==3'h5) && (jk_1==j))
					sm_r_5[j] <= 1'b1;
			else if(allow_ts_req[2] && (cln_2==3'h5) && (jk_2==j))
					sm_r_5[j] <= 1'b1;
			else if(allow_ts_req[3] && (cln_3==3'h5) && (jk_3==j))
					sm_r_5[j] <= 1'b1;
			//Vector Set
			else if(write_sm[0] && (cln_0==3'h5))
					sm_r_5[j] <= intercpu_si_0[j];
			else if(write_sm[1] && (cln_1==3'h5))
					sm_r_5[j] <= intercpu_si_1[j];
			else if(write_sm[2] && (cln_2==3'h5))
					sm_r_5[j] <= intercpu_si_2[j];
			else if(write_sm[3] && (cln_3==3'h5))
					sm_r_5[j] <= intercpu_si_3[j];
			//Bit-wise Set
			else if(set_sm[0] && (cln_0==3'h5) && (jk_0==j))
					sm_r_5[j] <= 1'b1;
			else if(set_sm[1] && (cln_1==3'h5) && (jk_1==j))
					sm_r_5[j] <= 1'b1;
			else if(set_sm[2] && (cln_2==3'h5) && (jk_2==j))
					sm_r_5[j] <= 1'b1;
			else if(set_sm[3] && (cln_3==3'h5) && (jk_3==j))
					sm_r_5[j] <= 1'b1;
			//Bit-wise Clear
			else if(clear_sm[0] && (cln_0==3'h5) && (jk_0==j))
					sm_r_5[j] <= 1'b0;
			else if(clear_sm[1] && (cln_1==3'h5) && (jk_1==j))
					sm_r_5[j] <= 1'b0;
			else if(clear_sm[2] && (cln_2==3'h5) && (jk_2==j))
					sm_r_5[j] <= 1'b0;
			else if(clear_sm[3] && (cln_3==3'h5) && (jk_3==j))
					sm_r_5[j] <= 1'b0;
			else
					sm_r_5[j] <= sm_r_5[j];
	end
endgenerate
/*
wire [3:0] write_rtc, read_rtc;
wire [3:0] write_sb, read_sb;
wire [3:0] write_st, read_st;
wire [3:0] clear_sm, set_sm, read_sm, write_sm;
*/										  

endmodule
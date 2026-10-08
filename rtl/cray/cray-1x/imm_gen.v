//This block is used to source immediate values
//when loading them into a/s registers

module imm_gen (
	i_instr,
	i_cip_i,
	i_cip_j,
	i_cip_k,
	i_lip,
	i_sj,
	i_vl,
	o_a_result,
	o_s_result
);

	input wire [6:0] i_instr;
	input wire [2:0] i_cip_i;
	input wire [2:0] i_cip_j;
	input wire [2:0] i_cip_k;
	input wire [15:0] i_lip;
	input wire [63:0] i_sj;
	input wire [6:0] i_vl;
	output reg [23:0] o_a_result;  //for 020 to 023 and 01h, while the instruction is in CIP
	output wire [63:0] o_s_result;  //for 040 and 041, while the instruction is in CIP

	wire [21:0] inv_jkm;


	assign inv_jkm = ~{i_cip_j, i_cip_k, i_lip};  // bitwise: 021/041 transmit the complement of jkm

	//01hijkm with the high bit of i set: the low 24 bits of ijkm to Ah
	wire long_a = (i_instr[6:3] == 4'b0001) && i_cip_i[2];
	//023i01: the VL register to Ai
	wire read_vl = ({i_cip_j, i_cip_k} == 6'o01);

	//Figure out which immediate value to send to Ai
	always @*
		if (long_a) o_a_result = {i_cip_i[1:0], i_cip_j, i_cip_k, i_lip};
		else
			case (i_instr[1:0])
				2'd0: o_a_result = {2'b00, i_cip_j, i_cip_k, i_lip};  //020: transmit jkm to Ai
				2'd1: o_a_result = {2'b11, inv_jkm};  //021: transmit 1's complement of jkm to Ai
				2'd2: o_a_result = {18'b0, i_cip_j, i_cip_k};  //022: transmit jk to Ai
				2'd3: o_a_result = read_vl ? {17'b0, i_vl} : i_sj[23:0];  //023: VL, or the lower 24 bits of Sj, to Ai
			endcase

	//Figure out which immediate value to send to Si
	assign o_s_result = (i_instr == 7'o041) ? {42'h3FFFFFFFFFF, inv_jkm} : {42'b0, i_cip_j, i_cip_k, i_lip};


endmodule

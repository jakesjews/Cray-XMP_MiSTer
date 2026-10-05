//This block is used to source immediate values
//when loading them into a/s registers

module imm_gen (
	clk,
	i_instr,
	i_cip_j,
	i_cip_k,
	i_lip,
	i_sj,
	o_a_result,
	o_s_result
);

	input clk;
	input wire [6:0] i_instr;
	input wire [2:0] i_cip_j;
	input wire [2:0] i_cip_k;
	input wire [15:0] i_lip;
	input wire [63:0] i_sj;
	output reg [23:0] o_a_result;
	output reg [63:0] o_s_result;

	wire [21:0] inv_jkm;


	assign inv_jkm = ~{i_cip_j, i_cip_k, i_lip};  // bitwise: 021/041 transmit the complement of jkm

	//Figure out which immediate value to send to Ai
	always @(posedge clk)
		if (i_instr == 7'o020)  //transmit jkm to Ai
			o_a_result <= {2'b00, i_cip_j, i_cip_k, i_lip};
		else if (i_instr == 7'o021)  //transmit 1's complement of jkm to Ai
			o_a_result <= {2'b11, inv_jkm};
		else if (i_instr == 7'o022)  //transmit jk to Ai
			o_a_result <= {18'b0, i_cip_j, i_cip_k};
		else if (i_instr == 7'o023)  //transmit lower 24 bits of Sj to Ai
			o_a_result <= i_sj[23:0];

	//Figure out which immediate value to send to Si
	always @(posedge clk)
		if (i_instr == 7'o040) o_s_result <= {42'b0, i_cip_j, i_cip_k, i_lip};
		else if (i_instr == 7'o041) o_s_result <= {42'h3FFFFFFFFFF, inv_jkm};


endmodule

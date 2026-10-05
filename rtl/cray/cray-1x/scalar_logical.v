//******************************************
//       Scalar Logical Unit
//******************************************
//executes instructions 042-051 (mask and boolean)

// 042ijk   - Form 64-jk bits one's mask in Si from right
// 043ijk   - Form jk bits of one's mask in Si from the left
// 044ijk   - Logical product of (Sj) AND (Sk) to Si
// 045ijk   - Logical product of (Sj) and complement of (Sk) to Si
// 046ijk   - Logical difference of (Sj) and (Sk) to Si
// 047ijk   - Logical equivalence of (Sk) and (Sj) to Si
// 050ijk   - Scalar merge
// 051ijk   - Logical sum of (Sj) and (Sk) to Si
//
module scalar_logical (
	i_instr,
	i_j,
	i_k,
	i_sj,
	i_sk,
	i_si,
	o_result,
	clk
);
	input wire [6:0] i_instr;
	input wire [2:0] i_j;
	input wire [2:0] i_k;
	input wire [63:0] i_sj;
	input wire [63:0] i_sk;
	input wire [63:0] i_si;
	input wire clk;
	output reg [63:0] o_result;

	wire [63:0] final_s_j;
	wire [63:0] final_s_k;

	assign final_s_j[63:0] = (i_j[2:0] == 3'b000) ? 64'b0 : i_sj[63:0];
	assign final_s_k[63:0] = (i_k[2:0] == 3'b000) ? {1'b1, 63'b0} : i_sk[63:0];

	always @(posedge clk) begin
		case (i_instr[6:0])
			7'b0100010: o_result[63:0] <= 64'hffffffffffffffff >> {i_j[2:0], i_k[2:0]};  //042
			7'b0100011: o_result[63:0] <= 64'hffffffffffffffff << (64'h40 - {58'b0, i_j[2:0], i_k[2:0]});  //043
			7'b0100100: o_result[63:0] <= final_s_j[63:0] & final_s_k[63:0];  //044
			7'b0100101: o_result[63:0] <= final_s_j[63:0] & ~final_s_k[63:0];  //045
			7'b0100110: o_result[63:0] <= final_s_j[63:0] ^ final_s_k[63:0];  //046
			7'b0100111:
			o_result[63:0] <= (final_s_j[63:0] & final_s_k[63:0]) | (~final_s_j[63:0] & ~final_s_k[63:0]);  //047
			7'b0101000: o_result[63:0] <= (final_s_j[63:0] & final_s_k[63:0]) | (i_si[63:0] & ~final_s_k[63:0]);  //050
			7'b0101001: o_result[63:0] <= (final_s_j[63:0] | final_s_k[63:0]);  //051
			default: ;
		endcase
	end






endmodule

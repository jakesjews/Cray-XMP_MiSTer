//******************************************
//       Scalar Constant Generator Unit
//******************************************
//
//This block is used to handle a few 'special
// case' instructions that load constant floating
// point values into one of the scalar registers
// (among other things)

module s_const_gen(clk,i_j,i_ak,o_result);
input wire clk;
input wire [2:0] i_j;
input wire [23:0] i_ak;
output reg [63:0] o_result;

reg [2:0]  cur_j;
reg [23:0] cur_ak;
wire [23:0] twos_compl_ak;
wire [23:0] float_ak;
assign twos_compl_ak = ~cur_ak + 24'b1;

assign float_ak = cur_ak[23] ? twos_compl_ak : cur_ak;

always@(posedge clk)
begin
   cur_j  <= i_j;
	cur_ak <= i_ak;
end
	
always@(posedge clk)
   case(cur_j)
	   3'b000:o_result <= {40'b0,cur_ak};
		3'b001:o_result <= {{40{cur_ak[23]}},cur_ak};
		3'b010:o_result <= {cur_ak[23],15'o40060,24'b0,float_ak};
		3'b011:o_result <= 64'o0400606000000000000000;  //0.75 * 2^48
		3'b100:o_result <= 64'o0400004000000000000000;  //0.5
		3'b101:o_result <= 64'o0400014000000000000000;  //1.0
		3'b110:o_result <= 64'o0400024000000000000000;  //2.0
		3'b111:o_result <= 64'o0400034000000000000000;  //4.0
	endcase


endmodule
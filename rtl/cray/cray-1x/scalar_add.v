//******************************************
//       Scalar ADD UNIT
//******************************************
//The scalar add unit performs 64-bit integer
//addition and subtraction. It executes instructions
//060 and 061. The addition and subtraction are
//performed in a similar manner. However, the two's
//complement subtraction for the 061 instruction
//occurs as follows. The one's complement of the Sk
//operand is added to the Sj operand. Then a one is
//added in the low order bit position of the
//result. No overflow is detected in the unit.
//
//The functional time is 3 clock periods.

module scalar_add (
	i_sk,
	i_sj,
	i_instr,
	clk,
	o_result
);

	input wire [63:0] i_sk;
	input wire [63:0] i_sj;

	output wire [63:0] o_result;  //the sum, two clocks after the operands
	input wire clk;
	input wire [6:0] i_instr;

	reg [ 6:0] instr;
	reg [63:0] sk_0;  //operand 1 
	reg [63:0] sj_0;  //operand 2
	reg [63:0] temp_result;

	always @(posedge clk) begin
		//grab new values
		sk_0[63:0] <= i_sk[63:0];
		sj_0[63:0] <= i_sj[63:0];
		instr[6:0] <= i_instr[6:0];
	end


	always @(posedge clk) begin
		temp_result[63:0] <= (instr[6:0]==7'b0110000) ? (sk_0[63:0] + sj_0[63:0]) : (sj_0[63:0] + ~sk_0[63:0] + 64'h000001);
	end

	//The unit's third clock is the one its result spends on the S result bus
	assign o_result = temp_result;

endmodule

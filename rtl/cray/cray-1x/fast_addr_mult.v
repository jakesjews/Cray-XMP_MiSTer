//******************************************
//       ADDRESS Multiply UNIT
//******************************************
//
//It takes 6 time units to produce a 24-bit result. No overflow is detected.
//If I add 4 partial products per cycle, it should work fine.

module fast_addr_mult (
	i_aj,
	i_ak,
	clk,
	o_result
);


	input wire clk;
	input wire [23:0] i_aj;
	input wire [23:0] i_ak;

	output wire [23:0] o_result;  //the product, five clocks after the operands

	reg [23:0] aj_0;
	reg [23:0] ak_0;
	reg [23:0] result_jk;  //the low half of the product is all that is kept

	reg [23:0] temp2;
	reg [23:0] temp3;
	reg [23:0] temp4;

	always @(posedge clk) begin
		//Flop the incoming
		ak_0      <= i_ak;
		aj_0      <= i_aj;
		//Do the actual multiplication (this should use hardware multipliers)
		result_jk <= aj_0 * ak_0;
		//Just pipeline the result along
		temp2     <= result_jk;
		temp3     <= temp2;
		temp4     <= temp3;
	end

	//The unit's sixth clock is the one its result spends on the A result bus
	assign o_result = temp4;



endmodule

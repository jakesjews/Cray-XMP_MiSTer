//*******************************************************
//       Scalar Pop Count / Leading Zero Count Unit
//*******************************************************
//
//This functional unit executes instructions 026 and 027.
//Instruction 026, which counts the number of bits having
//a value of 1 in the operand, executes in 4 clock periods.
//Instruction 027, which counts the number of bits of zero
//preceding a one bit in the operand, executes in 3 clock 
//periods. For either instruction, the 64-bit operand is 
//obtained from an S register and the 7-bit result is
//delivered to an A register

//026ijx   - Population count of (Sj) to Ai.
//Counts the number of bits set to one in (Sj) and
//enters the result in the lower order 7 bits of Ai. 
//The upper 17 bits are zeroed. 

//027ijx   - Leading zero count of (Sj) to Ai.
//This instructions counts the number of leading 
//zeros in Sj and enters the result into the low
//order 7 bits of Ai. The upper 17 bits are zeroed.

module scalar_pop_lz (
	i_sj,
	i_instr,
	i_issue,
	clk,
	o_result
);

	input wire [63:0] i_sj;
	input wire clk;
	input wire i_issue;  //the instruction on i_instr issues this clock
	input wire [6:0] i_instr;
	output wire [23:0] o_result;

	reg [63:0] sj0;
	reg [63:0] sj1;
	reg [63:0] sj2;

	reg [6:0] p_tmp0;
	reg [6:0] p_tmp1;
	reg [6:0] p_tmp2;

	wire [23:0] p_result;


	wire [23:0] lz_result;


	wire zb0, zb1, zb2, zb3, zb4, zb5, zb6, zb7;  //zero_bar output wires
	reg zbr0, zbr1, zbr2, zbr3, zbr4, zbr5, zbr6, zbr7;  //zero_bar output pipeline registers
	wire [2:0] zs0, zs1, zs2, zs3, zs4, zs5, zs6, zs7, lz_hi_results;  //"zeros" output wires
	reg [2:0]
		zsr0,
		zsr1,
		zsr2,
		zsr3,
		zsr4,
		zsr5,
		zsr6,
		zsr7,
		lz_hi_results_r,
		lz_low_results_r;  //"zeros" output pipeline registers

	reg lz_msb_result;

	reg [2:0] lz_low_results;


	lz_sub lz_sub0 (
		.i_data (sj0[7:0]),
		.z_bar  (zb0),
		.o_zeros(zs0)
	);
	lz_sub lz_sub1 (
		.i_data (sj0[15:8]),
		.z_bar  (zb1),
		.o_zeros(zs1)
	);
	lz_sub lz_sub2 (
		.i_data (sj0[23:16]),
		.z_bar  (zb2),
		.o_zeros(zs2)
	);
	lz_sub lz_sub3 (
		.i_data (sj0[31:24]),
		.z_bar  (zb3),
		.o_zeros(zs3)
	);
	lz_sub lz_sub4 (
		.i_data (sj0[39:32]),
		.z_bar  (zb4),
		.o_zeros(zs4)
	);
	lz_sub lz_sub5 (
		.i_data (sj0[47:40]),
		.z_bar  (zb5),
		.o_zeros(zs5)
	);
	lz_sub lz_sub6 (
		.i_data (sj0[55:48]),
		.z_bar  (zb6),
		.o_zeros(zs6)
	);
	lz_sub lz_sub7 (
		.i_data (sj0[63:56]),
		.z_bar  (zb7),
		.o_zeros(zs7)
	);

	lz_sub lz_sub_lower (
		.i_data ({zbr7, zbr6, zbr5, zbr4, zbr3, zbr2, zbr1, zbr0}),
		.z_bar  (),
		.o_zeros(lz_hi_results)
	);

	assign p_result[23:0]  = {17'b0, p_tmp2[6:0]};
	assign lz_result[23:0] = {17'b0, lz_msb_result, lz_hi_results_r[2:0], lz_low_results_r[2:0]};


	//A pop-count and leading-zero count can never be issued on back-to-back cycles, so this should never give us a problem
	//The population count is ready four clocks after an 026 issues and the leading zero
	//count three clocks after an 027.  The choice follows the instruction that issued,
	//not whatever parcel happens to be in the pipeline behind it.
	reg [3:0] pop_pipe;
	always @(posedge clk) pop_pipe <= {pop_pipe[2:0], i_issue && (i_instr == 7'b0010110)};
	assign o_result[23:0] = pop_pipe[3] ? p_result[23:0] : lz_result[23:0];

	always @(posedge clk) begin  //just pipelining the state so it carries through
		sj0 <= i_sj;
		sj1 <= sj0;
		sj2 <= sj1;
	end

	//calculate population count
	always @(posedge clk) begin
		p_tmp0 <= {6'b0, sj0[0]} + {6'b0, sj0[1]} + sj0[2] + sj0[3] + sj0[4] + sj0[5] + sj0[6] + sj0[7] + sj0[8] + sj0[9] + sj0[10] + sj0[11] + sj0[12] + sj0[13] + sj0[14] + sj0[15] + sj0[16] + sj0[17] + sj0[18] + sj0[19] + sj0[20];
		p_tmp1 <= p_tmp0 + sj1[21] + sj1[22] + sj1[23] + sj1[24] + sj1[25] + sj1[26] + sj1[27] + sj1[28] + sj1[29] + sj1[30] + sj1[31] + sj1[32] + sj1[33] + sj1[34] + sj1[35] + sj1[36] + sj1[37] + sj1[38] + sj1[39] + sj1[40] + sj1[41];
		p_tmp2 <= p_tmp1 + sj2[42] + sj2[43] + sj2[44] + sj2[45] + sj2[46] + sj2[47] + sj2[48] + sj2[49] + sj2[50] + sj2[51] + sj2[52] + sj2[53] + sj2[54] + sj2[55] + sj2[56] + sj2[57] + sj2[58] + sj2[59] + sj2[60] + sj2[61] + sj2[62] + sj2[63];
	end

	//calculate leading zero count
	always @(posedge clk) begin
		//first pipeline stage
		zbr0             <= zb0;
		zbr1             <= zb1;
		zbr2             <= zb2;
		zbr3             <= zb3;
		zbr4             <= zb4;
		zbr5             <= zb5;
		zbr6             <= zb6;
		zbr7             <= zb7;
		zsr0             <= zs0;
		zsr1             <= zs1;
		zsr2             <= zs2;
		zsr3             <= zs3;
		zsr4             <= zs4;
		zsr5             <= zs5;
		zsr6             <= zs6;
		zsr7             <= zs7;
		//second pipeline stage
		lz_hi_results_r  <= lz_hi_results;
		lz_low_results_r <= lz_low_results;
		lz_msb_result    <= ~|{zbr0, zbr1, zbr2, zbr3, zbr4, zbr5, zbr6, zbr7};
	end

	//mux in second stage of pipeline to choose LSBs of the final LZC;
	always @* begin
		case (lz_hi_results[2:0])
			3'b000: lz_low_results[2:0] = zsr7;
			3'b001: lz_low_results[2:0] = zsr6;
			3'b010: lz_low_results[2:0] = zsr5;
			3'b011: lz_low_results[2:0] = zsr4;
			3'b100: lz_low_results[2:0] = zsr3;
			3'b101: lz_low_results[2:0] = zsr2;
			3'b110: lz_low_results[2:0] = zsr1;
			3'b111: lz_low_results[2:0] = zsr0;
		endcase
	end

endmodule

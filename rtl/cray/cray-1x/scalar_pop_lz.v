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

//026ij1   - Population count parity of (Sj) to Ai.
//Enters only the low order bit of that count in the
//low order bit of Ai (HR-0004 rev F page 4-25).

//027ijx   - Leading zero count of (Sj) to Ai.
//This instructions counts the number of leading 
//zeros in Sj and enters the result into the low
//order 7 bits of Ai. The upper 17 bits are zeroed.

module scalar_pop_lz (
	i_sj,
	i_parity,
	clk,
	o_pop,
	o_lz
);
	input wire [63:0] i_sj;
	input wire clk;
	input wire i_parity;  //the instruction is 026ij1
	output wire [23:0] o_pop;  //the population count or its parity, four clocks after the operand
	output wire [23:0] o_lz;  //the leading zero count, three clocks after the operand

	//Both results come out of registers that the A result bus reads directly.

	reg [63:0] sj0;
	reg [63:0] sj1;
	reg [63:0] sj2;

	always @(posedge clk) begin  //just pipelining the state so it carries through
		sj0 <= i_sj;
		sj1 <= sj0;
		sj2 <= sj1;
	end

	//---- population count: the bits are added up a third at a time ----
	reg [6:0] p_tmp0;
	reg [6:0] p_tmp1;
	reg [6:0] p_out;
	reg [2:0] par_pipe;
	wire [ 6:0] p_sum = p_tmp1 + sj2[42] + sj2[43] + sj2[44] + sj2[45] + sj2[46] + sj2[47] + sj2[48] + sj2[49] + sj2[50] + sj2[51] + sj2[52] + sj2[53] + sj2[54] + sj2[55] + sj2[56] + sj2[57] + sj2[58] + sj2[59] + sj2[60] + sj2[61] + sj2[62] + sj2[63];

	always @(posedge clk) begin
		par_pipe <= {par_pipe[1:0], i_parity};
		p_tmp0   <= {6'b0, sj0[0]} + {6'b0, sj0[1]} + sj0[2] + sj0[3] + sj0[4] + sj0[5] + sj0[6] + sj0[7] + sj0[8] + sj0[9] + sj0[10] + sj0[11] + sj0[12] + sj0[13] + sj0[14] + sj0[15] + sj0[16] + sj0[17] + sj0[18] + sj0[19] + sj0[20];
		p_tmp1   <= p_tmp0 + sj1[21] + sj1[22] + sj1[23] + sj1[24] + sj1[25] + sj1[26] + sj1[27] + sj1[28] + sj1[29] + sj1[30] + sj1[31] + sj1[32] + sj1[33] + sj1[34] + sj1[35] + sj1[36] + sj1[37] + sj1[38] + sj1[39] + sj1[40] + sj1[41];
		p_out <= par_pipe[2] ? {6'b0, p_sum[0]} : p_sum;
	end

	assign o_pop = {17'b0, p_out};

	//---- leading zero count: first the bytes, each by itself, and the first byte
	//that is not zero; then the count within that byte ----
	wire [7:0] zb;  //the byte is not zero
	wire [2:0] zs                                                       [0:7];  //leading zeros within the byte
	wire [2:0] hi;  //bytes that are zero ahead of the first that is not
	reg  [2:0] zsr                                                      [0:7];
	reg  [2:0] hi_r;
	reg        all_r;  //the whole word is zero
	reg  [6:0] lz_out;

	genvar g;
	generate
		for (g = 0; g < 8; g = g + 1) begin : g_byte
			lz_sub lz_byte (
				.i_data (sj0[8*g+:8]),
				.z_bar  (zb[g]),
				.o_zeros(zs[g])
			);
			always @(posedge clk) zsr[g] <= zs[g];
		end
	endgenerate

	lz_sub lz_bytes (
		.i_data (zb),
		.z_bar  (),
		.o_zeros(hi)
	);

	always @(posedge clk) begin
		hi_r   <= hi;
		all_r  <= ~|zb;
		lz_out <= {all_r, hi_r, zsr[3'd7-hi_r]};
	end

	assign o_lz = {17'b0, lz_out};

endmodule

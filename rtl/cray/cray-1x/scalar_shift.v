//******************************************
//            Scalar Shift Unit
//******************************************
//
//The scalar shift unit shifts the entire 64-bit
//contents of an S-register or shifts the double 
//128-bit contents of two concatenated S registers.
//Shift counts are obtained from an A register or
//from the jk portion of the instruction. Shifts 
//are end off with zero fill. For a double-shift,
//a circular shift is effected if the shift count
//does not exceed 64 and the i and j designators
//are equal and non-zero.

//The scalar shift unit executes instructions 052
//through 057. Single-register shift instructions,
// 052 through 055, are executed in two clock
//periods. Double-register shift instructions, 056
//and 057, are executed in three clock periods.

//{g[3:0],h[2:0],i[2:0],jk[5:0]}
//g,h==opcode, i==oprnd and result reg, jk==shift, mask count

//052ijk == Shift (Si) left jk places and enters the result into S0
//053ijk == Shift (Si) right by 64-jk places and enters the result in S0
//054ijk == Shift (Si) left jk places and enters the result into Si
//055ijk == Shift (Si) right by 64-jk places and enters the result into Si


//056ijk == Shift (Si) and (Sj) left by (Ak) places to Si
//057ijk == Shift (Si) right by (Ak) places to Si

//Hold issue conditions:
//      034-037 in process
//      Exchange in process
//      S register access conflict
//      Si reserved
//      S0 reserved (052 and 053 only)

//Execution time
//      for 052, 053, S0 ready - 2 CPs
//      for 054, 055, Si ready - 2 CPs
//      for 056, 057 - 3 CPs
//      Instruction issue - 1 CP

module scalar_shift (
	clk,
	i_si,
	i_sj,
	i_ak,
	i_instr,
	i_j,
	i_k,
	o_result
);

	input wire [63:0] i_si;
	input wire [63:0] i_sj;
	input wire [23:0] i_ak;
	input wire [6:0] i_instr;
	input wire [2:0] i_j;
	input wire [2:0] i_k;
	input wire clk;
	output wire [63:0] o_result;

	reg  [  6:0] temp_instr0;
	reg  [ 63:0] temp_sj;
	reg  [ 63:0] temp_si;
	reg  [ 23:0] temp_ak;
	reg  [  5:0] temp_jk;
	reg  [ 63:0] result0;
	wire [127:0] result_d1_l;
	wire [127:0] result_d1_r;
	wire         shift_clear;


	//Detect if the shift count is greater than 127 (any bits [23:7] are high), and just clear the reg
	assign shift_clear = |temp_ak[23:7];

	//The operands are taken in during the clock the instruction issues and shifted in
	//the next.  A single shift is due the clock after that, straight from here; a
	//double shift a clock later, and that clock it spends on the S result bus.
	assign o_result = result0;

	always @(posedge clk) begin
		temp_instr0[6:0] <= i_instr[6:0];
		temp_si[63:0]    <= i_si[63:0];
		temp_sj[63:0]    <= i_sj[63:0];
		temp_ak[23:0]    <= i_ak[23:0];
		temp_jk[5:0]     <= {i_j[2:0], i_k[2:0]};
		case (temp_instr0[6:0])
			7'b0101010: result0[63:0] <= temp_si[63:0] << temp_jk;  //052
			7'b0101011: result0[63:0] <= temp_si[63:0] >> (7'd64 - temp_jk);  //053
			7'b0101100: result0[63:0] <= temp_si[63:0] << temp_jk;  //054
			7'b0101101: result0[63:0] <= temp_si[63:0] >> (7'd64 - temp_jk);  //055
			7'b0101110: begin  //056
				if (temp_jk[2:0] == 3'b0)  //056ij0 - shift {Si,Sj} << 1, take upper 64 bits
					result0[63:0] <= {temp_si[62:0], temp_sj[63]};
				else if (temp_jk[5:3] == 3'b0)  //056i0k - Shift Si << Ak
					result0[63:0] <= (|temp_ak[23:6]) ? 64'b0 : (temp_si[63:0] << temp_ak[5:0]);
				else  //056ijk
					result0[63:0] <= shift_clear ? 64'b0 : result_d1_l[127:64];
			end
			7'b0101111: begin  //057
				if (temp_jk[2:0] == 3'b0)  //057ij0 - shift {Sj,Si} >> 1, take lower 64 bits
					result0[63:0] <= {temp_sj[0], temp_si[63:1]};
				else if (temp_jk[5:3] == 3'b0)  //057i0k
					result0[63:0] <= (|temp_ak[23:6]) ? 64'b0 : (temp_si[63:0] >> temp_ak[5:0]);
				else  //057ijk
					result0[63:0] <= shift_clear ? 64'b0 : result_d1_r[63:0];
			end
			default:    ;
		endcase
	end

	assign result_d1_l[127:0] = {temp_si[63:0], temp_sj[63:0]} << temp_ak[6:0];
	assign result_d1_r[127:0] = {temp_sj[63:0], temp_si[63:0]} >> temp_ak[6:0];


endmodule

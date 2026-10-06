// What the issue logic needs to know about an instruction, decoded from its
// first parcel alone.
//
// func_top applies this to the parcel in NIP and loads the outcome into a
// register together with CIP.  Whether the current instruction may issue is
// then decided from flip-flops and not through a decode of CIP, which was the
// first half of the longest path in the CPU.  The fields are named in
// cray_pd.vh.
//
// Nothing here depends on the state of the machine.  What does (which
// registers have a result on its way, which units are busy) meets these fields
// in func_top, the schedulers and the branch unit.

module cray_predecode (
	i_parcel,
	o_pd
);
	parameter XMP = 0;

	`include "cray_pd.vh"

	input wire [15:0] i_parcel;
	output wire [PD_W-1:0] o_pd;

	wire [6:0] op = i_parcel[15:9];
	wire [2:0] i = i_parcel[8:6];
	wire [2:0] j = i_parcel[5:3];
	wire [2:0] k = i_parcel[2:0];

	//------------------------------------------------------------------
	// Registers read, kind of instruction, special waits
	//------------------------------------------------------------------
	cray_opnd #(
		.XMP(XMP)
	) opnd (
		.i_cip (i_parcel),
		.o_rd_a(o_pd[PD_RD_A+:8]),
		.o_rd_s(o_pd[PD_RD_S+:8])
	);

	assign o_pd[PD_TWO] = (i_parcel[15:10] == 6'b000011) ||  //006-007
		(i_parcel[15:12] == 4'b0001) ||  //010-017
		(i_parcel[15:10] == 6'b001000) ||  //020-021
		(i_parcel[15:10] == 6'b010000) ||  //040-041
		(i_parcel[15:14] == 2'b10);  //100-137
	assign o_pd[PD_EXCH] = (op == 7'o000) || (op == 7'o004);
	assign o_pd[PD_VTYPE] = (i_parcel[15:14] == 2'b11);
	assign o_pd[PD_MTYPE] = (i_parcel[15:11] == 5'b00111) || (i_parcel[15:14] == 2'b10) || (i_parcel[15:10] == 6'b111111);
	assign o_pd[PD_BTYPE] = (i_parcel[15:12] == 4'b0001) || (op == 7'o005) || (op == 7'o006) || (op == 7'o007);

	//076 reads a V register that must not be in use; 003, 073 and the merges 146 and 147
	//wait for a 175 to finish building the mask, and 003 for a merge to finish using it;
	//a scalar floating point instruction waits for a vector operation to leave its unit
	assign o_pd[PD_H076+:8] = (op == 7'o076) ? (8'd1 << j) : 8'd0;
	assign o_pd[PD_HVM] = (op == 7'o003) || (op == 7'o073) || (op == 7'o146) || (op == 7'o147) || (op == 7'o175);
	assign o_pd[PD_H003] = (op == 7'o003);
	assign o_pd[PD_HFADD] = (op == 7'o062) || (op == 7'o063);
	assign o_pd[PD_HFMUL] = (op[6:2] == 5'b01101);
	assign o_pd[PD_HFRCP] = (op == 7'o070);
	//the mode instructions 0021 to 0027 and the status register read 073i01
	assign o_pd[PD_HMODE] = ((op == 7'o002) && (i != 3'd0)) || ((XMP != 0) && (op == 7'o073) && (i_parcel[5:0] == 6'o01));
	assign o_pd[PD_TS] = (i_parcel[15:6] == 10'o0034);
	assign o_pd[PD_H074] = (op == 7'o074);
	assign o_pd[PD_H024] = (op == 7'o024);
	assign o_pd[PD_H072] = (op == 7'o072);

	//------------------------------------------------------------------
	// The instruction as the A and S schedulers see it
	//------------------------------------------------------------------
	//On a CRAY-1 the fields the manual marks x are ignored (023ijx, 026ijx, 027ijx,
	//072ixx, 073ixx).  The X-MP gives some of those encodings a meaning, and the lookup
	//tables tell them by the fields left in place here: 026ij7 and 027ij7 (SBj), 072i02
	//and 073i02 (the semaphores), 072ij3 and 073ij3 (STj) and 073i01 (the status
	//register).  026ij1 is told apart at the population count unit.
	reg [15:0] dec;
	always @* begin
		dec = i_parcel;
		case (op)
			7'o023: dec = {i_parcel[15:3], 3'b000};
			7'o026, 7'o027: if (!XMP || (k != 3'd7)) dec = {i_parcel[15:3], 3'b000};
			7'o072: if (!XMP || !((i_parcel[5:0] == 6'o02) || (k == 3'd3))) dec = {i_parcel[15:6], 6'b000000};
			7'o073:
			if (!XMP || !((i_parcel[5:0] == 6'o01) || (i_parcel[5:0] == 6'o02) || (k == 3'd3)))
				dec = {i_parcel[15:6], 6'b000000};
			default: ;
		endcase
	end

	wire [6:0] d_op = dec[15:9];
	wire [7:0] di = 8'd1 << dec[8:6];
	wire [7:0] dj = 8'd1 << dec[5:3];
	wire [7:0] dk = 8'd1 << dec[2:0];

	//------------------------------------------------------------------
	// S scheduler
	//------------------------------------------------------------------
	wire [3:0] s_delay;
	wire [4:0] s_src;
	wire       s_en;
	s_res_lut sbus_res_lut (
		.i_cip      (dec),
		.o_delay    (s_delay),
		.o_src      (s_src),
		.o_s_dest_en(s_en)
	);

	wire s_type = ((d_op[6:5] == 2'b01) && !(d_op == 7'b0111101) && !((d_op == 7'o073) && ((dec[5:0] == 6'o02) || (dec[2:0] == 3'o3)))) ||
		(d_op[6:3] == 4'b1010);
	//052 and 053 shift (Si) but put the result in S0
	wire s_to_s0 = (d_op == 7'o052) || (d_op == 7'o053);
	wire [7:0] s_dest = s_to_s0 ? 8'b00000001 : di;

	//A result due in d clocks enters stage d - 1 of the pipeline.  The pipeline takes
	//results due in 1 to 7, 11 and 14 clocks, and the stage it enters must
	//be about to become empty: that is stage d now.
	localparam [13:0] S_TAKES = 14'b10010001111111;
	genvar g;
	generate
		for (g = 0; g < 14; g = g + 1) begin : g_s
			assign o_pd[PD_S_STAGE+g] = s_type && s_en && S_TAKES[g] && (s_delay == g + 1);
			assign o_pd[PD_S_WPC+g]   = (s_delay == g);
		end
	endgenerate

	assign o_pd[PD_S_TYPE]     = s_type;
	assign o_pd[PD_S_SRC+:5]   = s_src;
	assign o_pd[PD_S_DEST+:8]  = s_dest;
	assign o_pd[PD_S_DNUM+:3]  = s_to_s0 ? 3'b000 : dec[8:6];
	assign o_pd[PD_S_CMASK+:8] = di | dj | dk | s_dest;
	//077: transmit (Sj) to element (Ak) of Vi
	assign o_pd[PD_S_077]      = (d_op == 7'b0111111);
	assign o_pd[PD_S_VW+:8]    = (d_op == 7'b0111111) ? di : 8'd0;

	//------------------------------------------------------------------
	// A scheduler
	//------------------------------------------------------------------
	wire [3:0] a_delay;
	wire [3:0] a_src;
	wire       a_en;
	a_res_lut abus_res_lut (
		.i_cip      (dec),
		.o_delay    (a_delay),
		.o_src      (a_src),
		.o_a_dest_en(a_en)
	);

	//020-037 and 100-107, except 034-037 (the block transfers) and 027ij7
	wire a_type = (((d_op[6:4] == 3'b001) && (d_op[6:2] != 5'b00111)) || (d_op[6:3] == 4'b1000)) &&
		!((d_op == 7'b0010111) && (dec[2:0] == 3'b111));

	//results due in 1, 2, 3, 4, 6 and 11 clocks
	localparam [10:0] A_TAKES = 11'b10000101111;
	generate
		for (g = 0; g < 11; g = g + 1) begin : g_a
			assign o_pd[PD_A_STAGE+g] = a_type && a_en && A_TAKES[g] && (a_delay == g + 1);
			assign o_pd[PD_A_WPC+g]   = (a_delay == g);
		end
	endgenerate

	assign o_pd[PD_A_TYPE]     = a_type;
	assign o_pd[PD_A_SRC+:4]   = a_src;
	assign o_pd[PD_A_DEST+:8]  = di;
	assign o_pd[PD_A_DNUM+:3]  = dec[8:6];
	assign o_pd[PD_A_CMASK+:8] = di | dj | dk;
	assign o_pd[PD_A_025]      = (d_op == 7'o025);
	//023, Ai Sj: the S register must have no result on its way
	assign o_pd[PD_A_SCONF+:8] = ((d_op == 7'b0010011) && (dec[2:0] == 3'b0)) ? dj : 8'd0;

	//------------------------------------------------------------------
	// V scheduler
	//------------------------------------------------------------------
	localparam VLOG = 3'b000,  //vector logical
	VSHIFT = 3'b001,  //vector shift
	VADD = 3'b010, FP_MUL = 3'b011,  //FP multiply
	FP_ADD = 3'b100,  //FP adder
	FP_RA = 3'b101,  //FP recip. approx.
	VPOP = 3'b110,  //vector pop count / parity
	MEM = 3'b111;

	//174ij1 and 174ij2 are the population count and its parity; any other 174ijk is the reciprocal
	wire        rcpl = (op == 7'o174) && (k != 3'd1) && (k != 3'd2);
	wire [15:0] vdec = rcpl ? {i_parcel[15:3], 3'b000} : i_parcel;

	reg [3:0] fu_delay;
	reg [2:0] fu;
	reg vi_en, vj_en, vk_en;
	always @* begin
		casez (vdec)
			//140-147
			16'b1100????????????: begin
				fu_delay = 4'd2;  //vector logical
				fu       = VLOG;
				vi_en    = 1'b1;
				vk_en    = 1'b1;
				vj_en    = i_parcel[9];  //vj for odd instructions, S for even instructions
			end
			//150-153
			16'b11010???????????: begin
				fu_delay = 4'd4;  //vector shift
				fu       = VSHIFT;
				vi_en    = 1'b1;
				vj_en    = 1'b1;
				vk_en    = 1'b0;
			end
			//154-157
			16'b11011???????????: begin
				fu_delay = 4'd3;  //vector add
				fu       = VADD;
				vi_en    = 1'b1;
				vk_en    = 1'b1;
				vj_en    = i_parcel[9];
			end
			//160-167
			16'b1110????????????: begin
				fu_delay = 4'd7;  //FP mul
				fu       = FP_MUL;
				vi_en    = 1'b1;
				vk_en    = 1'b1;
				vj_en    = i_parcel[9];
			end
			//170-173
			16'b11110???????????: begin
				fu_delay = 4'd6;  //FP add
				fu       = FP_ADD;
				vi_en    = 1'b1;
				vk_en    = 1'b1;
				vj_en    = i_parcel[9];
			end
			//174ij0 - floating point reciprocal approximation
			16'b1111100??????000: begin
				fu_delay = 4'd14;
				fu       = FP_RA;
				vi_en    = 1'b1;
				vk_en    = 1'b0;
				vj_en    = 1'b1;
			end
			//174ij1 - population count of (Vj elements) to Vi elements
			//174ij2 - population count parity of (Vj elements) to Vi elements
			16'b1111100??????001, 16'b1111100??????010: begin
				fu_delay = 4'd6;
				fu       = VPOP;
				vi_en    = 1'b1;
				vj_en    = 1'b1;
				vk_en    = 1'b0;
			end
			//175xj0-175xj3 - create vector mask based on the results of testing the Vj register
			16'b1111101?????????: begin
				fu_delay = 4'd2;  //vector logical
				fu       = VLOG;
				vi_en    = 1'b0;
				vj_en    = 1'b1;
				vk_en    = 1'b0;
			end
			//176ixk-177xj0
			16'b111111??????????: begin
				fu_delay = 4'd2;
				fu       = MEM;
				vi_en    = !i_parcel[9];  //write to Vi for 176
				vj_en    = i_parcel[9];  //read from Vj for 177
				vk_en    = 1'b0;
			end
			default: begin
				fu       = 3'b0;
				fu_delay = 4'b0;
				vi_en    = 1'b0;
				vj_en    = 1'b0;
				vk_en    = 1'b0;
			end
		endcase
	end

	assign o_pd[PD_V_I+:8]     = vi_en ? (8'd1 << i) : 8'd0;
	assign o_pd[PD_V_J+:8]     = vj_en ? (8'd1 << j) : 8'd0;
	assign o_pd[PD_V_K+:8]     = vk_en ? (8'd1 << k) : 8'd0;
	assign o_pd[PD_V_FU+:8]    = 8'd1 << fu;
	assign o_pd[PD_V_DELAY+:4] = fu_delay;

	//------------------------------------------------------------------
	// Branch unit
	//------------------------------------------------------------------
	assign o_pd[PD_BR_005] = (op == 7'o005);
	assign o_pd[PD_BR_A0]  = (i_parcel[15:11] == 5'b00010);
	assign o_pd[PD_BR_S0]  = (i_parcel[15:11] == 5'b00011);
	assign o_pd[PD_BR_JMP] = (op == 7'o006) || (op == 7'o007);

endmodule

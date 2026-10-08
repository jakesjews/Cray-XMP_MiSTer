// Which A and S registers the current instruction reads.
//
// An instruction must not issue while a register it reads still has a result on
// its way (the manual's "hold issue" conditions).  The schedulers check the
// registers of their own file; this decode covers every instruction and both
// files in one place, so that, for example, an S-register shift waits for its
// Ak shift count and a vector operation waits for its scalar operand.
//
// A j, k or h designator of zero is a constant, not a register (manual 4-5).
// An i designator always names the register itself.
//
// The forms for the shared registers are told apart: 027ij7 (SBj Ai) reads Ai
// and no S register, 026ij7 (Ai SBj) reads nothing, and 073ij3 (STj Si) and
// 073i02 (SM Si) read Si.

module cray_opnd (
	input  wire [15:0] i_cip,
	output reg  [ 7:0] o_rd_a,
	output reg  [ 7:0] o_rd_s
);

	wire [6:0] op = i_cip[15:9];
	wire [2:0] h = i_cip[11:9];
	wire [2:0] i = i_cip[8:6];
	wire [2:0] j = i_cip[5:3];
	wire [2:0] k = i_cip[2:0];

	wire [7:0] ri = 8'd1 << i;
	wire [7:0] rj = (j == 3'd0) ? 8'd0 : (8'd1 << j);
	wire [7:0] rk = (k == 3'd0) ? 8'd0 : (8'd1 << k);
	wire [7:0] rh = (h == 3'd0) ? 8'd0 : (8'd1 << h);
	localparam [7:0] R0 = 8'h01;

	always @(*) begin
		o_rd_a = 8'd0;
		o_rd_s = 8'd0;
		casez (op)
			7'o001:
			case (i)  // monitor functions
				3'd0, 3'd1: o_rd_a = rj | rk;  // CA,Aj Ak   CL,Aj Ak
				3'd2, 3'd3: o_rd_a = rj;  // CI,Aj      XA Aj
				3'd4:       o_rd_s = rj;  // RT Sj
				default:    ;
			endcase
			7'o002: if (i == 3'd0) o_rd_a = rk;  // VL Ak
			7'o003: o_rd_s = rj;  // VM Sj
			// with the high bit of i set these are Ah exp, which reads nothing
			7'o010, 7'o011, 7'o012, 7'o013: if (!i[2]) o_rd_a = R0;  // branch on A0
			7'o014, 7'o015, 7'o016, 7'o017: if (!i[2]) o_rd_s = R0;  // branch on S0
			7'o023: o_rd_s = rj;  // Ai Sj
			7'o025: o_rd_a = ri;  // Bjk Ai
			7'o026, 7'o027:
			if (k == 3'd7) begin
				if (op == 7'o027) o_rd_a = ri;  // SBj Ai
			end else o_rd_s = rj;  // Ai PSj, Ai ZSj
			7'o030, 7'o031, 7'o032: o_rd_a = rj | rk;
			7'o033: o_rd_a = rj;
			7'o034, 7'o035, 7'o036, 7'o037: o_rd_a = ri | R0;  // block transfers: count Ai, address A0
			7'o044, 7'o045, 7'o046, 7'o047, 7'o051: o_rd_s = rj | rk;
			7'o050: o_rd_s = ri | rj | rk;  // scalar merge
			7'o052, 7'o053, 7'o054, 7'o055: o_rd_s = ri;
			7'o056, 7'o057: begin
				o_rd_s = ri | rj;
				o_rd_a = rk;
			end
			7'o060, 7'o061, 7'o062, 7'o063, 7'o064, 7'o065, 7'o066, 7'o067: o_rd_s = rj | rk;
			7'o070: o_rd_s = rj;
			7'o071: o_rd_a = rk;
			7'o073: if ((k == 3'd3) || (i_cip[5:0] == 6'o02)) o_rd_s = ri;  // STj Si, SM Si
			7'o075: o_rd_s = ri;  // Tjk Si
			7'o076: o_rd_a = rk;  // Si Vj,Ak
			7'o077: begin
				o_rd_a = rk;
				o_rd_s = rj;
			end  // Vi,Ak Sj
			7'b1000???: o_rd_a = rh;  // Ai exp,Ah
			7'b1001???: o_rd_a = rh | ri;  // exp,Ah Ai
			7'b1010???: o_rd_a = rh;  // Si exp,Ah
			7'b1011???: begin
				o_rd_a = rh;
				o_rd_s = ri;
			end  // exp,Ah Si
			7'o140, 7'o142, 7'o144, 7'o146, 7'o154, 7'o156, 7'o160, 7'o162, 7'o164, 7'o166, 7'o170, 7'o172:
			o_rd_s = rj;  // scalar operand of a vector operation
			7'o150, 7'o151, 7'o152, 7'o153: o_rd_a = rk;  // vector shift count
			7'o176, 7'o177: o_rd_a = R0 | rk;  // vector load and store: address A0, stride Ak
			default: ;
		endcase
	end

endmodule

// Vector population count unit: 174ij1 and 174ij2, the vector population
// instructions option of the 1982 machine (HR-0004 rev F page 4-70).
//
// A plain pipeline like the other vector units: an element in during one
// clock, its result out six clocks later.  174ij1 delivers the number of one
// bits of the element in the low seven bits of the result.  174ij2 delivers
// only the low bit of that number.  All other result bits are zero.  Which of
// the two is wanted travels with the element, because the next operation can
// follow straight behind the last one in the pipeline.

module vector_pop (
	input wire clk,

	input  wire        i_parity,  // 174ij2: the low bit of the count only
	input  wire [63:0] i_d,
	output wire [63:0] o_result
);

	reg [47:0] c4;  // 16 counts of four bits each
	reg [19:0] c16;  // 4 counts of sixteen bits each
	reg [ 6:0] c64;
	reg [ 6:0] hold4;
	reg [ 6:0] hold5;
	reg [ 6:0] out;
	reg [ 4:0] par;  // i_parity beside its element

	integer n;
	always @(posedge clk) begin
		for (n = 0; n < 16; n = n + 1)
		c4[3*n+:3] <= {2'b0, i_d[4*n]} + {2'b0, i_d[4*n+1]} + {2'b0, i_d[4*n+2]} + {2'b0, i_d[4*n+3]};
		for (n = 0; n < 4; n = n + 1)
		c16[5*n+:5] <= {2'b0, c4[12*n+:3]} + {2'b0, c4[12*n+3+:3]} + {2'b0, c4[12*n+6+:3]} + {2'b0, c4[12*n+9+:3]};
		c64   <= {2'b0, c16[0+:5]} + {2'b0, c16[5+:5]} + {2'b0, c16[10+:5]} + {2'b0, c16[15+:5]};
		hold4 <= c64;
		hold5 <= hold4;
		out   <= par[4] ? {6'b0, hold5[0]} : hold5;
		par   <= {par[3:0], i_parity};
	end

	assign o_result = {57'b0, out};

endmodule

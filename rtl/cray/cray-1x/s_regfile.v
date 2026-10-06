//This is a parameterizable register file


//////////////////////////////////////////////////////////////////
//        Scalar Register File                                  //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block contains one 8 entry, 64-bit register file used
//for the Cray-1A's 8 main scalar registers

module s_regfile (
	clk,
	rst,
	i_j_addr,
	i_k_addr,
	i_i_addr,
	i_ex_addr,
	o_ex_data,
	o_j_data,
	o_k_data,
	o_i_data,
	i_wr_addr,
	i_wr_data,
	i_wr_en,
	i_bus,
	i_byp_addr,
	i_byp_en,
	o_s0_pos,
	o_s0_neg,
	o_s0_zero,
	o_s0_nzero
);

	parameter WIDTH    = 64;
	parameter DEPTH    = 64;
	parameter LOGDEPTH = 6;


	input wire clk;
	input wire rst;
	input wire [LOGDEPTH-1:0] i_j_addr;
	input wire [LOGDEPTH-1:0] i_k_addr;
	input wire [LOGDEPTH-1:0] i_i_addr;
	input wire [LOGDEPTH-1:0] i_ex_addr;
	output wire [WIDTH-1:0] o_ex_data;
	output wire [WIDTH-1:0] o_j_data;
	output wire [WIDTH-1:0] o_k_data;
	output wire [WIDTH-1:0] o_i_data;
	input wire [LOGDEPTH-1:0] i_wr_addr;
	input wire [WIDTH-1:0] i_wr_data;
	input wire i_wr_en;
	//A result on its way into a register goes to those who read the register in
	//the same clock.  It comes on i_bus, and which register it is for comes apart
	//from i_wr_addr and i_wr_en, which carry the exchange sequence as well: nothing
	//reads a register during an exchange, and the way from here into the units is
	//the longest there is.
	input wire [WIDTH-1:0] i_bus;
	input wire [LOGDEPTH-1:0] i_byp_addr;
	input wire i_byp_en;
	output wire o_s0_pos;
	output wire o_s0_neg;
	output wire o_s0_zero;
	output wire o_s0_nzero;


	reg     [WIDTH-1:0] data[DEPTH-1:0];  //the actual registers
	integer             i;
	wire    [     63:0] s0;


	//These signals are used for branching
	assign s0         = data[0];
	assign o_s0_pos   = !s0[63];  //assume 24'b1...=negative, 24'b0...=positive
	assign o_s0_neg   = s0[63];
	assign o_s0_zero  = (s0 == 64'b0);
	assign o_s0_nzero = (s0 != 64'b0);


	//write a register
	always @(posedge clk)
		if (rst) begin
			for (i = 0; i < DEPTH; i = i + 1) data[i] <= 0;
		end else if (i_wr_en) data[i_wr_addr] <= i_wr_data;


	//read registers

	assign o_j_data = (i_j_addr == 3'b0) ? 64'b0 : (((i_j_addr == i_byp_addr) && i_byp_en) ? i_bus : data[i_j_addr]);
	assign o_k_data = (i_k_addr==3'b0) ? (64'b1 << 63) : (((i_k_addr == i_byp_addr) && i_byp_en) ? i_bus : data[i_k_addr]);
	assign o_i_data = ((i_i_addr == i_byp_addr) && i_byp_en) ? i_bus : data[i_i_addr];
	assign o_ex_data = data[i_ex_addr];
endmodule

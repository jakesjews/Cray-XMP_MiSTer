//This is a parameterizable register file


//////////////////////////////////////////////////////////////////
//        Secondary Scalar Register File                        //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block contains one 64 entry, 64-bit register file used
//for the Cray-1A's 'secondary' scalar registers

module t_regfile_hard (
	clk,
	i_jk_addr,
	o_jk_data,
	i_wr_addr,
	i_wr_data,
	i_wr_en
);

	parameter WIDTH    = 64;
	parameter DEPTH    = 64;
	parameter LOGDEPTH = 6;


	input wire clk;
	input wire [LOGDEPTH-1:0] i_jk_addr;
	output wire [WIDTH-1:0] o_jk_data;
	input wire [LOGDEPTH-1:0] i_wr_addr;
	input wire [WIDTH-1:0] i_wr_data;
	input wire i_wr_en;


	//Memory made of logic cells, which is read in the clock it is addressed: a 074
	//has one clock to get its register to the S result bus.  Nothing reads a register
	//in the clock it is written (func_top holds a 074 behind a 075 for that).
	(* ramstyle = "MLAB, no_rw_check" *) reg [WIDTH-1:0] data[DEPTH-1:0];  //the actual registers
	//write a register
	always @(posedge clk) if (i_wr_en) data[i_wr_addr] <= i_wr_data;

	//read registers
	assign o_jk_data = data[i_jk_addr];


endmodule

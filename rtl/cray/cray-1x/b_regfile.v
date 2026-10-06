//This is a parameterizable register file


//////////////////////////////////////////////////////////////////
//        Secondary Address Register File                       //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block contains one 64 entry, 24-bit register file used
//for the Cray-1A's 'secondary' address registers

module b_regfile (
	clk,
	i_jk_addr,
	o_jk_data,
	i_wr_addr,
	i_wr_data,
	i_wr_en,
	i_cur_p,
	i_rtn_jump
);

	parameter WIDTH    = 24;
	parameter DEPTH    = 64;
	parameter LOGDEPTH = 6;


	input wire clk;
	input wire [LOGDEPTH-1:0] i_jk_addr;
	output wire [WIDTH-1:0] o_jk_data;
	input wire [LOGDEPTH-1:0] i_wr_addr;
	input wire [WIDTH-1:0] i_wr_data;
	input wire i_wr_en;
	input wire [WIDTH-1:0] i_cur_p;
	input wire i_rtn_jump;

	//Memory made of logic cells, which is read in the clock it is addressed: a 024
	//has one clock to get its register to the A result bus.  Nothing reads a register
	//in the clock it is written (func_top holds a 024 behind a 025 for that).
	(* ramstyle = "MLAB, no_rw_check" *)reg  [   WIDTH-1:0] data               [DEPTH-1:0];  //the actual registers
	wire [LOGDEPTH-1:0] wr_addr;
	wire                write_enable;
	wire [        23:0] data_to_be_written;


	assign wr_addr            = i_rtn_jump ? {LOGDEPTH{1'b0}} : i_wr_addr;
	assign write_enable       = i_wr_en || i_rtn_jump;
	assign data_to_be_written = i_rtn_jump ? i_cur_p : i_wr_data;

	//write a register
	always @(posedge clk) if (write_enable) data[wr_addr] <= data_to_be_written;

	//read registers
	assign o_jk_data = data[i_jk_addr];

endmodule

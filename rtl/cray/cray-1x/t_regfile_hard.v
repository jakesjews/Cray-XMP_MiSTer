//This is a parameterizable register file


//////////////////////////////////////////////////////////////////
//        Secondary Scalar Register File                        //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block contains one 64 entry, 64-bit register file used
//for the Cray-1A's 'secondary' scalar registers

module t_regfile_hard (clk,
                  i_jk_addr,
                  o_jk_data,
                  i_wr_addr,
                  i_wr_data,
                  i_wr_en);

parameter WIDTH = 64;
parameter DEPTH = 64;
parameter LOGDEPTH = 6;


input  wire                clk;
input  wire [LOGDEPTH-1:0] i_jk_addr;
output reg  [WIDTH-1:0]    o_jk_data;
input  wire [LOGDEPTH-1:0] i_wr_addr;
input  wire [WIDTH-1:0]    i_wr_data;
input  wire                i_wr_en;

wire [WIDTH-1:0] read_data;

reg [WIDTH-1:0] data [DEPTH-1:0];     //the actual registers
/*
hard_v_reg tmem (
	.clka(clk),
	.wea(i_wr_en), 
	.addra(i_wr_addr), 
	.dina(i_wr_data), 
	.clkb(clk),
	.rstb(rst),
	.addrb(i_jk_addr),
	.doutb(o_jk_data)); // Bus [63 : 0] 
*/

//write a register
always@(posedge clk)
   if(i_wr_en)
      data[i_wr_addr] <= i_wr_data;

//read registers
always@(posedge clk)
   o_jk_data <= data[i_jk_addr];


endmodule

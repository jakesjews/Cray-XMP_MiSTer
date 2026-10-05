//////////////////////////////////////////////////////////////////
//        Address Register File                                 //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This block contains one 8 entry, 24-bit register file used
//for the Cray-1A's 8 main address registers

module a_regfile (clk,
                  rst,
                  i_j_addr,
						i_k_addr,
						i_i_addr,
						i_h_addr,
						i_ex_addr,
						o_ex_data,
                  o_j_data,
						o_k_data,
						o_i_data,
						o_h_data,
						o_a0_data,
                  i_wr_addr,
                  i_wr_data,
                  i_wr_en,
						o_a0_pos,
						o_a0_neg,
						o_a0_zero,
						o_a0_nzero);

parameter WIDTH = 64;
parameter DEPTH = 64;
parameter LOGDEPTH = 6;


input  wire                clk;
input  wire                rst;
input  wire [LOGDEPTH-1:0] i_j_addr;
input  wire [LOGDEPTH-1:0] i_k_addr;
input  wire [LOGDEPTH-1:0] i_i_addr;
input  wire [LOGDEPTH-1:0] i_h_addr;
input  wire [LOGDEPTH-1:0] i_ex_addr;
output wire [WIDTH-1:0]    o_ex_data;
output wire [WIDTH-1:0]    o_j_data;
output wire [WIDTH-1:0]    o_k_data;
output wire [WIDTH-1:0]    o_i_data;
output wire [WIDTH-1:0]    o_h_data;
output wire [WIDTH-1:0]    o_a0_data;
input  wire [LOGDEPTH-1:0] i_wr_addr;
input  wire [WIDTH-1:0]    i_wr_data;
input  wire                i_wr_en;
output wire                o_a0_pos;
output wire                o_a0_neg;
output wire                o_a0_zero;
output wire                o_a0_nzero;



reg [WIDTH-1:0] data [DEPTH-1:0];     //the actual registers
integer i;
wire [23:0] a0;

//These signals are used for branching
assign a0 = data[0];
assign o_a0_pos = !a0[23];  //assume 24'b1...=negative, 24'b0...=positive
assign o_a0_neg = a0[23];
assign o_a0_zero = (a0==24'b0);
assign o_a0_nzero= (a0!=24'b0);
assign o_a0_data = a0;

//write a register
always@(posedge clk)
   if(rst)
	   begin
		   for(i=0;i<DEPTH;i=i+1)
	         data[i] <= 0;
		end
   else if(i_wr_en)
      data[i_wr_addr] <= i_wr_data;


//read registers

assign o_j_data = (i_j_addr==3'b0) ? 24'b0 : (((i_j_addr==i_wr_addr) && i_wr_en) ? i_wr_data : data[i_j_addr]);
assign o_k_data = (i_k_addr==3'b0) ? 24'b1 : (((i_k_addr==i_wr_addr) && i_wr_en) ? i_wr_data : data[i_k_addr]);
assign o_i_data = ((i_i_addr==i_wr_addr) && i_wr_en) ? i_wr_data : data[i_i_addr];
assign o_h_data = (i_h_addr==3'b0) ? 24'b0 : (((i_h_addr==i_wr_addr) && i_wr_en) ? i_wr_data : data[i_h_addr]);
assign o_ex_data= data[i_ex_addr];
endmodule

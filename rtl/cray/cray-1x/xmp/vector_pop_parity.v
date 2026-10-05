//******************************************
//   Vector Population Count / Parity Unit
//******************************************
//


module vector_pop_parity(clk,rst, i_start, i_k, i_j,i_v0,i_v1,i_v2,i_v3,i_v4,i_v5,i_v6,i_v7,o_result, o_busy, i_vl);

    input wire        clk;
	 input wire        rst;
    input wire        i_start;
    input wire [63:0] i_v0;
    input wire [63:0] i_v1;
    input wire [63:0] i_v2;
    input wire [63:0] i_v3;
    input wire [63:0] i_v4;
    input wire [63:0] i_v5;
    input wire [63:0] i_v6;
    input wire [63:0] i_v7;	 
    input wire [2:0]  i_k;
	 input wire [2:0]  i_j;
    output wire [63:0] o_result;
	 output wire o_busy;
	 input  wire [6:0] i_vl;

    reg [6:0] reservation_time;
    wire [63:0] vj0;
    reg [63:0]  vj1, vj2, vj3;
    reg [2:0] temp_k;
	 reg [2:0] cur_j;
    reg [6:0] p_tmp0, p_tmp1, p_tmp2, p_tmp3;

localparam POP_COUNT = 3'b001, 
           PARITY    = 3'b010;

reg [63:0] v_rd_data [7:0];

always@(posedge clk)
   begin
      v_rd_data[0] <= i_v0;
      v_rd_data[1] <= i_v1;
      v_rd_data[2] <= i_v2;
      v_rd_data[3] <= i_v3;
      v_rd_data[4] <= i_v4;
      v_rd_data[5] <= i_v5;
      v_rd_data[6] <= i_v6;
      v_rd_data[7] <= i_v7;
   end
    //A pop-count and leading-zero count can never be issued on back-to-back cycles, so this should never give us a problem
    assign o_result[63:0] = (temp_k==POP_COUNT) ? {57'b0,p_tmp3[6:0]} : {63'b0,p_tmp3[0]}; 

assign vj0[63:0] = v_rd_data[cur_j];

    always@(posedge clk)
    begin               //just pipelining the state so it carries through
        //vj0[63:0] <= v_rd_data[cur_j];
        vj1[63:0] <= vj0[63:0];
        vj2[63:0] <= vj1[63:0];
	     vj3[63:0] <= vj2[63:0];
        if(i_start)
		  begin
           temp_k <= i_k;
			  cur_j  <= i_j;
		  end
    end

//Set up the reservation system so that we report when we're busy
assign o_busy = (reservation_time != 7'b0);
always@(posedge clk)
   if(rst)
	   reservation_time <= 7'b0;
   else if(i_start)
	   reservation_time <= (i_vl > 7'b0000100) ? (i_vl + 7'b0000100) : (7'b0000101 + 7'b0000100);
	else if(reservation_time != 7'b0)
	   reservation_time <= reservation_time - 7'b1;
		
		
    //calculate population count
    always@(posedge clk)
    begin
        p_tmp0 <= vj0[0] + vj0[1] + vj0[2] + vj0[3] + vj0[4] + vj0[5] + vj0[6] + vj0[7] + vj0[8] + vj0[9] + vj0[10] + vj0[11] + vj0[12] + vj0[13] + vj0[14] + vj0[15];
        p_tmp1 <= p_tmp0[6:0] + vj1[16] + vj1[17] + vj1[18] + vj1[19] + vj1[20] + vj1[21] + vj1[22] + vj1[23] + vj1[24] + vj1[25] + vj1[26] + vj1[27] + vj1[28] + vj1[29] + vj1[30] + vj1[31];
        p_tmp2 <= p_tmp1[6:0] + vj2[32] + vj2[33] + vj2[34] + vj2[35] + vj2[36] + vj2[37] + vj2[38] + vj2[39] + vj2[40] + vj2[41] + vj2[42] + vj2[43] + vj2[44] + vj2[45] + vj2[46] + vj2[47];
        p_tmp3 <= p_tmp2[6:0] + vj3[48] + vj3[49] + vj3[50] + vj3[51] + vj3[52] + vj3[53] + vj3[54] + vj3[55] + vj3[56] + vj3[57] + vj3[58] + vj3[59] + vj3[60] + vj3[61] + vj3[62] + vj3[63];  
    end


endmodule

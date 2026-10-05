//Simple channel interface to model the 6MB/s Cray channels.
//Converts an incoming stream of 16-bit data into 64-bit words

/*
output wire                    p_channel_srdy, // '1' means data, data_valid and disconnect are valid
input  wire                    p_channel_drdy, // '1' means receiver is ready to accept data. In every clock-cycle where both srdy and drdy are '1' a data-transfer happens
output wire                    p_channel_disconnect, // '1' means last cycle of the transfer.
output wire                    p_channel_data_valid, // '1' means data signals contain valid information
output wire [15:0]             p_channel_data, // 16-bit data
*/

module slow_ch_rx(input  wire        rst,
						input  wire        clk,
						//External Channel Interface
						input  wire [15:0] p_channel_data,
						input  wire        p_channel_srdy,
						output wire        p_channel_drdy,
						input  wire        p_channel_disconnect,
						input  wire        p_channel_data_valid,
						//Internal Interface
						output wire        o_full,
						output reg [63:0]  o_data,
						input  wire        i_rd,
						output reg         o_int);
						
reg [1:0] ptr;
reg state;

//Track if we're full or not
always@(posedge clk)
   if(rst)
	   state <= 1'b0;
	else case(state)
		1'b0:if((ptr[1:0]==2'b11) && p_channel_srdy && p_channel_data_valid)
				state <= 1'b1;
		1'b1:if(i_rd)
				state <= 1'b0;
			endcase
//Inform DMA engine we can't handle more data until we've sent our packet
assign o_full = state;
assign p_channel_drdy = !state;

//choose what data to send
always@(posedge clk)
   begin
	   if(p_channel_srdy && p_channel_data_valid && !o_full)
	   case(ptr[1:0])
/*			2'b00:o_data[15:0]  <= p_channel_data[15:0];
			2'b01:o_data[31:16] <= p_channel_data[15:0];
			2'b10:o_data[47:32] <= p_channel_data[15:0];
			2'b11:o_data[63:48] <= p_channel_data[15:0];*/
			2'b11:o_data[15:0]  <= p_channel_data[15:0];
			2'b10:o_data[31:16] <= p_channel_data[15:0];
			2'b01:o_data[47:32] <= p_channel_data[15:0];
			2'b00:o_data[63:48] <= p_channel_data[15:0];
		endcase
	end

always@(posedge clk)
	o_int <= p_channel_disconnect;
	
always@(posedge clk)
   ptr[1:0] <= rst ? 2'b00 : (ptr[1:0] + (!o_full && p_channel_srdy && p_channel_data_valid));
endmodule
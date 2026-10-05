//Simple module to take in data 64 bits at a time
//and send it back out on a 16-bit port

/*
output wire                    p_channel_srdy, // '1' means data, data_valid and disconnect are valid
input  wire                    p_channel_drdy, // '1' means receiver is ready to accept data. In every clock-cycle where both srdy and drdy are '1' a data-transfer happens
output wire                    p_channel_disconnect, // '1' means last cycle of the transfer.
output wire                    p_channel_data_valid, // '1' means data signals contain valid information
output wire [15:0]             p_channel_data, // 16-bit data



*/


module slow_ch_tx(      input  wire rst,
						input  wire clk,
						//External interface signals
						output reg [15:0] p_channel_data,
						output wire       p_channel_srdy,
						output wire       p_channel_disconnect,
						output wire       p_channel_data_valid,
						input  wire       p_channel_drdy,
						//Internal Interface signals
						output wire        o_full,
						input  wire [63:0] i_data,
						input  wire        i_wr,
						input  wire        i_last);
						
reg [1:0]  ptr;
reg        state;
reg [63:0] data;
reg        last;

//Track if we're busy or not
always@(posedge clk)
   begin
      if(rst)
		   state <= 1'b0;
	   else case(state)
			1'b0:if(i_wr)
					state <= 1'b1;
			1'b1:if((ptr[1:0]==2'b11) && p_channel_drdy)
					state <= 1'b0;
			endcase
	end

//Grab the new data and buffer it
always@(posedge clk)
   if((state==1'b0) && i_wr)
	    data[63:0] <= i_data;
		 
//Inform DMA engine we can't handle more data until we've sent our packet
assign o_full = state;
assign p_channel_srdy = state;
assign p_channel_data_valid = state;

always@(posedge clk)
	if(rst)
	   last <= 1'b0;
   else if((state==1'b0) && i_wr)
	   last <= i_last;
	else
	   last <= last;
	
assign p_channel_disconnect = last && p_channel_drdy && (ptr[1:0]==2'b11);        //FIXME: I'm pretty sure this needs to actually be driven based on the end of the DMA operation.
//choose what data to send
always@*
   begin
	   case(ptr[1:0])
/*			2'b00:p_channel_data[15:0] = data[15:0];
			2'b01:p_channel_data[15:0] = data[31:16];
			2'b10:p_channel_data[15:0] = data[47:32];
			2'b11:p_channel_data[15:0] = data[63:48];*/
			2'b11:p_channel_data[15:0] = data[15:0];
			2'b10:p_channel_data[15:0] = data[31:16];
			2'b01:p_channel_data[15:0] = data[47:32];
			2'b00:p_channel_data[15:0] = data[63:48];
		endcase
	end

always@(posedge clk)
   ptr[1:0] <= rst ? 2'b00 : (ptr[1:0] + (state && p_channel_drdy));
	
endmodule
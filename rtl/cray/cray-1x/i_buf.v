//////////////////////////////////////////////////////////////////
//        Cray Instruction Buffer                               //
//        Author: Christopher Fenton                            //
//        Date:  8/8/23                                         //
//////////////////////////////////////////////////////////////////
//
//This is the block of instruction buffers. Each of the four
//buffers can hold 128 16-bit parcels: 32 words, as on the X-MP
//(CSM-0111000 page 3-5). Instruction buffers load 64-bit words
//from memory, 16 at a time: first the half of the block with the
//parcel that is wanted, which can be run as soon as it is in,
//then the other half.

module i_buf (
	clk,
	rst,
	i_p_addr,
	i_jump,
	i_jump_addr,
	o_nip_nxt,
	o_nip_vld,
	o_mem_ce,
	o_mem_burst,
	o_mem_addr,
	i_mem_data,
	i_mem_vld,
	i_hold,
	o_busy
);

	input wire clk;
	input wire rst;
	input wire [23:0] i_p_addr;
	input wire i_jump;  // a branch is taken this clock: i_p_addr is its address in the next
	input wire [23:0] i_jump_addr;
	output reg [15:0] o_nip_nxt;
	output wire o_nip_vld;
	//64-bit wide memory interface
	output wire o_mem_ce;
	output wire o_mem_burst;  // the request is for the 16 words of half a buffer
	output wire [21:0] o_mem_addr;
	input wire [63:0] i_mem_data;
	input wire i_mem_vld;
	input wire i_hold;  // do not start a new fill
	output wire o_busy;  // a fill is in progress


	reg [16:0] buf_delay;
	reg [16:0] cur_buf;
	//instruction buffers
	(* ramstyle = "MLAB, no_rw_check" *)reg [63:0] buf0      [31:0];
	(* ramstyle = "MLAB, no_rw_check" *)reg [63:0] buf1      [31:0];
	(* ramstyle = "MLAB, no_rw_check" *)reg [63:0] buf2      [31:0];
	(* ramstyle = "MLAB, no_rw_check" *)reg [63:0] buf3      [31:0];

	wire [63:0] cur_buf0_word, cur_buf1_word, cur_buf2_word, cur_buf3_word;

	//beginning address registers: the block a buffer holds or is being filled with
	reg [16:0] beg_addr0, beg_addr1, beg_addr2, beg_addr3;
	//A buffer answers for a parcel only while it holds the half of the block the
	//parcel is in: not after reset, and not while that half is still to come.  The
	//words of a new block overwrite the old ones as they arrive, and P can come back
	//to the old block before the fill is over (a jump taken after a fetch ahead).
	//Bit 2n is the first half of buffer n, bit 2n+1 the second.
	reg  [7:0] buf_vld;
	wire       fill_start;

	//Buffers are replaced in turn, by the 2-bit buffer counter
	reg [1:0] buf_cnt;
	reg [3:0] mem_cnt;  // word of the half that arrives next

	reg buf_state;  //state register
	reg fill_half;  //the half being fetched
	reg fill_last;  //it is the second of the two


	wire buf0_match, buf1_match, buf2_match, buf3_match;
	wire no_match;
	wire half_complete;
	wire load_complete;

	reg [16:0] tmp_addr;

	localparam IDLE = 1'b0, RX = 1'b1;



	//This bit is kind of weird. There is a 2-cycle delay for some reason
	//when the selected buffer changes. The incoming address is the address
	//being sent out on the "nip_nxt" port, though, so I'm just implementing
	//this by delaying the 'buffer address' lines by  2 cycles, and then
	//ANDing (i_buf_addr==cur_buf_addr) with the o_nip_vld signal to get the
	//expected behavior
	//
	//That is the X-MP's 2-CP delay when a program runs on from one buffer into
	//another.  A branch that is taken costs two clocks as well, but the same two
	//whichever buffer its address is in (CSM-0111000 section 5: 5 CPs with the
	//branch address in a buffer): the buffers are asked for it at once, and answer
	//two clocks later.
	reg [1:0] jump_wait;
	always @(posedge clk)
		if (i_jump) begin
			buf_delay <= i_jump_addr[23:7];
			cur_buf   <= i_jump_addr[23:7];
		end else begin
			buf_delay <= i_p_addr[23:7];
			cur_buf   <= buf_delay;
		end

	always @(posedge clk)
		if (rst) jump_wait <= 2'd0;
		else if (i_jump) jump_wait <= 2'd2;
		else if (jump_wait != 2'd0) jump_wait <= jump_wait - 2'd1;

	wire same_block = (cur_buf == i_p_addr[23:7]);

	//Enable memory if we're trying to fill a buffer, and provide the correct address
	assign o_mem_ce    = (buf_state == RX);
	//A half of the block is a 16-word line and is fetched as one 16-word burst: the
	//address stays at the start of the line and i_mem_vld pulses once per word.  The
	//request stays up from one half to the other; the address changes behind the
	//last word of the first.
	assign o_mem_addr  = {tmp_addr, fill_half, 4'b0};
	assign o_mem_burst = 1'b1;

	//tell the main block if the next instruction parcel is valid or not
	assign o_nip_vld = (buf0_match || buf1_match || buf2_match || buf3_match) && same_block && (jump_wait == 2'd0);

	//Let's check if the incoming address matches any beginning addresses
	assign buf0_match = buf_vld[{2'd0, i_p_addr[6]}] && (cur_buf == beg_addr0);
	assign buf1_match = buf_vld[{2'd1, i_p_addr[6]}] && (cur_buf == beg_addr1);
	assign buf2_match = buf_vld[{2'd2, i_p_addr[6]}] && (cur_buf == beg_addr2);
	assign buf3_match = buf_vld[{2'd3, i_p_addr[6]}] && (cur_buf == beg_addr3);

	assign no_match = ~(buf0_match || buf1_match || buf2_match || buf3_match);


	//count the words of a half as they arrive
	always @(posedge clk)
		if (rst || (buf_state == IDLE)) mem_cnt <= 4'b0;
		else if (i_mem_vld) mem_cnt <= mem_cnt + 4'b1;

	//Fill in the correct buffer as we're loading from memory
	always @(posedge clk) if ((buf_cnt == 2'b00) && i_mem_vld) buf0[{fill_half, mem_cnt}] <= i_mem_data;

	always @(posedge clk) if ((buf_cnt == 2'b01) && i_mem_vld) buf1[{fill_half, mem_cnt}] <= i_mem_data;

	always @(posedge clk) if ((buf_cnt == 2'b10) && i_mem_vld) buf2[{fill_half, mem_cnt}] <= i_mem_data;

	always @(posedge clk) if ((buf_cnt == 2'b11) && i_mem_vld) buf3[{fill_half, mem_cnt}] <= i_mem_data;

	//detect when a half is in, and when both are
	assign half_complete = (buf_state == RX) && i_mem_vld && (mem_cnt == 4'd15);
	assign load_complete = half_complete && fill_last;

	//load the 'beginning address' register of a buffer when its fill starts; its
	//halves are not valid then
	always @(posedge clk) if ((buf_cnt == 2'b00) && fill_start) beg_addr0 <= i_p_addr[23:7];

	always @(posedge clk) if ((buf_cnt == 2'b01) && fill_start) beg_addr1 <= i_p_addr[23:7];

	always @(posedge clk) if ((buf_cnt == 2'b10) && fill_start) beg_addr2 <= i_p_addr[23:7];

	always @(posedge clk) if ((buf_cnt == 2'b11) && fill_start) beg_addr3 <= i_p_addr[23:7];

	always @(posedge clk)
		if (rst) buf_vld <= 8'b0;
		else if (fill_start) begin
			buf_vld[{buf_cnt, 1'b0}] <= 1'b0;
			buf_vld[{buf_cnt, 1'b1}] <= 1'b0;
		end else if (half_complete) buf_vld[{buf_cnt, fill_half}] <= 1'b1;



	//Now that the load is finished, increment the buffer counter every time we fill a buffer
	always @(posedge clk)
		if (rst) buf_cnt <= 2'b00;
		else if (load_complete) buf_cnt <= buf_cnt + 2'b1;



	//now lets select some data
	assign cur_buf0_word = buf0[i_p_addr[6:2]];
	assign cur_buf1_word = buf1[i_p_addr[6:2]];
	assign cur_buf2_word = buf2[i_p_addr[6:2]];
	assign cur_buf3_word = buf3[i_p_addr[6:2]];

	//select the correct 16-bit parcel out of the current 64-bit word
	always @* begin
		case ({
			buf0_match, buf1_match, buf2_match, buf3_match
		})
			4'b1000:
			case (i_p_addr[1:0])
				2'b11: o_nip_nxt = cur_buf0_word[15:0];
				2'b10: o_nip_nxt = cur_buf0_word[31:16];
				2'b01: o_nip_nxt = cur_buf0_word[47:32];
				2'b00: o_nip_nxt = cur_buf0_word[63:48];
			endcase

			4'b0100:
			case (i_p_addr[1:0])
				2'b11: o_nip_nxt = cur_buf1_word[15:0];
				2'b10: o_nip_nxt = cur_buf1_word[31:16];
				2'b01: o_nip_nxt = cur_buf1_word[47:32];
				2'b00: o_nip_nxt = cur_buf1_word[63:48];
			endcase

			4'b0010:
			case (i_p_addr[1:0])
				2'b11: o_nip_nxt = cur_buf2_word[15:0];
				2'b10: o_nip_nxt = cur_buf2_word[31:16];
				2'b01: o_nip_nxt = cur_buf2_word[47:32];
				2'b00: o_nip_nxt = cur_buf2_word[63:48];
			endcase

			4'b0001:
			case (i_p_addr[1:0])
				2'b11: o_nip_nxt = cur_buf3_word[15:0];
				2'b10: o_nip_nxt = cur_buf3_word[31:16];
				2'b01: o_nip_nxt = cur_buf3_word[47:32];
				2'b00: o_nip_nxt = cur_buf3_word[63:48];
			endcase

			default: o_nip_nxt = 16'b0;
		endcase
	end

	//State machine to retrieve the two 16-word halves of a block from memory
	always @(posedge clk)
		if (rst) buf_state <= IDLE;
		else
			case (buf_state)
				IDLE: if (fill_start) buf_state <= RX;
				RX:   if (load_complete) buf_state <= IDLE;
			endcase

	//A fill starts for the block P has pointed at for two clocks: while P has just
	//moved, what is known about a match is still about the block it left.
	assign fill_start = (buf_state == IDLE) && no_match && same_block && !i_hold;

	always @(posedge clk)
		if (fill_start) begin
			tmp_addr  <= i_p_addr[23:7];
			fill_half <= i_p_addr[6];
			fill_last <= 1'b0;
		end else if (half_complete) begin
			fill_half <= !fill_half;
			fill_last <= 1'b1;
		end



	assign o_busy = (buf_state == RX);

endmodule

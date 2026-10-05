//******************************************
//       Memory Functional Unit
//******************************************
//
// Sequences every memory reference an instruction makes:
//
//   10h-13h   one word between memory and an A or S register, address (Ah) + jkm
//   034-037   (Ai) words between memory and the B or T registers, starting at
//             address (A0) and register jk, registers wrapping from 77 to 00
//   176, 177  VL words between memory and a V register, starting at address
//             (A0) and stepping by (Ak)
//
// A memory instruction stays the current instruction until its last word is
// done, and issues in the DONE clock.  Each word is its own request to memory,
// and nothing here assumes how long memory takes: a read word is handed to its
// register the clock after the acknowledge, and a write word is requested only
// once its register has been read.
//
// All addresses are relative to the data base address and are checked against
// the limit address (manual 3-43).  Addresses are 24 bits, as the A registers
// are.  A reference outside the field is not made: a store is dropped, a load
// delivers whatever the unit last read, and o_range_err pulses so the operand
// range flag can be set.

module mem_fu (
	clk,
	rst,
	i_cip,
	i_cip_vld,
	i_lip,
	i_lip_vld,
	i_vector_length,
	i_vstart,
	i_data_base_addr,
	i_data_limit_addr,
	//interface to A rf
	i_a0_data,
	i_ai_data,
	i_ak_data,
	i_ah_data,
	i_a_res_mask,
	//interface to S rf
	i_si_data,
	i_s_res_mask,
	//interface to V regs
	i_v0_data,
	i_v1_data,
	i_v2_data,
	i_v3_data,
	i_v4_data,
	i_v5_data,
	i_v6_data,
	i_v7_data,
	o_v_wr,
	o_v_wr_idx,
	o_v_rd_idx,
	//interface to B rf
	o_b_rd_addr,
	i_b_rd_data,
	o_b_wr_addr,
	o_b_wr_en,
	//interface to T rf
	o_t_rd_addr,
	i_t_rd_data,
	o_t_wr_addr,
	o_t_wr_en,
	//memory interface
	o_mem_data,
	o_mem_addr,
	i_mem_rd_data,
	o_mem_wr_data,
	o_mem_wr_en,
	o_mem_ce,
	i_mem_ack,
	o_mem_type,
	o_mem_issue,
	o_mem_busy,
	o_range_err
);

	parameter V_READ_WAIT = 2;  // clocks from asking a V register for an element to its data

	//system signals
	input wire clk;
	input wire rst;
	input wire [15:0] i_cip;  //current instruction parcel
	input wire i_cip_vld;  //and it may start
	input wire [15:0] i_lip;  //lower instruction parcel
	input wire i_lip_vld;
	input wire [6:0] i_vector_length;
	input wire i_vstart;  //the vector scheduler starts a 176 or 177
	input wire [23:0] i_data_base_addr;
	input wire [23:0] i_data_limit_addr;
	//interface to registers
	input wire [23:0] i_a0_data;
	input wire [23:0] i_ai_data;
	input wire [23:0] i_ak_data;
	input wire [23:0] i_ah_data;
	input wire [7:0] i_a_res_mask;
	input wire [63:0] i_si_data;
	input wire [7:0] i_s_res_mask;
	input wire [63:0] i_v0_data;
	input wire [63:0] i_v1_data;
	input wire [63:0] i_v2_data;
	input wire [63:0] i_v3_data;
	input wire [63:0] i_v4_data;
	input wire [63:0] i_v5_data;
	input wire [63:0] i_v6_data;
	input wire [63:0] i_v7_data;
	output reg o_v_wr;  //176: o_mem_data holds element o_v_wr_idx
	output wire [5:0] o_v_wr_idx;
	output wire [5:0] o_v_rd_idx;  //177: the element about to be stored
	output wire [5:0] o_b_rd_addr;
	input wire [23:0] i_b_rd_data;
	output wire [5:0] o_b_wr_addr;
	output reg o_b_wr_en;
	output wire [5:0] o_t_rd_addr;
	input wire [63:0] i_t_rd_data;
	output wire [5:0] o_t_wr_addr;
	output reg o_t_wr_en;
	output reg [63:0] o_mem_data;  //word read from memory, for the registers
	//interface to memory
	output wire [21:0] o_mem_addr;
	input wire [63:0] i_mem_rd_data;
	output reg [63:0] o_mem_wr_data;
	output wire o_mem_wr_en;
	output wire o_mem_ce;
	input wire i_mem_ack;
	//instruction issue
	output wire o_mem_type;
	output wire o_mem_issue;
	output wire o_mem_busy;
	output reg o_range_err;  //a reference outside the field was dropped

	localparam IDLE = 3'd0, RD = 3'd1,  // read request outstanding
	RD_PUT = 3'd2,  // hand the word to its register
	WR_PREP = 3'd3,  // wait for the source register to be read
	WR = 3'd4,  // write request outstanding
	DONE = 3'd5;

	reg [ 2:0] state;
	reg [23:0] address;
	reg [23:0] stride;
	reg [ 6:0] remaining;
	reg [ 5:0] reg_idx;  // B or T register in hand
	reg [ 3:0] wait_cnt;
	reg [ 5:0] put_idx;  // B or T register a word just read goes to

	//-----------------------------------------------------------------
	// Decode
	//-----------------------------------------------------------------
	wire [ 6:0] op = i_cip[15:9];
	wire        b_t_type = (i_cip[15:11] == 5'b00111);  //034-037, 1 parcel
	wire        a_s_type = (i_cip[15:14] == 2'b10);  //100-137, 2 parcels
	wire        v_type = (op == 7'o176) || (op == 7'o177);  //1 parcel
	wire [23:0] jkm = {{2{i_cip[5]}}, i_cip[5:0], i_lip[15:0]};  //signed displacement

	wire is_read = b_t_type ? !i_cip[9] : a_s_type ? !i_cip[12] : (op == 7'o176);
	wire to_b = (op == 7'o034);
	wire to_t = (op == 7'o036);
	wire from_b = (op == 7'o035);
	wire from_t = (op == 7'o037);

	//The operation count of a vector instruction: VL of 0 means 64 (manual 4-10)
	wire [6:0] vl_count = (i_vector_length[5:0] == 6'd0) ? 7'd64 : {1'b0, i_vector_length[5:0]};

	reg [23:0] start_addr;
	reg [23:0] start_stride;
	reg [ 6:0] start_count;
	reg        reg_conflict;

	always @*
		if (b_t_type) begin  //(Ai) words from address (A0)
			start_addr   = i_a0_data;
			start_stride = 24'd1;
			start_count  = i_ai_data[6:0];
			reg_conflict = i_a_res_mask[0] || i_a_res_mask[i_cip[8:6]];
		end else if (a_s_type) begin  //one word at (Ah) + jkm
			start_addr = i_ah_data + jkm;
			start_stride = 24'd0;
			start_count = 7'd1;
			//a load must not pass a result still on its way to the same register file
			reg_conflict = !i_cip[12] ? (i_cip[13] ? (|{i_a_res_mask,i_s_res_mask}) : (|i_a_res_mask))
								   : (i_cip[13] ? (i_a_res_mask[i_cip[11:9]] || i_s_res_mask[i_cip[8:6]])
												: (i_a_res_mask[i_cip[11:9]] || i_a_res_mask[i_cip[8:6]]));
		end else begin  //VL words from (A0), stepping by (Ak)
			start_addr   = i_a0_data;
			start_stride = i_ak_data;
			start_count  = vl_count;
			reg_conflict = i_a_res_mask[0] || i_a_res_mask[i_cip[2:0]];
		end

	assign o_mem_type = b_t_type || a_s_type || v_type;

	wire start = (state==IDLE) &&
			 ((b_t_type && i_cip_vld && !reg_conflict) ||
			  (a_s_type && i_cip_vld && i_lip_vld && !reg_conflict) ||
			  (v_type   && i_vstart));

	//-----------------------------------------------------------------
	// Sequencer
	//-----------------------------------------------------------------
	wire last = (remaining == 7'd1);

	always @(posedge clk) begin
		o_b_wr_en   <= 1'b0;
		o_t_wr_en   <= 1'b0;
		o_v_wr      <= 1'b0;
		o_range_err <= 1'b0;

		if (rst) state <= IDLE;
		else
			case (state)
				IDLE:
				if (start) begin
					address   <= start_addr;
					stride    <= start_stride;
					remaining <= start_count;
					reg_idx   <= v_type ? 6'd0 : i_cip[5:0];
					wait_cnt  <= from_b || from_t ? 4'd1 : (v_type ? V_READ_WAIT[3:0] : 4'd0);
					state     <= (start_count == 7'd0) ? DONE : (is_read ? RD : WR_PREP);
				end

				RD:
				if (out_of_field) begin
					o_range_err <= 1'b1;
					state       <= RD_PUT;
				end else if (i_mem_ack) state <= RD_PUT;

				//o_mem_data now holds the word
				RD_PUT: begin
					o_b_wr_en <= to_b;
					o_t_wr_en <= to_t;
					o_v_wr    <= v_type;
					put_idx   <= reg_idx;
					reg_idx   <= reg_idx + 6'd1;
					remaining <= remaining - 7'd1;
					address   <= address + stride;
					state     <= last ? DONE : RD;
				end

				WR_PREP:
				if (wait_cnt == 4'd0) state <= WR;
				else wait_cnt <= wait_cnt - 4'd1;

				WR:
				if (i_mem_ack || out_of_field) begin
					o_range_err <= out_of_field;
					remaining   <= remaining - 7'd1;
					address     <= address + stride;
					reg_idx     <= reg_idx + 6'd1;
					wait_cnt    <= from_b || from_t ? 4'd1 : V_READ_WAIT[3:0];
					state       <= last ? DONE : WR_PREP;
				end

				DONE: state <= IDLE;

				default: state <= IDLE;
			endcase
	end

	assign o_v_wr_idx  = put_idx;
	assign o_v_rd_idx  = reg_idx;
	assign o_b_wr_addr = put_idx;
	assign o_t_wr_addr = put_idx;
	assign o_b_rd_addr = reg_idx;
	assign o_t_rd_addr = reg_idx;

	always @(posedge clk) if (i_mem_ack) o_mem_data <= i_mem_rd_data;

	//The word to store.  The source register is not changing while this unit runs.
	always @*
		if (from_b) o_mem_wr_data = {40'b0, i_b_rd_data};
		else if (from_t) o_mem_wr_data = i_t_rd_data;
		else if (a_s_type) o_mem_wr_data = i_cip[13] ? i_si_data : {40'b0, i_ai_data};
		else
			case (i_cip[5:3])
				3'd0: o_mem_wr_data = i_v0_data;
				3'd1: o_mem_wr_data = i_v1_data;
				3'd2: o_mem_wr_data = i_v2_data;
				3'd3: o_mem_wr_data = i_v3_data;
				3'd4: o_mem_wr_data = i_v4_data;
				3'd5: o_mem_wr_data = i_v5_data;
				3'd6: o_mem_wr_data = i_v6_data;
				3'd7: o_mem_wr_data = i_v7_data;
			endcase

	//-----------------------------------------------------------------
	// Memory port and issue
	//-----------------------------------------------------------------
	// The field: absolute address = address + base, valid below the limit and below
	// the top of memory (one million words).
	reg [21:0] base;
	reg [20:0] limit;
	always @(posedge clk) begin
		base  <= i_data_base_addr[21:0];
		limit <= (|i_data_limit_addr[23:20]) ? 21'h100000 : {1'b0, i_data_limit_addr[19:0]};
	end

	wire [24:0] absolute = {1'b0, address} + {3'b0, base};
	wire        out_of_field = (absolute >= {4'b0, limit});

	assign o_mem_addr  = absolute[21:0];
	assign o_mem_ce    = ((state == RD) || (state == WR)) && !out_of_field;
	assign o_mem_wr_en = (state == WR) && !out_of_field;

	assign o_mem_issue = (state == DONE);
	assign o_mem_busy  = (state != IDLE);

endmodule

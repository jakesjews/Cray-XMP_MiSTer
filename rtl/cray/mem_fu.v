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
// A scalar reference or a block transfer stays the current instruction until
// its last word is done, and issues in the DONE clock.  A vector transfer
// issues three clocks after it starts and goes on in the background while
// other instructions issue, as on the real machine (manual 4-70); the V
// register stays reserved and other memory instructions wait.  The exception
// is a vector transfer whose first or last address is outside the field: it
// stays the current instruction to its end, so that the range error
// interrupt is taken right behind it.  Nothing here assumes how long memory
// takes: a read word is handed to its register the clock after the
// acknowledge, and a write word is requested only once its register has been
// read.  A store works ahead: it reads the next word from its register while
// the one before is on its way, and hands it to the memory multiplexer as
// soon as that takes it (seq and take), so the words of a block or vector
// store follow each other closely.
//
// A word is normally its own request to memory.  A vector load stepping by a
// small positive amount instead reads whole 16-word lines in one burst when
// three or more of its elements lie in the line, and hands the elements to
// the V register as their words go by.  That is several times faster on
// memory with a long latency.  The line SINGLE_LINE is never read that way,
// because reading a word there can have an effect.
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
	o_mem_burst,
	o_mem_seq,
	i_mem_take,
	i_mem_ack,
	o_mem_type,
	o_mem_issue,
	i_issue,
	o_v_num,
	o_v_store,
	o_mem_busy,
	o_range_err
);

	parameter        V_READ_WAIT = 2;          // clocks from asking a V register for an element to its data
	parameter [17:0] SINGLE_LINE = 18'h0FFFF;  // a 16-word line never read as a burst: the core's I/O page

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
	output wire o_mem_burst;  //the request is a 16-word read of the line at o_mem_addr
	output wire o_mem_seq;  //a store: the next word is presented as soon as one is taken
	input wire i_mem_take;  //the word presented is being taken this clock
	input wire i_mem_ack;
	//instruction issue
	output wire o_mem_type;
	output wire o_mem_issue;
	input wire i_issue;  //the current instruction issues this clock
	output wire [2:0] o_v_num;  //the V register of the vector transfer under way
	output wire o_v_store;  //and it is being stored
	output wire o_mem_busy;
	output reg o_range_err;  //a reference outside the field was dropped

	localparam IDLE = 3'd0, RD = 3'd1,  // read request outstanding
	RD_PUT = 3'd2,  // hand the word to its register
	WR = 3'd4,  // a store: words are read from the register and written
	DONE = 3'd5, RD_SEL = 3'd6,  // a vector load chooses between one word and a line
	RD_BURST = 3'd7;  // a line is arriving

	reg [ 2:0] state;
	reg [23:0] address;
	reg [23:0] stride;
	reg [ 6:0] remaining;
	reg [ 5:0] reg_idx;  // B or T register in hand
	reg [ 3:0] wait_cnt;
	reg [ 5:0] put_idx;  // B or T register a word just read goes to
	reg [ 3:0] bcnt;  // word of the line that arrives next
	reg [17:0] burst_line;
	reg [63:0] wr_word;  // a store: the word in hand, at `address`
	reg        wr_valid;  // it has not been taken yet
	reg [ 6:0] fetch_left;  // words not yet read from the register

	//-----------------------------------------------------------------
	// Decode
	//-----------------------------------------------------------------
	//The instruction being carried out.  A vector transfer goes on after its
	//instruction has issued, so nothing looks at i_cip once a transfer has started.
	reg  [15:0] cur;
	wire [15:0] ins = (state == IDLE) ? i_cip : cur;

	wire [ 6:0] op = ins[15:9];
	wire        b_t_type = (ins[15:11] == 5'b00111);  //034-037, 1 parcel
	wire        a_s_type = (ins[15:14] == 2'b10);  //100-137, 2 parcels
	wire        v_type = (op == 7'o176) || (op == 7'o177);  //1 parcel
	wire [23:0] jkm = {{2{ins[5]}}, ins[5:0], i_lip[15:0]};  //signed displacement

	wire is_read = b_t_type ? !ins[9] : a_s_type ? !ins[12] : (op == 7'o176);
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
			reg_conflict = i_a_res_mask[0] || i_a_res_mask[ins[8:6]];
		end else if (a_s_type) begin  //one word at (Ah) + jkm
			start_addr = i_ah_data + jkm;
			start_stride = 24'd0;
			start_count = 7'd1;
			//a load must not pass a result still on its way to the same register file
			reg_conflict = !ins[12] ? (ins[13] ? (|{i_a_res_mask,i_s_res_mask}) : (|i_a_res_mask))
								   : (ins[13] ? (i_a_res_mask[ins[11:9]] || i_s_res_mask[ins[8:6]])
												: (i_a_res_mask[ins[11:9]] || i_a_res_mask[ins[8:6]]));
		end else begin  //VL words from (A0), stepping by (Ak)
			start_addr   = i_a0_data;
			start_stride = i_ak_data;
			start_count  = vl_count;
			reg_conflict = i_a_res_mask[0] || i_a_res_mask[ins[2:0]];
		end

	//what the current instruction is, whatever this unit is doing
	assign o_mem_type = (i_cip[15:11] == 5'b00111) || (i_cip[15:14] == 2'b10) || (i_cip[15:10] == 6'b111111);

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
					cur        <= i_cip;
					address    <= start_addr;
					stride     <= start_stride;
					remaining  <= start_count;
					reg_idx    <= v_type ? 6'd0 : ins[5:0];
					wait_cnt   <= src_wait;
					fetch_left <= start_count;
					wr_valid   <= 1'b0;
					state      <= (start_count == 7'd0) ? DONE : (is_read ? (v_type ? RD_SEL : RD) : WR);
				end

				RD_SEL: begin
					bcnt       <= 4'd0;
					burst_line <= absolute[21:4];
					state      <= want_burst ? RD_BURST : RD;
				end

				//the elements in the line go to the register as their words pass
				RD_BURST:
				if (i_mem_ack) begin
					bcnt <= bcnt + 4'd1;
					if (burst_hit) begin
						o_v_wr    <= 1'b1;
						put_idx   <= reg_idx;
						reg_idx   <= reg_idx + 6'd1;
						remaining <= remaining - 7'd1;
						address   <= address + stride;
					end
					if (bcnt == 4'd15) state <= (burst_hit ? last : (remaining == 7'd0)) ? DONE : RD_SEL;
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
					state     <= last ? DONE : (v_type ? RD_SEL : RD);
				end

				//A store.  The register word for reg_idx is ready when wait_cnt
				//reaches zero.  It moves into wr_word when that is free or being
				//taken, and the register is asked for the word after it.  `remaining`
				//counts the words not yet written or dropped.
				WR: begin
					if (wait_cnt != 4'd0) wait_cnt <= wait_cnt - 4'd1;
					if (wr_gone) begin
						address  <= address + stride;
						wr_valid <= 1'b0;
					end
					if ((wait_cnt == 4'd0) && (fetch_left != 7'd0) && (!wr_valid || wr_gone)) begin
						wr_word    <= src_word;
						wr_valid   <= 1'b1;
						reg_idx    <= reg_idx + 6'd1;
						wait_cnt   <= src_wait;
						fetch_left <= fetch_left - 7'd1;
					end
					o_range_err <= wr_skip;
					remaining   <= remaining - wr_ended;
					if ((wr_ended != 7'd0) && (remaining == wr_ended)) state <= DONE;
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

	//The register word for reg_idx, valid src_wait clocks after reg_idx is set.
	//The source register is not changing while this unit runs.
	wire [ 3:0] src_wait = from_b || from_t ? 4'd1 : (v_type ? V_READ_WAIT[3:0] : 4'd0);
	reg  [63:0] src_word;
	always @*
		if (from_b) src_word = {40'b0, i_b_rd_data};
		else if (from_t) src_word = i_t_rd_data;
		else if (a_s_type) src_word = ins[13] ? i_si_data : {40'b0, i_ai_data};
		else
			case (ins[5:3])
				3'd0: src_word = i_v0_data;
				3'd1: src_word = i_v1_data;
				3'd2: src_word = i_v2_data;
				3'd3: src_word = i_v3_data;
				3'd4: src_word = i_v4_data;
				3'd5: src_word = i_v5_data;
				3'd6: src_word = i_v6_data;
				3'd7: src_word = i_v7_data;
			endcase

	//The word being stored is held here, so the registers can move on to the next.
	always @* o_mem_wr_data = wr_word;

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

	// A burst pays off with three elements in the line: the element in hand and
	// two more, the second of them still short of the end of the line.
	wire third_fits = ({1'b0, absolute[3:0]} + {1'b0, stride[2:0], 1'b0}) < 5'd16;
	wire       want_burst = !out_of_field && (stride[23:3] == 21'd0) && (stride[2:0] != 3'd0) &&
		(remaining >= 7'd3) && third_fits && (absolute[21:4] != SINGLE_LINE);
	wire burst_hit = (remaining != 7'd0) && (absolute[21:4] == burst_line) && (absolute[3:0] == bcnt);

	assign o_mem_addr = (state == RD_BURST) ? {burst_line, 4'b0} : absolute[21:0];
	// A store word outside the field is dropped without a request; one inside
	// leaves when the multiplexer takes it.
	wire       wr_skip = (state == WR) && wr_valid && out_of_field;
	wire       wr_gone = (state == WR) && wr_valid && (out_of_field || i_mem_take);
	wire [6:0] wr_ended = {6'd0, i_mem_ack} + {6'd0, wr_skip};  // words finished this clock

	assign o_mem_ce    = ((state == RD) && !out_of_field) || (state == RD_BURST) || o_mem_wr_en;
	assign o_mem_burst = (state == RD_BURST);
	assign o_mem_wr_en = (state == WR) && wr_valid && !out_of_field;
	assign o_mem_seq   = (state == WR);

	//-----------------------------------------------------------------
	// Letting a vector instruction issue while its transfer goes on
	//-----------------------------------------------------------------
	// The addresses of a transfer run evenly from the first to the last, so the
	// whole of it is inside the field when both of those are.
	reg               released;  // the instruction has issued; the transfer goes on behind it
	reg        [ 1:0] age;  // clocks since the start, up to 3
	reg signed [31:0] span;  // from the first address to the last
	reg               fits;  // a vector transfer with both ends in the field

	wire signed [32:0] last_rel = $signed({9'b0, address}) + $signed({span[31], span});
	wire        [32:0] last_abs = $unsigned(last_rel) + {11'b0, base};
	wire               last_in = !last_rel[32] && (last_abs < {12'b0, limit});

	always @(posedge clk)
		if (rst || (state == IDLE)) begin
			released <= 1'b0;
			age      <= 2'd0;
			fits     <= 1'b0;
		end else begin
			if (age != 2'd3) age <= age + 2'd1;
			if (age == 2'd0) span <= $signed({1'b0, remaining} - 8'sd1) * $signed(stride);
			if (age == 2'd1) fits <= v_type && !out_of_field && last_in;
			if (o_mem_issue && i_issue) released <= 1'b1;
		end

	assign o_mem_issue = ((state == DONE) || fits) && !released;
	assign o_mem_busy  = (state != IDLE);
	assign o_v_num     = is_read ? ins[8:6] : ins[5:3];
	assign o_v_store   = (state != IDLE) && v_type && !is_read;

endmodule

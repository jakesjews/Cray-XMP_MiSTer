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
//             (A0) and stepping by (Ak); or, for the gather 176i1k and the
//             scatter 1771jk, at the addresses (A0) + (Vk element)
//
// Every one of them issues without waiting for memory, and the instructions
// behind it go on (CSM-0111000 pages 2-6, 5-40, 5-64, 5-92).
//
// A scalar reference issues as soon as both its parcels are there.  Its
// address is formed and checked against the field in the clock after, and it
// then waits in a queue: scalar references are made in the order they issued.
// The register of a load is reserved until its word has come, which the unit
// tells the A or the S registers a clock ahead (o_sc_a, o_sc_s).  The word of a
// store is taken from its register when the instruction issues.  No scalar
// reference issues while a block or vector transfer is under way ("Port A, B,
// or C busy") or the queue is full.
//
// A block or vector transfer issues in the clock it starts, which is when no
// other is under way and no scalar reference is waiting, and goes on in the
// background.  Its B, T or V registers stay reserved.  The instructions
// behind it wait until the transfer is known to stay inside the field
// (o_unsure): not at all when a quick look at its first address and its
// step says so, two clocks when the last address has to be worked out.  A register
// being loaded can be the operand of an instruction behind the load, and a
// store can be of a register that is still receiving its result: both take
// the elements as they come (chaining, CSM-0111000 page 4-12).  A gather or a
// scatter takes its addresses from a register the same way, one element
// after the other, each as its own request to memory.  The exception
// is a transfer whose first or last address is outside the field, and
// every gather and scatter, whose addresses are not known beforehand: the
// instructions behind it wait to its end, so that the range error interrupt
// is taken right behind it.  Nothing here assumes how long memory
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
	i_lip,
	i_scalar,
	i_block,
	i_vector_length,
	i_vstart,
	i_data_base_addr,
	i_data_limit_addr,
	//interface to A rf
	i_a0_data,
	i_ai_data,
	i_ak_data,
	i_ah_data,
	o_sc_a,
	//interface to S rf
	i_si_data,
	o_sc_s,
	o_sc_num,
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
	o_v_last,
	o_v_wr_idx,
	o_v_rd_idx,
	i_v_avail,
	i_vk_avail,
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
	o_sc_ready,
	o_blk_ready,
	o_mem_issue,
	i_issue,
	o_v_num,
	o_vk_num,
	o_v_reads,
	o_blk_b,
	o_blk_t,
	o_unsure,
	o_sc_pending,
	o_mem_busy,
	o_range_err
);

	parameter         V_READ_WAIT = 2;          // clocks from asking a V register for an element to its data
	// The data field follows CSM-0111000 pages 3-19 to 3-21: only the low 22 bits of
	// an address count, memory has four million words, and a load from outside the
	// field delivers zero.
	// a 16-word line never read as a burst: the core's I/O page at the top of memory
	localparam [17:0] SINGLE_LINE = 18'h3FFFF;

	//system signals
	input wire clk;
	input wire rst;
	input wire [15:0] i_cip;  //current instruction parcel
	input wire [15:0] i_lip;  //lower instruction parcel
	input wire i_scalar;  //the current instruction is a scalar reference
	input wire i_block;  //it is a block transfer
	input wire [6:0] i_vector_length;
	input wire i_vstart;  //the vector scheduler starts a 176 or 177
	input wire [23:0] i_data_base_addr;
	input wire [23:0] i_data_limit_addr;
	//interface to registers
	input wire [23:0] i_a0_data;
	input wire [23:0] i_ai_data;
	input wire [23:0] i_ak_data;
	input wire [21:0] i_ah_data;
	input wire [63:0] i_si_data;
	//a scalar load: its word is in o_mem_data, for A or S register o_sc_num
	output reg o_sc_a;
	output reg o_sc_s;
	output reg [2:0] o_sc_num;
	input wire [63:0] i_v0_data;
	input wire [63:0] i_v1_data;
	input wire [63:0] i_v2_data;
	input wire [63:0] i_v3_data;
	input wire [63:0] i_v4_data;
	input wire [63:0] i_v5_data;
	input wire [63:0] i_v6_data;
	input wire [63:0] i_v7_data;
	output reg o_v_wr;  //176: o_mem_data holds element o_v_wr_idx
	output reg o_v_last;  //176: the last element is delivered with this clock
	output wire [5:0] o_v_wr_idx;
	output wire [5:0] o_v_rd_idx;  //177: the element about to be stored
	input wire i_v_avail;  //and it is in its register
	input wire i_vk_avail;  //176i1k, 1771jk: so is the element of Vk with its address
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
	output wire o_sc_ready;  //a scalar reference may issue
	output wire o_blk_ready;  //a block transfer may
	output wire o_mem_issue;  //a vector transfer has started and its instruction has not issued
	input wire i_issue;  //the current instruction issues this clock
	output wire [2:0] o_v_num;  //the V register of the vector transfer under way
	output wire [2:0] o_vk_num;  //the V register with the addresses of a gather or scatter
	output wire [7:0] o_v_reads;  //the V registers a vector transfer reads, one bit a register
	output wire o_blk_b;  //a block transfer of the B registers is under way
	output wire o_blk_t;  //one of the T registers
	output reg o_unsure;  //a transfer is under way that may leave the field: what is behind it waits
	output wire o_sc_pending;  //a scalar reference has issued and is not done
	output wire o_mem_busy;  //anything is under way
	output wire o_range_err;  //a reference outside the field was dropped

	localparam IDLE = 4'd0, PREP = 4'd3,  // the first address is formed
	RD = 4'd1,  // read request outstanding
	RD_PUT = 4'd2,  // hand the word to its register
	WR = 4'd4,  // a store: words are read from the register and written
	DONE = 4'd5, RD_SEL = 4'd6,  // a vector load chooses between one word and a line
	RD_BURST = 4'd7,  // a line is arriving
	RD_IDX = 4'd8;  // a gather waits for the element of Vk with its next address

	// a scalar reference: none, a read or a write outstanding, zero for a load from outside the field
	localparam S_IDLE = 2'd0, S_RD = 2'd1, S_WR = 2'd2, S_ZERO = 2'd3;
	reg  [1:0] sc_state;
	wire       sc_req = (sc_state == S_RD) || (sc_state == S_WR);

	reg [ 3:0] state;
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
	//What the current instruction asks for.  This is looked at to start only: a
	//vector transfer goes on after its instruction has issued, so a transfer under
	//way goes by what was latched at the start (the r_ registers below).
	wire [15:0] ins = i_cip;
	wire [ 6:0] op = ins[15:9];
	wire        b_t_type = (ins[15:11] == 5'b00111);  //034-037, 1 parcel
	wire        v_type = (op == 7'o176) || (op == 7'o177);  //1 parcel
	wire        gather = (op == 7'o176) && (ins[5:3] == 3'd1);  //176i1k
	wire        scatter = (op == 7'o177) && (ins[8:6] == 3'd1);  //1771jk
	wire [21:0] jkm = {ins[5:0], i_lip[15:0]};  //the displacement of a scalar reference

	wire is_read = b_t_type ? !ins[9] : (op == 7'o176);
	wire to_b = (op == 7'o034);
	wire to_t = (op == 7'o036);
	wire from_b = (op == 7'o035);
	wire from_t = (op == 7'o037);

	//The operation count of a vector instruction: VL of 0 means 64 (manual 4-10)
	wire [6:0] vl_count = (i_vector_length[5:0] == 6'd0) ? 7'd64 : {1'b0, i_vector_length[5:0]};

	//A transfer starts at (A0): (Ai) words one after the other, or VL words stepping
	//by (Ak) or at (A0) + (Vk element).  The registers are read in the clock the
	//instruction issues, when none of them has a result on its way any more.
	wire [23:0] start_addr = i_a0_data;
	wire [23:0] start_stride = b_t_type ? 24'd1 : i_ak_data;
	wire [ 6:0] start_count = b_t_type ? i_ai_data[6:0] : vl_count;

	wire start = (state == IDLE) && ((i_block && i_issue) || (v_type && i_vstart));

	//-----------------------------------------------------------------
	// Sequencer
	//-----------------------------------------------------------------
	wire last = (remaining == 7'd1);

	always @(posedge clk) begin
		o_b_wr_en    <= 1'b0;
		o_t_wr_en    <= 1'b0;
		o_v_wr       <= 1'b0;
		o_v_last     <= 1'b0;
		tr_range_err <= 1'b0;

		if (rst) begin
			state    <= IDLE;
			r_vreads <= 8'b0;
		end else
			case (state)
				//While idle the unit takes in what the current instruction asks for, every
				//clock, and the instruction that starts it leaves what it asked for.  Only
				//the state, and the mark on the V register a store reads, wait for the
				//decision to start; nothing wide does.
				IDLE: begin
					r_v <= v_type;
					r_to_b <= to_b;
					r_to_t <= to_t;
					r_from_b <= from_b;
					r_from_t <= from_t;
					r_vnum <= is_read ? ins[8:6] : ins[5:3];
					r_knum <= ins[2:0];
					r_gather <= gather;
					r_scatter <= scatter;
					r_own <= (ins[8:6] == ins[2:0]);
					gbase <= start_addr;
					r_wait <= src_wait;
					address <= start_addr;
					stride <= start_stride;
					remaining <= start_count;
					reg_idx <= v_type ? 6'd0 : ins[5:0];
					wait_cnt <= src_wait;
					fetch_left <= start_count;
					wr_valid <= 1'b0;
					after <= (start_count == 7'd0) ? DONE : (is_read ? (gather ? RD_IDX : v_type ? RD_SEL : RD) : WR);
					if (start) begin
						r_vreads <= ((v_type && !is_read) ? (8'd1 << ins[5:3]) : 8'b0) | ((gather || scatter) ? (8'd1 << ins[2:0]) : 8'b0);
						state <= PREP;
					end
				end

				//a clock for the registers to follow the unit
				PREP: state <= after;

				RD_SEL: begin
					bcnt       <= 4'd0;
					burst_line <= absolute[21:4];
					state      <= want_burst ? RD_BURST : RD;
				end

				//A gather: the element of Vk for reg_idx is ready when wait_cnt reaches
				//zero, and is taken when it is in (Vk may still be receiving a result;
				//when it is the register being loaded, an element is its address before
				//the word read there takes its place).
				RD_IDX: begin
					if (wait_cnt != 4'd0) wait_cnt <= wait_cnt - 4'd1;
					else if (r_own || i_vk_avail) begin
						address <= gaddr;
						state   <= RD;
					end
				end

				//the elements in the line go to the register as their words pass
				RD_BURST:
				if (i_mem_ack) begin
					bcnt <= bcnt + 4'd1;
					if (burst_hit) begin
						o_v_wr    <= 1'b1;
						o_v_last  <= last;
						put_idx   <= reg_idx;
						reg_idx   <= reg_idx + 6'd1;
						remaining <= remaining - 7'd1;
						address   <= address + stride;
					end
					if (bcnt == 4'd15) state <= (burst_hit ? last : (remaining == 7'd0)) ? DONE : RD_SEL;
				end

				RD:
				if (out_of_field) begin
					tr_range_err <= 1'b1;
					state        <= RD_PUT;
				end else if (i_mem_ack) state <= RD_PUT;

				//o_mem_data now holds the word
				RD_PUT: begin
					o_b_wr_en <= r_to_b;
					o_t_wr_en <= r_to_t;
					o_v_wr    <= r_v;
					o_v_last  <= r_v && last;
					put_idx   <= reg_idx;
					reg_idx   <= reg_idx + 6'd1;
					remaining <= remaining - 7'd1;
					address   <= address + stride;
					wait_cnt  <= r_wait;
					state     <= last ? DONE : (r_gather ? RD_IDX : r_v ? RD_SEL : RD);
				end

				//A store.  The register word for reg_idx is ready when wait_cnt
				//reaches zero.  It moves into wr_word when that is free or being
				//taken, and the register is asked for the word after it.  `remaining`
				//counts the words not yet written or dropped.  The V register a 177
				//stores may still be receiving a result (chaining): its element is
				//taken when it is in.  A scatter takes the element of Vk with the
				//word's address at the same time: the address register follows the
				//address of the element that is taken next (gaddr) while no word is
				//in hand or the word in hand is leaving.
				WR: begin
					if (wait_cnt != 4'd0) wait_cnt <= wait_cnt - 4'd1;
					if (wr_gone) begin
						address  <= address + stride;
						wr_valid <= 1'b0;
					end
					if (r_scatter && (!wr_valid || wr_gone)) address <= gaddr;
					if ((wait_cnt == 4'd0) && (fetch_left != 7'd0) && (!wr_valid || wr_gone) && (!r_v || i_v_avail) && (!r_scatter || i_vk_avail)) begin
						wr_word    <= src_word;
						wr_valid   <= 1'b1;
						reg_idx    <= reg_idx + 6'd1;
						wait_cnt   <= r_wait;
						fetch_left <= fetch_left - 7'd1;
					end
					tr_range_err <= wr_skip;
					remaining    <= remaining - wr_ended;
					if ((wr_ended != 7'd0) && (remaining == wr_ended)) state <= DONE;
				end

				DONE: begin
					state    <= IDLE;
					r_vreads <= 8'b0;
				end

				default: state <= IDLE;
			endcase
	end

	assign o_v_wr_idx  = put_idx;
	assign o_v_rd_idx  = reg_idx;
	assign o_b_wr_addr = put_idx;
	assign o_t_wr_addr = put_idx;
	assign o_b_rd_addr = reg_idx;
	assign o_t_rd_addr = reg_idx;

	always @(posedge clk)
		if (i_mem_ack) o_mem_data <= i_mem_rd_data;
		else if (((state == RD) && out_of_field) || (sc_state == S_ZERO)) o_mem_data <= 64'b0;

	//What the transfer under way is, latched when it started.
	reg tr_range_err;
	reg r_v, r_to_b, r_to_t, r_from_b, r_from_t;
	reg r_gather, r_scatter;
	reg        r_own;  // a gather into the register its addresses come from
	reg [ 2:0] r_vnum;  // the V register of a vector transfer
	reg [ 2:0] r_knum;  // the one with the addresses of a gather or scatter
	reg [ 7:0] r_vreads;
	reg [23:0] gbase;  // (A0) of a gather or scatter
	reg [ 3:0] after;  // the state that follows PREP
	reg [ 3:0] r_wait;

	//The register word for reg_idx, valid src_wait clocks after reg_idx is set.
	//The source register is not changing while this unit runs.  A gather or a
	//scatter waits a clock more, for the address formed from the element of Vk.
	wire [ 3:0] src_wait = from_b || from_t ? 4'd1 : (v_type ? (V_READ_WAIT[3:0] + {3'd0, gather || scatter}) : 4'd0);
	reg  [63:0] src_word;
	always @*
		if (r_from_b) src_word = {40'b0, i_b_rd_data};
		else if (r_from_t) src_word = i_t_rd_data;
		else
			case (r_vnum)
				3'd0: src_word = i_v0_data;
				3'd1: src_word = i_v1_data;
				3'd2: src_word = i_v2_data;
				3'd3: src_word = i_v3_data;
				3'd4: src_word = i_v4_data;
				3'd5: src_word = i_v5_data;
				3'd6: src_word = i_v6_data;
				3'd7: src_word = i_v7_data;
			endcase

	//The address of an element of a gather or scatter: the low 24 bits of the
	//element of Vk, to be added to (A0) (CSM-0111000 page 5-91).
	reg [23:0] idx_word;
	always @*
		case (r_knum)
			3'd0: idx_word = i_v0_data[23:0];
			3'd1: idx_word = i_v1_data[23:0];
			3'd2: idx_word = i_v2_data[23:0];
			3'd3: idx_word = i_v3_data[23:0];
			3'd4: idx_word = i_v4_data[23:0];
			3'd5: idx_word = i_v5_data[23:0];
			3'd6: idx_word = i_v6_data[23:0];
			3'd7: idx_word = i_v7_data[23:0];
		endcase

	//and the address itself, a clock later: off the way of the address register,
	//which decides by its own value whether a word is inside the field
	reg [23:0] gaddr;
	always @(posedge clk) gaddr <= gbase + idx_word;

	//The word being stored is held here, so the registers can move on to the next.
	always @* o_mem_wr_data = sc_req ? sc_word : wr_word;

	//-----------------------------------------------------------------
	// Scalar references
	//-----------------------------------------------------------------
	// What the current instruction would refer to is taken in every clock, and is
	// a reference if the instruction is one and issues.  In the clock after, the
	// address is formed, of which the low 22 bits count, and looked at: it is
	// inside the field if it is below the words the field has from its base on
	// (room, further down).  So the operand range flag is set before the next
	// instruction can issue, which is the clock after that at the earliest: a
	// scalar reference has two parcels.  The reference then joins the queue.
	reg e_v;
	reg e_store, e_s;
	reg [2:0] e_i;
	reg [21:0] e_ah, e_jkm;
	reg  [63:0] e_sdata;  // the registers as they come: which of them is stored is picked a clock later
	reg  [23:0] e_adata;
	wire [21:0] e_rel = e_ah + e_jkm;
	wire        e_oof = ({1'b0, e_rel} >= room);
	always @(posedge clk) begin
		e_v     <= !rst && i_scalar && i_issue;
		e_store <= ins[12];
		e_s     <= ins[13];
		e_i     <= ins[8:6];
		e_ah    <= i_ah_data;
		e_jkm   <= jkm;
		e_sdata <= i_si_data;
		e_adata <= i_ai_data;
	end

	// The queue, and the reference that is being made.  A load from outside the
	// field delivers zero and a store there is dropped, neither with a request
	// to memory.
	localparam QW = 6 + 22 + 64;
	(* ramstyle = "MLAB, no_rw_check" *) reg [QW-1:0] q[0:7];
	reg [2:0] q_wr, q_rd;
	reg  [   3:0] q_n;
	reg           q_ok;  // there is room for a reference that issues now
	wire [QW-1:0] head = q[q_rd];
	wire          h_oof = head[91];
	wire          h_store = head[90];

	reg [21:0] sc_addr;
	reg [63:0] sc_word;
	reg        sc_s;
	reg [ 2:0] sc_i;

	wire       sc_free = !sc_req || i_mem_ack;  // nothing is being made, or it ends in this clock
	wire       sc_pop = sc_free && (q_n != 4'd0);
	wire [3:0] q_n_nxt = q_n + {3'd0, e_v} - {3'd0, sc_pop};

	always @(posedge clk) begin
		o_sc_a <= 1'b0;
		o_sc_s <= 1'b0;
		if (rst) begin
			sc_state <= S_IDLE;
			q_wr     <= 3'd0;
			q_rd     <= 3'd0;
			q_n      <= 4'd0;
			q_ok     <= 1'b1;
		end else begin
			if (e_v) begin
				q[q_wr] <= {e_oof, e_store, e_s, e_i, e_rel, e_s ? e_sdata : {40'b0, e_adata}};
				q_wr    <= q_wr + 3'd1;
			end
			q_n  <= q_n_nxt;
			q_ok <= (q_n_nxt < 4'd7);

			//the word of a load is in o_mem_data in the next clock
			if (((sc_state == S_RD) && i_mem_ack) || (sc_state == S_ZERO)) begin
				o_sc_a   <= !sc_s;
				o_sc_s   <= sc_s;
				o_sc_num <= sc_i;
			end

			if (sc_pop) begin
				q_rd     <= q_rd + 3'd1;
				sc_addr  <= head[85:64] + base[21:0];
				sc_word  <= head[63:0];
				sc_s     <= head[89];
				sc_i     <= head[88:86];
				sc_state <= h_oof ? (h_store ? S_IDLE : S_ZERO) : (h_store ? S_WR : S_RD);
			end else if (sc_free) sc_state <= S_IDLE;
		end
	end

	assign o_sc_pending = e_v || (q_n != 4'd0) || (sc_state != S_IDLE);
	assign o_sc_ready   = (state == IDLE) && q_ok;
	assign o_blk_ready  = (state == IDLE) && !o_sc_pending;
	assign o_range_err  = tr_range_err || (e_v && e_oof);

`ifdef VERILATOR
	// a transfer and a scalar reference never have the port at the same time,
	// and the queue never runs over
	always @(posedge clk)
		if (!rst && (((state != IDLE) && o_sc_pending) || (e_v && q_n[3]))) begin
			$display("mem_fu: a scalar reference beside a transfer, or the queue over its end");
			$finish;
		end
`endif

	//-----------------------------------------------------------------
	// Memory port and issue
	//-----------------------------------------------------------------
	// The field: absolute address = address + base, valid below the limit and below
	// the top of memory, four million words.  The low 22 bits of the address count;
	// the base has 24.  Whether an address is inside is told without the sum: it is
	// if it is below the words the field has from its base on (room, further down).
	reg [23:0] base;
	reg [22:0] limit;
	always @(posedge clk) begin
		base  <= i_data_base_addr;
		limit <= (|i_data_limit_addr[23:22]) ? 23'h400000 : {1'b0, i_data_limit_addr[21:0]};
	end

	wire [21:0] absolute = address[21:0] + base[21:0];
	wire        out_of_field = ({1'b0, address[21:0]} >= room);

	// A burst pays off with three elements in the line: the element in hand and
	// two more, the second of them still short of the end of the line.
	wire third_fits = ({1'b0, absolute[3:0]} + {1'b0, stride[2:0], 1'b0}) < 5'd16;
	wire       want_burst = !out_of_field && (stride[23:3] == 21'd0) && (stride[2:0] != 3'd0) &&
		(remaining >= 7'd3) && third_fits && (absolute[21:4] != SINGLE_LINE);
	wire burst_hit = (remaining != 7'd0) && (absolute[21:4] == burst_line) && (absolute[3:0] == bcnt);

	assign o_mem_addr = sc_req ? sc_addr : (state == RD_BURST) ? {burst_line, 4'b0} : absolute[21:0];
	// A store word outside the field is dropped without a request; one inside
	// leaves when the multiplexer takes it.
	wire       wr_skip = (state == WR) && wr_valid && out_of_field;
	wire       wr_gone = (state == WR) && wr_valid && (out_of_field || i_mem_take);
	wire [6:0] wr_ended = {6'd0, i_mem_ack} + {6'd0, wr_skip};  // words finished this clock

	assign o_mem_ce    = ((state == RD) && !out_of_field) || (state == RD_BURST) || o_mem_wr_en || sc_req;
	assign o_mem_burst = (state == RD_BURST);
	assign o_mem_wr_en = ((state == WR) && wr_valid && !out_of_field) || (sc_state == S_WR);
	assign o_mem_seq   = (state == WR);

	//-----------------------------------------------------------------
	// Letting the instructions behind a transfer issue while it goes on
	//-----------------------------------------------------------------
	// The addresses of a transfer run evenly from the first to the last, so the
	// whole of it is inside the field when both of those are.
	reg               released;  // the instruction has issued; the transfer goes on behind it
	reg        [ 1:0] age;  // clocks since the start, up to 3
	reg signed [31:0] span;  // from the first address to the last

	// Addresses wrap at 22 bits; a transfer that would is not let go.  The last
	// address is inside if it is below the words the field has from its base on.
	wire signed [32:0] last_rel = $signed({9'b0, address}) + $signed({span[31], span});
	wire               wraps = (|address[23:22]) || (|last_rel[31:22]);
	wire               last_in = !last_rel[32] && !wraps && ({1'b0, last_rel[21:0]} < room);

	wire fit_now = !r_gather && !r_scatter && !out_of_field && last_in;

	always @(posedge clk)
		if (rst || (state == IDLE)) begin
			// a transfer's instruction issues in the clock the transfer starts
			released <= !rst && start && i_issue;
			age      <= 2'd0;
		end else begin
			if (age != 2'd3) age <= age + 2'd1;
			if (age == 2'd0) span <= $signed({1'b0, remaining} - 8'sd1) * $signed(stride);
			if (o_mem_issue && i_issue) released <= 1'b1;
		end

	// The quick look, in the clock a transfer starts: a step of less than 2, 16 or
	// 1,024 words, counted up from a first address that is 64, 1,024 or 65,536
	// words or more below the end of the field, cannot leave it in 64 elements.
	// (A block transfer has up to 127 words and takes the second of the three.)
	// Whatever does not pass waits for the last address to be worked out.
	reg [22:0] room;  // words of the field from its base on
	reg [22:0] top1, top2, top3;  // the highest first address for each of the three
	reg ok1, ok2, ok3;
	always @(posedge clk) begin
		room <= ({1'b0, base} < {2'b0, limit}) ? (limit - base[22:0]) : 23'd0;
		ok1  <= (room >= 23'd64);
		ok2  <= (room >= 23'd1024);
		ok3  <= (room >= 23'd65536);
		top1 <= room - 23'd64;
		top2 <= room - 23'd1024;
		top3 <= room - 23'd65536;
	end
	wire [22:0] first = {1'b0, start_addr[21:0]};
	wire quick = (start_addr[23:22] == 2'd0) && !gather && !scatter && (
		(!b_t_type && (start_stride[23:1] == 23'd0) && ok1 && (first <= top1)) ||
		((start_stride[23:4] == 20'd0) && ok2 && (first <= top2)) ||
		((start_stride[23:10] == 14'd0) && ok3 && (first <= top3)));

	always @(posedge clk)
		if (rst) o_unsure <= 1'b0;
		else if (state == IDLE) o_unsure <= start && !quick;
		else if ((state == DONE) || ((age == 2'd1) && fit_now)) o_unsure <= 1'b0;

	// The instruction of a vector transfer issues as the transfer starts, which
	// func_top sees to; should it not have, it may from then on.
	assign o_mem_issue = r_v && (state != IDLE) && !released;
	assign o_mem_busy  = (state != IDLE) || o_sc_pending;
	assign o_blk_b     = (state != IDLE) && (r_to_b || r_from_b);
	assign o_blk_t     = (state != IDLE) && (r_to_t || r_from_t);
	assign o_v_num     = r_vnum;
	assign o_vk_num    = r_knum;
	assign o_v_reads   = r_vreads;

endmodule

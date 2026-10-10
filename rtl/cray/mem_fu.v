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
// The memory port takes requests in a stream and answers reads in order
// (rtl/cray/cray_mem_mux.v): a request is presented with its address and,
// for a read, the number of words, and is taken in a clock where i_mem_take
// is high; the words of reads come back one an acknowledge, in the order the
// reads were taken; a write is done with its take.  Nothing here assumes how
// long memory takes.
//
// A scalar reference issues as soon as both its parcels are there.  Its
// address is formed and checked against the field in the clock after, and it
// then waits in a queue: scalar references are made in the order they issued,
// one presented to memory as soon as the one before has been taken.  The
// register of a load is reserved until its word has come, which the unit
// tells the A or the S registers a clock ahead (o_sc_a, o_sc_s).  The word of
// a store is taken from its register when the instruction issues.  No scalar
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
// is taken right behind it.
//
// A load works ahead of memory.  Its reads are issued one behind the other
// without waiting for their words, up to eight of them, and the words are
// handed to the registers as they come: a read word is at its register the
// clock after the acknowledge.  A load stepping by 1 to 7 words reads the
// elements that lie in one 16-word line as one request, from the first of
// them to the last, and picks the elements out as the words go by; from the
// line SINGLE_LINE only single words are ever read, because reading a word
// there can have an effect.  A store reads its register a word a clock and
// hands the words to memory as it takes them, up to eight of them waiting.
//
// All addresses are relative to the data base address and are checked against
// the limit address (manual 3-43).  Addresses are 24 bits, as the A registers
// are.  A reference outside the field is not made: a store is dropped, a load
// delivers zero, and o_range_err pulses so the operand range flag can be set.

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
	o_mem_len,
	i_mem_rd_data,
	o_mem_wr_data,
	o_mem_wr_en,
	o_mem_ce,
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
	// a 16-word line never read as more than a word: the core's I/O page at the top of memory
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
	output wire [5:0] o_v_rd_idx;  //177: the element about to be read; 176i1k, 1771jk: the element of Vk with the next address
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
	output wire [6:0] o_mem_len;  //words of a read, 1 to 16
	input wire [63:0] i_mem_rd_data;
	output wire [63:0] o_mem_wr_data;
	output wire o_mem_wr_en;
	output wire o_mem_ce;
	input wire i_mem_take;  //the request presented is taken this clock
	input wire i_mem_ack;  //a word read is in i_mem_rd_data
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

	localparam IDLE = 3'd0,  // and for one clock after a start, while the registers follow the unit
	LOAD = 3'd2,  // reads are issued ahead and their words handed to the registers
	STORE = 3'd3,  // the register is read ahead and its words handed to memory
	DONE = 3'd4;

	// a load: the reads are issued by the fetch side, which for a gather waits
	// for each address in turn
	localparam F_RUN = 2'd0, F_GWAIT = 2'd1, F_GREQ = 2'd2, F_DONE = 2'd3;

	localparam IT = 8;  // reads of a load issued and not yet answered, at most, behind the one in hand
	localparam WQ = 8;  // words of a store read from the register and not yet taken, at most

	reg  [ 2:0] state;
	// A transfer starts by the word of the issue logic, which comes late in the
	// clock: only this flag takes it, and the state follows a clock later, with
	// everything else that waits for the start.
	reg         started;  // a transfer started in the clock before
	wire        idle = (state == IDLE) && !started;
	reg  [ 2:0] after;  // the state that follows the start
	reg  [23:0] address;  // of the element that is delivered (a load) or taken (a store) next
	reg  [23:0] stride;
	reg  [ 6:0] remaining;  // elements not yet delivered or taken
	reg  [23:0] f_addr;  // a load: the address of the next element to read
	reg  [ 6:0] f_left;  // and how many are still to be read
	reg  [ 1:0] f_st;
	reg  [21:0] g_addr;  // a gather: the address of the element in hand (the low 22 bits count)
	reg  [ 5:0] reg_idx;  // the B, T or V element read next: a store's word, a gather's or scatter's address
	reg  [ 5:0] reg_idx1;  // reg_idx + 1, for the B or T register word asked for ahead
	reg  [ 5:0] put_idx;  // the B, T or V element a word read goes to next
	reg  [ 5:0] wr_idx;  // the one the word in o_mem_data goes to
	reg  [ 3:0] wait_cnt;
	reg  [ 6:0] fetch_left;  // a store: words not yet read from the register
	reg  [23:0] p_addr;  // a store: the address of the next word read from the register

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

	wire start = idle && ((i_block && i_issue) || (v_type && i_vstart));
	always @(posedge clk) started <= !rst && start;

	//What the transfer under way is, latched when it started.
	reg tr_range_err;
	reg r_v, r_to_b, r_to_t, r_from_b, r_from_t;
	reg r_gather, r_scatter;
	reg        r_own;  // a gather into the register its addresses come from
	reg [ 2:0] r_vnum;  // the V register of a vector transfer
	reg [ 2:0] r_knum;  // the one with the addresses of a gather or scatter
	reg [ 7:0] r_vreads;
	reg [21:0] gbase;  // (A0) of a gather or scatter: the low 22 bits count
	reg [ 3:0] r_wait;

	//-----------------------------------------------------------------
	// The field
	//-----------------------------------------------------------------
	// absolute address = address + base, valid below the limit and below the top
	// of memory, four million words.  The low 22 bits of the address count; the
	// base has 24.  Whether an address is inside is told without the sum: it is
	// if it is below the words the field has from its base on (room).
	reg [23:0] base;
	reg [22:0] limit;
	reg [22:0] room;  // words of the field from its base on
	always @(posedge clk) begin
		base  <= i_data_base_addr;
		limit <= (|i_data_limit_addr[23:22]) ? 23'h400000 : {1'b0, i_data_limit_addr[21:0]};
		room  <= ({1'b0, base} < {2'b0, limit}) ? (limit - base[22:0]) : 23'd0;
	end

	wire [21:0] absolute = address[21:0] + base[21:0];
	wire        out_of_field = ({1'b0, address[21:0]} >= room);

	//-----------------------------------------------------------------
	// A load: what to read next
	//-----------------------------------------------------------------
	// Stepping by 1 to 7 words, the elements that lie in one 16-word line are
	// read as one request from the first of them to the last, as far as the
	// transfer and the field go.  Anything else is read a word at a time.
	// Stage one, from the element at f_addr: where it is, and how far the line,
	// the transfer and the field go on from it.  Stage two, from that: the
	// elements the request covers and how far it advances.  Stage three holds
	// the request until memory takes it, and advances the address as it is
	// filled.  So the chain of arithmetic is cut in three, and a load presents
	// a request every third clock, which the parts of lines do not notice.
	wire    [21:0] f_abs = f_addr[21:0] + base[21:0];
	wire           f_oof = ({1'b0, f_addr[21:0]} >= room);
	wire    [22:0] in_field = room - {1'b0, f_addr[21:0]};  // words of the field from f_addr on, when !f_oof
	reg            s1_v;  // stage one holds an element
	reg            s1_oof;  // outside the field
	reg     [21:0] s1_abs;
	reg            s1_near;  // fewer than 64 words of the field go on from it
	reg     [ 5:0] s1_nf;  // how many then
	reg            s1_io;  // it is in the line SINGLE_LINE
	reg     [ 6:0] s1_left;  // elements of the transfer from it on
	wire    [ 4:0] o5 = {1'b0, s1_abs[3:0]};
	wire    [ 2:0] s3 = stride[2:0];
	wire           small_stride = (stride[23:3] == 21'd0) && (s3 != 3'd0);
	wire           one_stride = (stride == 24'd1);
	// stepping by 1: the words to the end of the line, the transfer or the field
	wire    [ 6:0] run_a = {2'b0, 5'd16 - o5};
	wire    [ 6:0] run_c = s1_near ? {1'b0, s1_nf} : 7'd64;
	wire    [ 6:0] run_ab = (run_a < s1_left) ? run_a : s1_left;
	wire    [ 6:0] run = (run_ab < run_c) ? run_ab : run_c;
	// stepping by 2 to 7: the elements after the first that lie in the line,
	// and how far the last of them is from the first (below 16: the line
	// bounds it; so no multiplier, which the fitter would take a DSP block
	// and its input registers for)
	reg     [ 3:0] hits;
	reg     [ 5:0] hs;  // h steps on
	reg     [ 3:0] last_hs;
	integer        h;
	always @* begin
		hits    = 4'd1;
		last_hs = 4'd0;
		for (h = 1; h < 8; h = h + 1) begin
			hs = h[2:0] * s3;
			if ((({2'b0, o5} + {1'b0, hs}) < 7'd16) && (!s1_near || (hs < s1_nf)) && (s1_left > {4'b0, h[2:0]})) begin
				hits    = hits + 4'd1;
				last_hs = hs[3:0];
			end
		end
	end
	wire [ 4:0] len2 = {1'b0, last_hs} + 5'd1;  // at most 15
	wire        use_chunk = !s1_oof && !s1_io && small_stride && (one_stride ? (run > 7'd1) : (hits > 4'd1));
	wire [ 6:0] item_cnt = use_chunk ? (one_stride ? run : {3'b0, hits}) : 7'd1;  // elements covered
	wire [ 4:0] item_len = use_chunk ? (one_stride ? run[4:0] : len2) : 5'd1;  // words read
	wire [23:0] f_adv = use_chunk ? ({19'b0, item_len - 5'd1} + {21'b0, s3}) : stride;
	// stage two: what the request covers and how far it advances
	reg         s2_v;
	reg         s2_skip;
	reg  [21:0] s2_abs;
	reg  [ 4:0] s2_len;
	reg  [ 6:0] s2_cnt;
	reg  [23:0] s2_adv;
	// the item presented to memory, or delivered as a zero
	reg         nx_v;
	reg         nx_skip;
	reg  [21:0] nx_abs;
	reg  [ 4:0] nx_len;
	wire        ng_push;  // the item in hand is pushed this clock (below)
	wire        nx_load = s2_v && (!nx_v || ng_push);
	wire        s2_load = s1_v && (!s2_v || nx_load);
	// (stage one waits for the address to move on behind the item in stage two)
	wire        s1_load = !s1_v && !s2_v && (f_left != 7'd0) && ((state == LOAD) || started) && !r_gather;
	always @(posedge clk)
		if (rst || idle) begin
			s1_v <= 1'b0;
			s2_v <= 1'b0;
			nx_v <= 1'b0;
		end else begin
			if (s1_load) begin
				s1_v    <= 1'b1;
				s1_oof  <= f_oof;
				s1_abs  <= f_abs;
				s1_near <= (in_field < 23'd64);
				s1_nf   <= in_field[5:0];
				s1_io   <= (f_abs[21:4] == SINGLE_LINE);
				s1_left <= f_left;
			end
			if (s2_load) begin
				s1_v    <= 1'b0;
				s2_v    <= 1'b1;
				s2_skip <= s1_oof;
				s2_abs  <= s1_abs;
				s2_len  <= item_len;
				s2_cnt  <= item_cnt;
				s2_adv  <= f_adv;
			end
			if (nx_load) begin
				s2_v    <= 1'b0;
				nx_v    <= 1'b1;
				nx_skip <= s2_skip;
				nx_abs  <= s2_abs;
				nx_len  <= s2_len;
			end else if (ng_push) nx_v <= 1'b0;
		end

	wire [21:0] g_abs = g_addr + base[21:0];
	reg         g_oof;  // it is outside the field: decided with the address, off the way to the request

	// The reads issued and the elements they are to deliver, in order: a word
	// or a line part, or no read at all for an element outside the field.
	// The one in hand is in registers of its own.
	localparam IW = 1 + 22 + 5;
	reg [IW-1:0] it[0:IT-1];
	reg [$clog2(IT)-1:0] it_wr, it_rd;
	reg [$clog2(IT):0] it_n;  // behind the one in hand
	reg [         3:0] skips;  // of them, not reads
	reg h_valid, h_skip;
	reg  [21:0] cur_word;  // the address of the word that comes next
	reg  [ 4:0] words_left;
	wire        it_room = (it_n != IT[$clog2(IT):0]);

	// A read is not issued behind an element outside the field: that element
	// is delivered first, so that a word can never come for an element that
	// is not the one in hand.
	wire f_want = (state == LOAD) && it_room && (skips == 4'd0);
	wire ld_req = f_want && (r_gather ? ((f_st == F_GREQ) && !g_oof) : (nx_v && !nx_skip));
	wire skip_push = (state == LOAD) && it_room && (r_gather ? ((f_st == F_GREQ) && g_oof) : (nx_v && nx_skip));
	wire ld_take = ld_req && i_mem_take;
	wire it_push = ld_take || skip_push;
	assign ng_push = it_push && !r_gather;
	wire [IW-1:0] it_data = {skip_push, r_gather ? g_abs : nx_abs, r_gather ? 5'd1 : nx_len};

	// the elements delivered
	wire hit = h_valid && !h_skip && i_mem_ack && (r_gather || (absolute == cur_word));
	wire put_skip = h_valid && h_skip;
	wire put = hit || put_skip;
	wire it_pop = put_skip || (h_valid && !h_skip && i_mem_ack && (words_left == 5'd1));
	wire it_new = !h_valid || it_pop;
	wire it_from_q = it_new && (it_n != 0);
	wire it_bypass = it_new && (it_n == 0) && it_push;

	always @(posedge clk)
		if (rst || idle) begin
			h_valid <= 1'b0;
			it_wr   <= 3'd0;
			it_rd   <= 3'd0;
			it_n    <= 4'd0;
			skips   <= 4'd0;
		end else begin
			if (h_valid && !h_skip && i_mem_ack) begin
				cur_word   <= cur_word + 22'd1;
				words_left <= words_left - 5'd1;
			end
			if (it_from_q) begin
				{h_skip, cur_word, words_left} <= it[it_rd];
				h_valid                        <= 1'b1;
				it_rd                          <= it_rd + 1'd1;
			end else if (it_bypass) begin
				{h_skip, cur_word, words_left} <= it_data;
				h_valid                        <= 1'b1;
			end else if (it_new) h_valid <= 1'b0;
			if (it_push && !it_bypass) begin
				it[it_wr] <= it_data;
				it_wr     <= it_wr + 1'd1;
			end
			it_n  <= it_n + {{$clog2(IT) {1'b0}}, it_push && !it_bypass} - {{$clog2(IT) {1'b0}}, it_from_q};
			skips <= skips + {3'd0, skip_push} - {3'd0, put_skip};
		end

	//-----------------------------------------------------------------
	// A store: the words read from the register, waiting for memory
	//-----------------------------------------------------------------
	// The register is asked for a word a clock while there is room for the
	// words on their way.  A B or T register answers in the same clock, a V
	// register two clocks later; a scatter's address comes a clock after its
	// word, so the word waits for it.
	reg [2:0] rv;  // a word asked for 1 to 3 clocks ago
	reg [3:0] credit;  // room in the queue not claimed by a word on its way
	wire        rd_ok = (state == STORE) && (fetch_left != 7'd0) && (credit != 4'd0) && (!r_v || i_v_avail) && (!r_scatter || i_vk_avail);
	wire wq_push = (r_wait == 4'd1) ? rd_ok : (r_wait == 4'd2) ? rv[1] : rv[2];
	reg [63:0] src_word_d;
	wire [63:0] wq_data = r_scatter ? src_word_d : src_word;
	wire [21:0] wq_rel = r_scatter ? gaddr : p_addr[21:0];  // the low 22 bits count
	wire wq_oof = ({1'b0, wq_rel} >= room);
	wire [21:0] wq_abs = wq_rel + base[21:0];

	// The words are in a memory block; whether the one at the head is inside the
	// field, the first thing asked of it, is in flip-flops beside it.
	localparam WW = 22 + 64;
	(* ramstyle = "MLAB, no_rw_check" *)reg [WW-1:0] wq      [0:WQ-1];
	reg [WQ-1:0] wq_oofs;
	reg [$clog2(WQ)-1:0] wq_wr, wq_rd;
	reg  [$clog2(WQ):0] wq_n;
	wire [      WW-1:0] wq_head = wq[wq_rd];
	wire                wq_head_oof = wq_oofs[wq_rd];
	wire                wq_valid = (wq_n != 0);
	wire                st_req = (state == STORE) && wq_valid && !wq_head_oof;
	wire                wr_skip = (state == STORE) && wq_valid && wq_head_oof;  // outside the field: dropped
	wire                wr_gone = st_req && i_mem_take;
	wire                wq_pop = wr_skip || wr_gone;

	always @(posedge clk) begin
		src_word_d <= src_word;
		if (rst || idle) begin
			rv     <= 3'b0;
			credit <= WQ[3:0];
			wq_wr  <= 3'd0;
			wq_rd  <= 3'd0;
			wq_n   <= 4'd0;
		end else begin
			rv <= {rv[1:0], rd_ok};
			if (wq_push) begin
				wq[wq_wr]      <= {wq_abs, wq_data};
				wq_oofs[wq_wr] <= wq_oof;
				wq_wr          <= wq_wr + 1'd1;
			end
			if (wq_pop) wq_rd <= wq_rd + 1'd1;
			wq_n   <= wq_n + {{$clog2(WQ) {1'b0}}, wq_push} - {{$clog2(WQ) {1'b0}}, wq_pop};
			credit <= credit - {3'd0, rd_ok} + {3'd0, wq_pop};
		end
	end

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
				//the start flag, and the mark on the V register a store reads, wait for
				//the decision to start; nothing wide does.  The clock after, the registers
				//follow the unit and the state follows the flag.
				IDLE:
				if (started) state <= after;
				else begin
					r_v        <= v_type;
					r_to_b     <= to_b;
					r_to_t     <= to_t;
					r_from_b   <= from_b;
					r_from_t   <= from_t;
					r_vnum     <= is_read ? ins[8:6] : ins[5:3];
					r_knum     <= ins[2:0];
					r_gather   <= gather;
					r_scatter  <= scatter;
					r_own      <= (ins[8:6] == ins[2:0]);
					gbase      <= start_addr[21:0];
					r_wait     <= src_wait;
					address    <= start_addr;
					stride     <= start_stride;
					remaining  <= start_count;
					f_addr     <= start_addr;
					f_left     <= start_count;
					f_st       <= gather ? F_GWAIT : F_RUN;
					p_addr     <= start_addr;
					reg_idx    <= v_type ? 6'd0 : ins[5:0];
					reg_idx1   <= (v_type ? 6'd0 : ins[5:0]) + 6'd1;
					put_idx    <= v_type ? 6'd0 : ins[5:0];
					wait_cnt   <= src_wait;
					fetch_left <= start_count;
					after      <= (start_count == 7'd0) ? DONE : (is_read ? LOAD : STORE);
					if (start)
						r_vreads <= ((v_type && !is_read) ? (8'd1 << ins[5:3]) : 8'b0) | ((gather || scatter) ? (8'd1 << ins[2:0]) : 8'b0);
				end

				//The fetch side issues the reads; the words come back in order and
				//the elements among them go to the registers, outside-the-field
				//elements as zeros.
				LOAD: begin
					case (f_st)
						F_RUN: begin
							if (nx_load) begin
								f_addr <= f_addr + s2_adv;
								f_left <= f_left - s2_cnt;
							end
							if ((f_left == 7'd0) && !s1_v && !s2_v && !nx_v) f_st <= F_DONE;
						end

						//A gather: the element of Vk for reg_idx is ready when wait_cnt reaches
						//zero, and is taken when it is in (Vk may still be receiving a result;
						//when it is the register being loaded, an element is its address before
						//the word read there takes its place).  The next element is asked for
						//while this one's read is issued.
						F_GWAIT:
						if (wait_cnt != 4'd0) wait_cnt <= wait_cnt - 4'd1;
						else if (r_own || i_vk_avail) begin
							g_addr   <= gaddr;
							g_oof    <= ({1'b0, gaddr} >= room);
							reg_idx  <= reg_idx + 6'd1;
							wait_cnt <= r_wait;
							f_st     <= F_GREQ;
						end

						F_GREQ:
						if (it_push) begin
							f_left <= f_left - 7'd1;
							f_st   <= (f_left == 7'd1) ? F_DONE : F_GWAIT;
						end

						default: ;
					endcase

					if (put) begin
						o_b_wr_en    <= r_to_b;
						o_t_wr_en    <= r_to_t;
						o_v_wr       <= r_v;
						o_v_last     <= r_v && last;
						tr_range_err <= put_skip;
						wr_idx       <= put_idx;
						put_idx      <= put_idx + 6'd1;
						remaining    <= remaining - 7'd1;
						address      <= address + stride;
						if (last) state <= DONE;
					end
				end

				//A store.  The register is read ahead (rd_ok) and the words leave the
				//queue as memory takes them, or are dropped, with the range error,
				//when outside the field.  `remaining` counts the words not yet
				//written or dropped.  The V register a 177 stores may still be
				//receiving a result (chaining): an element is read when it is in.
				//A scatter reads the element of Vk with the word's address at the
				//same time.
				STORE: begin
					if (rd_ok) begin
						reg_idx    <= reg_idx + 6'd1;
						reg_idx1   <= reg_idx1 + 6'd1;
						fetch_left <= fetch_left - 7'd1;
					end
					if (wq_push && !r_scatter) p_addr <= p_addr + stride;
					if (wq_pop) begin
						tr_range_err <= wr_skip;
						remaining    <= remaining - 7'd1;
						address      <= address + stride;
						if (last) state <= DONE;
					end
				end

				DONE: begin
					state    <= IDLE;
					r_vreads <= 8'b0;
				end

				default: state <= IDLE;
			endcase
	end

	assign o_v_wr_idx  = wr_idx;
	assign o_v_rd_idx  = reg_idx;
	assign o_b_wr_addr = wr_idx;
	assign o_t_wr_addr = wr_idx;

	//A B or T register answers in the clock it is asked, too late in that clock
	//for the word to go on into the queue.  So it is asked a clock ahead, for the
	//element after the one in hand when that one is taken, and the word is held
	//a clock: it is there in the clock reg_idx is set, as the queue expects.
	wire        bt_rd = (state == STORE) && (fetch_left != 7'd0) && (credit != 4'd0);  // rd_ok, for a B or T register
	reg  [63:0] bt_word;  // the B or T register word for reg_idx
	wire [ 5:0] bt_rd_addr = bt_rd ? reg_idx1 : reg_idx;
	always @(posedge clk) bt_word <= r_from_b ? {40'b0, i_b_rd_data} : i_t_rd_data;
	assign o_b_rd_addr = bt_rd_addr;
	assign o_t_rd_addr = bt_rd_addr;

	//The register word for reg_idx, valid src_wait clocks after reg_idx is set.
	//A gather or a scatter waits a clock more, for the address formed from the
	//element of Vk.
	wire [ 3:0] src_wait = from_b || from_t ? 4'd1 : (v_type ? (V_READ_WAIT[3:0] + {3'd0, gather || scatter}) : 4'd0);
	reg  [63:0] src_word;
	always @*
		if (r_from_b || r_from_t) src_word = bt_word;
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
	//element of Vk, to be added to (A0) (CSM-0111000 page 5-91); of the sum
	//the low 22 bits count.
	reg [21:0] idx_word;
	always @*
		case (r_knum)
			3'd0: idx_word = i_v0_data[21:0];
			3'd1: idx_word = i_v1_data[21:0];
			3'd2: idx_word = i_v2_data[21:0];
			3'd3: idx_word = i_v3_data[21:0];
			3'd4: idx_word = i_v4_data[21:0];
			3'd5: idx_word = i_v5_data[21:0];
			3'd6: idx_word = i_v6_data[21:0];
			3'd7: idx_word = i_v7_data[21:0];
		endcase

	//and the address itself, a clock later: off the way of the address register,
	//which decides by its own value whether a word is inside the field
	reg [21:0] gaddr;
	always @(posedge clk) gaddr <= gbase + idx_word;

	//-----------------------------------------------------------------
	// Scalar references
	//-----------------------------------------------------------------
	// What the current instruction would refer to is taken in every clock, and is
	// a reference if the instruction is one and issues.  In the clock after, the
	// address is formed, of which the low 22 bits count, and looked at: it is
	// inside the field if it is below the words the field has from its base on
	// (room).  So the operand range flag is set before the next instruction can
	// issue, which is the clock after that at the earliest: a scalar reference
	// has two parcels.  The reference then joins the queue.
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

	// The queue, and the reference at its front, which is presented to memory
	// until it is taken.  A load from outside the field delivers zero and a
	// store there is dropped, neither with a request to memory.  The loads
	// taken and not yet answered are remembered in order (so), to give each
	// word to its register; a zero is given when nothing is on its way, and
	// nothing is taken while it waits, so that a word can never come for a
	// load behind it.
	localparam QW = 6 + 22 + 64;
	(* ramstyle = "MLAB, no_rw_check" *) reg [QW-1:0] q[0:7];
	reg [2:0] q_wr, q_rd;
	reg  [   3:0] q_n;
	reg           q_ok;  // there is room for a reference that issues now
	wire [QW-1:0] head = q[q_rd];

	reg        fr_valid;
	reg        fr_oof;
	reg        fr_store;
	reg        fr_s;
	reg [ 2:0] fr_i;
	reg [21:0] fr_addr;
	reg [63:0] fr_word;

	localparam SO = 8;
	reg [4:0] so[0:SO-1];  // {zero, s, i}
	reg [$clog2(SO)-1:0] so_wr, so_rd;
	reg  [$clog2(SO):0] so_n;
	wire                so_room = (so_n != SO[$clog2(SO):0]);
	wire                so_zero = (so_n != 0) && so[so_rd][4];
	wire                sc_ack = i_mem_ack && (so_n != 0) && !so_zero;

	wire       fr_mem = fr_valid && !fr_oof;
	wire       fr_drop = fr_valid && fr_oof && fr_store;
	wire       fr_zero = fr_valid && fr_oof && !fr_store;
	wire       sc_req = fr_mem && (fr_store || so_room) && !so_zero;
	wire       fr_done = fr_drop || (fr_zero && (so_n == 0)) || (sc_req && i_mem_take);
	wire       fr_load = (q_n != 4'd0) && (!fr_valid || fr_done);
	wire       so_push = (fr_zero && (so_n == 0)) || (sc_req && i_mem_take && !fr_store);
	wire       so_pop = so_zero || sc_ack;
	wire [3:0] q_n_nxt = q_n + {3'd0, e_v} - {3'd0, fr_load};

	always @(posedge clk) begin
		o_sc_a <= 1'b0;
		o_sc_s <= 1'b0;
		if (rst) begin
			q_wr     <= 3'd0;
			q_rd     <= 3'd0;
			q_n      <= 4'd0;
			q_ok     <= 1'b1;
			fr_valid <= 1'b0;
			so_wr    <= 3'd0;
			so_rd    <= 3'd0;
			so_n     <= 4'd0;
		end else begin
			if (e_v) begin
				q[q_wr] <= {e_oof, e_store, e_s, e_i, e_rel, e_s ? e_sdata : {40'b0, e_adata}};
				q_wr    <= q_wr + 3'd1;
			end
			q_n  <= q_n_nxt;
			q_ok <= (q_n_nxt < 4'd7);

			if (fr_load) begin
				q_rd     <= q_rd + 3'd1;
				fr_valid <= 1'b1;
				fr_oof   <= head[91];
				fr_store <= head[90];
				fr_s     <= head[89];
				fr_i     <= head[88:86];
				fr_addr  <= head[85:64] + base[21:0];
				fr_word  <= head[63:0];
			end else if (fr_done) fr_valid <= 1'b0;

			if (so_push) begin
				so[so_wr] <= {fr_zero, fr_s, fr_i};
				so_wr     <= so_wr + 1'd1;
			end
			if (so_pop) so_rd <= so_rd + 1'd1;
			so_n <= so_n + {{$clog2(SO) {1'b0}}, so_push} - {{$clog2(SO) {1'b0}}, so_pop};

			//the word of a load is in o_mem_data in the next clock
			if (so_pop) begin
				o_sc_a   <= !so[so_rd][3];
				o_sc_s   <= so[so_rd][3];
				o_sc_num <= so[so_rd][2:0];
			end
		end
	end

	always @(posedge clk)
		if (i_mem_ack) o_mem_data <= i_mem_rd_data;
		else if (put_skip || so_zero) o_mem_data <= 64'b0;

	assign o_sc_pending = e_v || (q_n != 4'd0) || fr_valid || (so_n != 0);
	assign o_sc_ready   = idle && q_ok;
	assign o_blk_ready  = idle && !o_sc_pending;
	assign o_range_err  = tr_range_err || (e_v && e_oof);

`ifdef VERILATOR
	// a transfer and a scalar reference never have the port at the same time,
	// and the queue never runs over
	always @(posedge clk)
		if (!rst && ((!idle && o_sc_pending) || (e_v && q_n[3]))) begin
			$display("mem_fu: a scalar reference beside a transfer, or the queue over its end");
			$finish;
		end
`endif

	//-----------------------------------------------------------------
	// Memory port and issue
	//-----------------------------------------------------------------
	assign o_mem_addr    = fr_mem ? fr_addr : (state == STORE) ? wq_head[85:64] : (r_gather ? g_abs : nx_abs);
	assign o_mem_len     = ((state == LOAD) && !r_gather) ? {2'b0, nx_len} : 7'd1;
	assign o_mem_wr_data = fr_mem ? fr_word : wq_head[63:0];
	assign o_mem_ce      = sc_req || ld_req || st_req;
	assign o_mem_wr_en   = (sc_req && fr_store) || st_req;

	//-----------------------------------------------------------------
	// Letting the instructions behind a transfer issue while it goes on
	//-----------------------------------------------------------------
	// The addresses of a transfer run evenly from the first to the last, so the
	// whole of it is inside the field when both of those are.
	reg                released;  // the instruction has issued; the transfer goes on behind it
	reg         [ 1:0] age;  // clocks since the start, up to 3
	// The multiplier takes the step and the count from registers of its own,
	// loaded a clock after the start from the start's latch: a short way in,
	// where the latch itself, with the register file's read and the result
	// bypass in front of it, is a long one.  The product is combinational in
	// the clock after, when the last address is looked at: the same clock as
	// before.
	reg         [ 7:0] span_n;
	reg         [23:0] span_s;
	wire signed [31:0] span = $signed(span_n) * $signed(span_s);  // from the first address to the last

	// Addresses wrap at 22 bits; a transfer that would is not let go.  The last
	// address is inside if it is not below the field's base and is below the
	// words the field has from its base on: the span against how far the first
	// address is from each, taken into registers with the step and the count,
	// which keeps the sum out of the way of the compare.
	reg signed [23:0] span_lo;  // minus the first address
	reg signed [23:0] span_hi;  // the words of the field from the first address on
	wire              wraps = |address[23:22];
	wire              last_in = !wraps && (span >= {{8{span_lo[23]}}, span_lo}) && (span < {{8{span_hi[23]}}, span_hi});

	wire fit_now = !r_gather && !r_scatter && !out_of_field && last_in;

	always @(posedge clk)
		if (rst || idle) begin
			// a transfer's instruction issues in the clock the transfer starts
			released <= !rst && start && i_issue;
			age      <= 2'd0;
		end else begin
			if (age != 2'd3) age <= age + 2'd1;
			if (age == 2'd0) begin
				span_n  <= {1'b0, remaining} - 8'd1;
				span_s  <= stride;
				span_lo <= -$signed({2'b0, address[21:0]});
				span_hi <= $signed({1'b0, room}) - $signed({2'b0, address[21:0]});
			end
			if (o_mem_issue && i_issue) released <= 1'b1;
		end

	// The quick look, in the clock a transfer starts: a step of less than 2, 16 or
	// 1,024 words, counted up from a first address that is 64, 1,024 or 65,536
	// words or more below the end of the field, cannot leave it in 64 elements.
	// (A block transfer has up to 127 words and takes the second of the three.)
	// Whatever does not pass waits for the last address to be worked out.
	reg [22:0] top1, top2, top3;  // the highest first address for each of the three
	reg ok1, ok2, ok3;
	always @(posedge clk) begin
		ok1  <= (room >= 23'd64);
		ok2  <= (room >= 23'd1024);
		ok3  <= (room >= 23'd65536);
		top1 <= room - 23'd64;
		top2 <= room - 23'd1024;
		top3 <= room - 23'd65536;
	end
	wire [22:0] first = {1'b0, start_addr[21:0]};
	// (the step's tests go by the register straight: a block transfer's step of 1 passes them all)
	wire step_lt2 = (i_ak_data[23:1] == 23'd0);
	wire step_lt16 = b_t_type || (i_ak_data[23:4] == 20'd0);
	wire step_lt1024 = b_t_type || (i_ak_data[23:10] == 14'd0);
	wire quick = (start_addr[23:22] == 2'd0) && !gather && !scatter && (
		(!b_t_type && step_lt2 && ok1 && (first <= top1)) ||
		(step_lt16 && ok2 && (first <= top2)) ||
		(step_lt1024 && ok3 && (first <= top3)));

	always @(posedge clk)
		if (rst) o_unsure <= 1'b0;
		else if (idle) o_unsure <= start && !quick;
		else if ((state == DONE) || ((age == 2'd1) && fit_now)) o_unsure <= 1'b0;

	// The instruction of a vector transfer issues as the transfer starts, which
	// func_top sees to; should it not have, it may from then on.
	assign o_mem_issue = r_v && !idle && !released;
	assign o_mem_busy  = !idle || o_sc_pending;
	assign o_blk_b     = !idle && (r_to_b || r_from_b);
	assign o_blk_t     = !idle && (r_to_t || r_from_t);
	assign o_v_num     = r_vnum;
	assign o_vk_num    = r_knum;
	assign o_v_reads   = r_vreads;

endmodule

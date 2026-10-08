// The I/O Processor of the Cray I/O Subsystem: a 16-bit accumulator machine
// (HR-0030, I/O Subsystem Model B Hardware Reference Manual, sections 3 to 6).
//
// The reference is the model in tools/crates/ios/src/iop.rs, which has the
// page numbers; this module does what that one does, instruction for
// instruction and clock period for clock period, and sim/harness/iop_main.cpp
// checks it against a record of the model's steps.
//
//   A (16 bits) with the carry C as its bit 2**16; B (9 bits); 512 operand
//   registers; P; a program exit stack of 16 locations addressed by E; the
//   System Interrupt Enable flag I.  An instruction is a parcel with f in
//   bits 15 to 9 and d in bits 8 to 0, or two parcels with a constant k.
//
// Time.  An instruction takes the clock periods in which the manual has its
// result delivered, counted from its issue: one for a PASS, a constant or a
// function that sends, two to five for the other register instructions, seven
// to thirteen with an operand in Local Memory, five for a branch inside the
// instruction stack and nine for one that leaves it (the model has the table
// and the pages).  The real machine can issue an instruction while those
// before it are still under way; this one, like the model, does not.
//
// An instruction is done in its first clock, or in its second when it waits
// for Local Memory or for a channel, and the clocks that are left pass.  So
// that one can follow another in the next clock, the instruction behind it
// is read from Local Memory meanwhile: the last clock of an instruction takes
// its successor into the instruction register and asks for the parcel after
// that, which is the constant of a two-parcel instruction or the next but
// one.  An operand register is read for an instruction as it arrives.
//
// Channels 0 to 3 are part of the processor: 0 reads the number of the channel
// that asks for an interrupt, 1 is the Program Fetch Request flag, 2 the exit
// stack, 3 the Local Memory error flag, which never sets.  Channels 4 to 47
// octal are outside; channel 4, the real-time clock, is kept with them.  A
// channel number above 47 names nothing: flags clear, zero read.  A function
// for a channel outside goes out in the first clock of its instruction, what
// it reads is taken in the second, and so is the flag of a test.
//
// Master Clear (rst) clears P, E, the exit stack and the flags of channels 1
// and 2, and sets I.  The processor then does nothing until a channel asks for
// an interrupt: the dead start is the interrupt of the Buffer Memory channel
// that has loaded Local Memory, and it starts the program at the address in
// exit stack location 0, which is 0.  The accumulator, B, the carry and the
// operand registers keep what they held.

module iop_cpu (
	input wire clk,
	input wire rst,  // Master Clear

	// Local Memory: the parcel at o_mem_addr is on i_mem_rdata in the next clock
	output reg  [15:0] o_mem_addr,
	output reg         o_mem_we,
	output wire [15:0] o_mem_wdata,
	input  wire [15:0] i_mem_rdata,

	// channels 4 to 47 octal
	output wire [ 5:0] o_channel,   // of a function or a flag test
	output wire [ 3:0] o_function,
	output wire        o_strobe,    // the function is sent: one clock
	output wire [15:0] o_a,         // the accumulator, valid with the strobe
	input  wire [15:0] i_data,      // what a function 10 to 13 read, in the clock after the strobe
	input  wire        i_busy,      // the flags of channel o_channel
	input  wire        i_done,
	input  wire [ 5:0] i_request,   // the lowest numbered channel that asks for an interrupt, 0 for none

	// for the simulation: a step (an instruction or an interrupt) ended in
	// the clock before, and what the registers now hold
	output reg         o_step,
	output reg         o_step_interrupt,
	output wire [15:0] o_p,
	output wire [ 8:0] o_b,
	output wire [ 3:0] o_e,
	output wire [ 5:0] o_flags            // held, exit stack boundary, program fetch request, enable delayed, I, C
);

	reg [15:0] a  /* verilator public_flat_rw */;
	reg        c  /* verilator public_flat_rw */;
	reg [ 8:0] b  /* verilator public_flat_rw */;
	reg [15:0] p;  // the instruction in ir in its first clock, its successor after that
	reg [ 3:0] e;
	reg        i_en;  // System Interrupt Enable
	reg        i_late;  // 003 has been executed and I is not set yet
	reg        held;  // Master Clear, and no interrupt since
	reg        pfr;  // channel 1: Program Fetch Request flag
	reg [ 8:0] pfr_reg;
	reg        pfr_en;
	reg        bound;  // channel 2: Exit Stack Boundary flag
	reg        pxs_en;

	reg [15:0] ir;  // the instruction of the step under way
	reg [ 3:0] ph;  // its clock: 0 is the first
	reg [ 3:0] left;  // clocks still to come, this one included, from the second on
	reg        intr;  // it is an interrupt
	reg [15:0] ea;  // address of the operand in memory
	reg [15:0] wd;  // what an instruction stores there

	reg [15:0] xs    [ 0:15]  /* verilator public_flat_rw */;
	(* ramstyle = "no_rw_check" *)
	reg [15:0] or_mem[0:511]  /* verilator public_flat_rw */;
	reg [15:0] or_q;

	assign o_a     = a;
	assign o_p     = p;
	assign o_b     = b;
	assign o_e     = e;
	assign o_flags = {held, bound, pfr, i_late, i_en, c};

	// ---- the instruction

	wire [ 6:0] f = ir[15:9];
	wire [ 8:0] d = ir[8:0];
	wire [15:0] k = i_mem_rdata;  // in the first clock

	// the parcel that arrives, to choose the operand register of the instruction
	// it is when this is the last clock of the one before
	wire [8:0] or_raddr = (i_mem_rdata[15:12] == 4'o06) ? b : i_mem_rdata[8:0];

	wire g_shift = (f[6:2] == 5'b00001) || (f[6:2] == 5'b01001);  // 004-007, 044-047
	wire g_imm = (f[6:3] == 4'o01);  // 010-017
	wire g_reg = (f[6:3] == 4'o02) || (f[6:3] == 4'o06);  // 020-027, 060-067
	wire g_mem = (f[6:3] == 4'o03);  // 030-037
	wire g_flag = (f[6:2] == 5'b01000);  // 040-043
	wire g_b = (f[6:3] == 4'o05);  // 050-057
	wire g_branch = (f >= 7'o070) && (f < 7'o140);
	wire g_io = (f >= 7'o140);

	// ---- the adder and the shifter

	reg [15:0] x;  // the operand
	always @(*) begin
		if (g_mem) x = i_mem_rdata;  // in the second clock
		else if (g_imm) x = f[2] ? k : {7'b0, d};
		else if (g_b) x = {7'b0, b};
		else x = or_q;
	end

	// load, logical product, add, subtract, store, add and store, increment,
	// decrement; 010 to 017 have the first four only
	wire [ 2:0] op = g_imm ? {1'b0, f[1:0]} : f[2:0];
	wire [16:0] ca = {c, a};
	wire [16:0] add_a = (op == 3'd6) ? 17'd1 : (op == 3'd7) ? 17'h0FFFF : ca;
	wire [16:0] add_b = (op == 3'd3) ? {1'b0, ~x} : {1'b0, x};
	wire [16:0] sum = add_a + add_b + {16'b0, op == 3'd3};
	reg  [16:0] ca_op;
	always @(*)
		case (op)
			3'd0:    ca_op = {1'b0, x};
			3'd1:    ca_op = {1'b0, a & x};
			3'd4:    ca_op = ca;
			default: ca_op = sum;
		endcase
	wire stores = !g_imm && op[2];

	// What goes into an operand register is formed from an operand register,
	// with an adder of its own: Local Memory is slow to deliver, and the
	// adder above has it among its operands.
	wire [15:0] or_sum = add_a[15:0] + or_q;
	wire [15:0] or_res = (op == 3'd4) ? a : or_sum;

	// carry and accumulator shift as one register of 17 bits; the count is
	// the low 5 bits of d or B, and a circular shift goes by the count modulo 17
	wire [ 4:0] count = f[5] ? b[4:0] : d[4:0];
	wire        over = (count > 5'd16);
	wire [ 4:0] turn = (count >= 5'd17) ? (count - 5'd17) : count;
	wire [33:0] twice = {ca, ca};
	wire [33:0] turned_r = twice >> turn;
	wire [33:0] turned_l = twice << turn;
	wire [16:0] ended_r = ca >> count;
	wire [16:0] ended_l = ca << count;
	reg  [16:0] ca_shift;
	always @(*)
		case (f[1:0])
			2'd0: ca_shift = over ? 17'd0 : ended_r;
			2'd1: ca_shift = over ? 17'd0 : ended_l;
			2'd2: ca_shift = turned_r[16:0];
			2'd3: ca_shift = turned_l[33:17];
		endcase

	// ---- branches

	wire [2:0] mode = f[6] ? f[4:2] : f[2:0];
	reg        met;
	always @(*)
		case (f[1:0])
			2'd0: met = !c;
			2'd1: met = c;
			2'd2: met = (a == 16'd0);
			2'd3: met = (a != 16'd0);
		endcase
	wire        taken = !f[6] || met;
	wire        two = (g_imm && f[2]) || (g_branch && mode[2] && mode[0]);
	wire [15:0] p_next = p + (two ? 16'd2 : 16'd1);
	wire [15:0] target = mode[2] ? (or_q + (mode[0] ? k : 16'd0)) : mode[0] ? (p - {7'b0, d}) : (p + {7'b0, d});
	wire [ 3:0] e_up = e + 4'd1;
	// a relative branch of a few parcels stays in the instruction stack (HR-0030 page 3-5)
	wire        in_stack = !mode[2] && (mode[0] ? (d <= 9'o13) : (d <= 9'o11));

	// ---- channels

	wire [ 8:0] number = (f[6] ? f[4] : f[1]) ? b : d;  // 042, 043 and 160 to 177 go by B
	wire        ch_own = (number < 9'd4);
	wire        ch_out = (number >= 9'd4) && (number < 9'o50);
	wire [ 3:0] fn = f[3:0];
	wire        reads = (fn[3:2] == 2'b10);  // functions 10 to 13
	// the channel that asks: 1, 2, or one of the others
	wire [ 5:0] asking = (pfr && pfr_en) ? 6'd1 : (bound && pxs_en) ? 6'd2 : i_request;
	reg  [15:0] own_data;
	always @(*) begin
		own_data = 16'd0;
		if (fn == 4'o10)
			case (number[1:0])
				2'd0: own_data = {10'b0, asking};
				2'd1: own_data = {7'b0, pfr_reg};
				2'd2: own_data = {12'b0, e};
				2'd3: own_data = 16'd0;
			endcase
		else if ((fn == 4'o11) && (number[1:0] == 2'd2)) own_data = xs[e];
		if (!ch_own) own_data = 16'd0;
	end
	// channel 0 is always done; none of the four is ever busy
	wire own_done = (number[1:0] == 2'd0) || ((number[1:0] == 2'd1) && pfr) || ((number[1:0] == 2'd2) && bound);
	wire own_flag = ch_own && !f[0] && own_done;

	// ---- the sequence

	wire first = (ph == 4'd0);
	wire interrupt = first && i_en && (asking != 6'd0);
	wire issue = first && !interrupt && !held;  // the instruction in ir begins

	// The clock periods of the instruction in ir: HR-0030 section 6 as the model
	// has them (CLOCK_PERIODS and what follows it in iop.rs).
	reg [3:0] len;
	always @(*) begin
		len = 4'd1;
		casez (f)
			7'o000, 7'o002: len = 4'd1;
			7'o001: len = 4'd7;
			7'o003: len = 4'd2;
			7'b000_01??: len = 4'd3;  // 004-007
			7'o010, 7'o011: len = 4'd1;
			7'o012, 7'o013: len = 4'd3;
			7'o014, 7'o015: len = 4'd2;
			7'o016, 7'o017: len = 4'd4;
			7'o020, 7'o021, 7'o060, 7'o061: len = 4'd2;
			7'o022, 7'o023, 7'o062, 7'o063: len = 4'd4;
			7'o024, 7'o064: len = 4'd2;
			7'o025, 7'o026, 7'o027, 7'o065, 7'o066, 7'o067: len = 4'd5;
			7'o030, 7'o031: len = 4'd7;
			7'o032, 7'o033: len = 4'd9;
			7'o034: len = 4'd6;
			7'o035, 7'o036, 7'o037: len = 4'd13;
			7'b010_00??: len = 4'd5;  // 040-043
			7'b010_01??: len = 4'd3;  // 044-047
			7'o050, 7'o051: len = 4'd1;
			7'o052, 7'o053, 7'o054: len = 4'd3;
			7'o055, 7'o056, 7'o057: len = 4'd4;
			default:
			if (g_branch) len = !taken ? (two ? 4'd2 : 4'd1) : in_stack ? 4'd5 : 4'd9;
			else len = reads ? 4'd5 : 4'd1;  // a function that reads, one that sends
		endcase
	end
	localparam [3:0] LEN_INTERRUPT = 4'd9;  // as a branch out of the instruction stack

	// the last clock of the step; none while Master Clear holds the processor
	wire last = first ? (issue && (len == 4'd1)) : (left == 4'd1);

	// A function for a channel outside goes out in the first clock of its
	// instruction; the number is there for the flag test in the second as well.
	assign o_channel  = number[5:0];
	assign o_function = fn;
	assign o_strobe   = issue && g_io && ch_out;

	// ---- memory

	// The successor of the instruction is asked for in every clock but the last,
	// and the parcel behind it in the last.  In the first clock P is still the
	// instruction itself; a branch that is taken has its address in P from the
	// second.  An operand in memory is asked for in the first clock; a result is
	// stored in the third, the clock after the one it is formed in, so that the
	// way from memory through the adder does not lead back into memory.  An
	// interrupt has its first clock asking for nothing it needs.
	assign o_mem_wdata = wd;
	always @(*) begin
		o_mem_we = 1'b0;
		if (first) begin
			if (g_mem) o_mem_addr = or_q;
			else o_mem_addr = (len == 4'd1) ? (p + 16'd2) : p_next;
		end else if ((ph == 4'd2) && g_mem && stores && !intr) begin
			o_mem_addr = ea;
			o_mem_we   = 1'b1;
		end else o_mem_addr = (left == 4'd1) ? (p + 16'd1) : p;
	end

	integer n;
	always @(posedge clk) begin
		or_q             <= or_mem[or_raddr];
		o_step           <= last && !rst;
		o_step_interrupt <= last && !rst && !first && intr;  // an interrupt is never one clock

		if (rst) begin
			ph      <= 4'd0;
			intr    <= 1'b0;
			p       <= 16'd0;
			e       <= 4'd0;
			i_en    <= 1'b1;
			i_late  <= 1'b0;
			held    <= 1'b1;
			pfr     <= 1'b0;
			pfr_reg <= 9'd0;
			pfr_en  <= 1'b0;
			bound   <= 1'b0;
			pxs_en  <= 1'b0;
			for (n = 0; n < 16; n = n + 1) xs[n] <= 16'd0;
		end else begin
			// the clocks of the step
			if (first) begin
				if (interrupt || issue) begin
					left <= (interrupt ? LEN_INTERRUPT : len) - 4'd1;
					intr <= interrupt;
					if (interrupt || (len != 4'd1)) ph <= 4'd1;
				end
			end else begin
				left <= left - 4'd1;
				ph   <= (left == 4'd1) ? 4'd0 : (ph + 4'd1);
			end
			if (last) ir <= i_mem_rdata;

			// An interrupt is taken between instructions when a channel asks
			// and I is set: I clears, E advances, P is stored there, and the
			// program goes on at the address in location 0.
			if (interrupt) begin
				i_en     <= 1'b0;
				e        <= e_up;
				xs[e_up] <= p;
				if (e_up != 4'd0) p <= xs[0];
				held <= 1'b0;
			end

			// the first clock of an instruction: everything it does without
			// memory or a channel outside
			if (issue) begin
				p  <= p_next;
				ea <= or_q;
				if (f == 7'o001) begin
					// EXIT: at E = 0 the pointer stays and the boundary flag sets
					p <= xs[e];
					if (e == 4'd0) bound <= 1'b1;
					else e <= e - 4'd1;
				end else if (g_shift) {c, a} <= ca_shift;
				else if (g_imm || g_reg || g_b) begin
					{c, a} <= ca_op;
					if (stores && g_reg) or_mem[f[5]?b : d] <= or_res;
					if (stores && g_b) b <= ca_op[8:0];
				end else if (g_branch) begin
					if (taken) begin
						p <= target;
						// a return jump; the boundary flag sets as E reaches 14
						if (mode[1]) begin
							e        <= e_up;
							xs[e_up] <= p_next;
							if (e_up == 4'd14) bound <= 1'b1;
						end
						if (mode[2] && (or_q == 16'd0)) begin
							pfr     <= 1'b1;
							pfr_reg <= d;
						end
					end
				end else if (g_flag) begin
					if (!ch_out) c <= own_flag;
				end else if (g_io && !ch_out) begin
					if (ch_own)
						case (number[1:0])
							2'd1:
							case (fn)
								4'o00, 4'o10: pfr <= 1'b0;
								4'o06:        pfr_en <= 1'b0;
								4'o07:        pfr_en <= 1'b1;
								default:      ;
							endcase
							2'd2:
							case (fn)
								4'o00:   bound <= 1'b0;
								4'o06:   pxs_en <= 1'b0;
								4'o07:   pxs_en <= 1'b1;
								4'o14:   e <= a[3:0];
								4'o15:   xs[e] <= a;
								default: ;
							endcase
							default: ;
						endcase
					if (reads) {c, a} <= {1'b0, own_data};
				end

				// I = 0 also forgets a 003; I = 1 takes effect when the next
				// instruction that is not an EXIT, a 003, a flag test, a
				// branch or a function has been executed
				if (f == 7'o002) begin
					i_en   <= 1'b0;
					i_late <= 1'b0;
				end else if (f == 7'o003) i_late <= 1'b1;
				else if (i_late && !((f == 7'o001) || g_flag || g_branch || g_io)) begin
					i_en   <= 1'b1;
					i_late <= 1'b0;
				end
			end

			// the second clock: the operand from memory, the flag of a channel
			// outside, what a function read there
			if ((ph == 4'd1) && !intr) begin
				if (g_mem) begin
					{c, a} <= ca_op;
					wd     <= ca_op[15:0];
				end else if (g_flag && ch_out) c <= f[0] ? i_busy : i_done;
				else if (g_io && ch_out && reads) {c, a} <= {1'b0, i_data};
			end
		end
	end

endmodule

// The I/O Processor of the Cray I/O Subsystem: a 16-bit accumulator machine
// (HR-0030, I/O Subsystem Model B Hardware Reference Manual, sections 3 to 6).
//
// The reference is the model in tools/crates/ios/src/iop.rs, which has the
// page numbers; this module does what that one does, instruction for
// instruction, and sim/harness/iop_main.cpp checks it against a record of the
// model's steps.  It does not keep the real machine's timing: an instruction
// takes three clocks, four with an operand in memory, five if it stores
// there or works a channel outside the processor.
//
//   A (16 bits) with the carry C as its bit 2**16; B (9 bits); 512 operand
//   registers; P; a program exit stack of 16 locations addressed by E; the
//   System Interrupt Enable flag I.  An instruction is a parcel with f in
//   bits 15 to 9 and d in bits 8 to 0, or two parcels with a constant k.
//
// Channels 0 to 3 are part of the processor: 0 reads the number of the channel
// that asks for an interrupt, 1 is the Program Fetch Request flag, 2 the exit
// stack, 3 the Local Memory error flag, which never sets.  Channels 4 to 47
// octal are outside; channel 4, the real-time clock, is kept with them.  A
// channel number above 47 names nothing: flags clear, zero read.
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
	output reg  [ 5:0] o_channel,   // of a function or a flag test
	output reg  [ 3:0] o_function,
	output reg         o_strobe,    // the function is sent: one clock
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

	localparam S_FETCH = 3'd0,  // between instructions: an interrupt, or the address of the next
	S_DECODE = 3'd1,  // the instruction arrives; its operand register is read, and its k
	S_EXEC = 3'd2,  // most instructions end here
	S_MEM = 3'd3,  // the operand from memory arrives
	S_STORE = 3'd7,  // the result goes back to memory, a clock after it was formed
	S_FLAG = 3'd4,  // the flag of an outside channel
	S_SEND = 3'd5,  // the function is with an outside channel
	S_READ = 3'd6;  // what it read

	reg [ 2:0] state;
	reg [15:0] a  /* verilator public_flat_rw */;
	reg        c  /* verilator public_flat_rw */;
	reg [ 8:0] b  /* verilator public_flat_rw */;
	reg [15:0] p;
	reg [ 3:0] e;
	reg        i_en;  // System Interrupt Enable
	reg        i_late;  // 003 has been executed and I is not set yet
	reg        held;  // Master Clear, and no interrupt since
	reg        pfr;  // channel 1: Program Fetch Request flag
	reg [ 8:0] pfr_reg;
	reg        pfr_en;
	reg        bound;  // channel 2: Exit Stack Boundary flag
	reg        pxs_en;

	reg [15:0] ir;
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
	wire [15:0] k = i_mem_rdata;  // in S_EXEC

	// the instruction as it arrives, to choose the operand register
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
		if (state == S_MEM) x = i_mem_rdata;
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

	// ---- memory

	// A result is stored in the clock after the one it is formed in, so that
	// the way from memory through the adder does not lead back into memory.
	assign o_mem_wdata = wd;
	always @(*) begin
		o_mem_addr = p;
		o_mem_we   = 1'b0;
		case (state)
			S_DECODE: o_mem_addr = p + 16'd1;
			S_EXEC:   o_mem_addr = or_q;
			S_STORE: begin
				o_mem_addr = ea;
				o_mem_we   = 1'b1;
			end
			default:  ;
		endcase
	end

	// ---- the sequence

	wire interrupt = (state == S_FETCH) && i_en && (asking != 6'd0);

	// the instruction ends in this clock
	reg ends;
	always @(*)
		case (state)
			S_EXEC:                  ends = !(g_mem || ((g_flag || g_io) && ch_out));
			S_MEM:                   ends = !stores;
			S_STORE, S_FLAG, S_READ: ends = 1'b1;
			default:                 ends = 1'b0;
		endcase

	integer n;
	always @(posedge clk) begin
		or_q             <= or_mem[or_raddr];
		o_strobe         <= 1'b0;
		o_step           <= 1'b0;
		o_step_interrupt <= 1'b0;

		if (rst) begin
			state   <= S_FETCH;
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
			case (state)
				// An interrupt is taken between instructions when a channel
				// asks and I is set: I clears, E advances, P is stored there,
				// and the program goes on at the address in location 0.
				S_FETCH:
				if (interrupt) begin
					i_en     <= 1'b0;
					e        <= e_up;
					xs[e_up] <= p;
					if (e_up != 4'd0) p <= xs[0];
					held             <= 1'b0;
					o_step           <= 1'b1;
					o_step_interrupt <= 1'b1;
				end else if (!held) state <= S_DECODE;

				S_DECODE: begin
					ir    <= i_mem_rdata;
					state <= S_EXEC;
				end

				S_EXEC: begin
					state <= S_FETCH;
					p     <= p_next;
					ea    <= or_q;
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
					end else if (g_mem) state <= S_MEM;
					else if (g_branch) begin
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
						if (ch_out) begin
							p         <= p;
							o_channel <= number[5:0];
							state     <= S_FLAG;
						end else c <= own_flag;
					end else if (g_io) begin
						if (ch_out) begin
							o_channel  <= number[5:0];
							o_function <= fn;
							o_strobe   <= 1'b1;
							state      <= S_SEND;
						end else begin
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
					end
				end

				S_MEM: begin
					{c, a} <= ca_op;
					wd     <= ca_op[15:0];
					state  <= stores ? S_STORE : S_FETCH;
				end

				S_STORE: state <= S_FETCH;

				S_FLAG: begin
					c     <= f[0] ? i_busy : i_done;
					p     <= p_next;
					state <= S_FETCH;
				end

				S_SEND: state <= S_READ;

				S_READ: begin
					if (reads) {c, a} <= {1'b0, i_data};
					state <= S_FETCH;
				end

				default: state <= S_FETCH;
			endcase

			if (ends) begin
				o_step <= 1'b1;
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
		end
	end

endmodule

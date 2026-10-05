// One I/O Processor of the I/O Subsystem with what every one of them has:
// the processor, its Local Memory, and the channels 4 to 13 octal (HR-0030
// sections 5 and 8).  The reference is the model in tools/crates/ios
// (devices.rs and system.rs), which has the page numbers.
//
//   4       the real-time clock: its Done flag sets every millisecond
//   5       Buffer Memory: block copies between Local Memory and Buffer
//           Memory, and the dead start, which loads Local Memory from Buffer
//           Memory address 0 and then interrupts
//   6, 7    from and to the first of the other IOPs, in ascending order
//   10, 11  the second
//   12, 13  the third
//
// Channels 14 to 47 octal lead out of this module, to the interfaces one IOP
// has and another has not.  They get every function with the accumulator,
// return their Busy and Done flags and, in the clock after a function 10 to
// 13, what it read.  They reach Local Memory through a port of their own.
//
// A channel asks for an interrupt while its Done flag and its Interrupt
// Enable flag are set.  Functions 6 and 7 clear and set Interrupt Enable; the
// flags are kept here for all channels.  A channel named in RAW_REQUEST has
// functions 6 and 7 of its own (the Peripheral Expander) and says itself
// when it asks.
//
// Master Clear holds the processor and clears every flag and register here.
// When it has dropped, i_dead_start starts the load of Local Memory.

module iop #(
	parameter [39:0] RAW_REQUEST   = 40'd0,  // bit n: channel n (decimal) makes its own request
	parameter        CLOCKS_PER_MS = 80000   // of clk; 80,000 or more
) (
	input wire clk,
	input wire i_master_clear,
	input wire i_dead_start,    // one clock
	input wire i_short,         // with i_dead_start: load 4,096 parcels, not all 65,536

	// Buffer Memory: one word a request, held until the acknowledge, which
	// is one clock with the word read
	output reg         o_bm_req,
	output reg         o_bm_we,
	output reg  [23:0] o_bm_addr,
	output reg  [63:0] o_bm_wdata,
	input  wire        i_bm_ack,
	input  wire [63:0] i_bm_rdata,

	// the three other IOPs; element 0 is the first of them
	output wire [47:0] o_link_word,     // the data register to each
	output reg  [ 2:0] o_link_sent,     // one clock: a word was put in it
	output reg  [ 2:0] o_link_taken,    // one clock: the word from that IOP was read
	output wire [11:0] o_link_control,  // bit 0 Master Clear, 1 dead start, 3 short transfer
	input  wire [47:0] i_link_word,
	input  wire [ 2:0] i_link_sent,
	input  wire [ 2:0] i_link_taken,

	// channels 14 to 47 octal (12 to 39)
	output wire         o_ch_strobe,    // a function for channel o_ch_number: one clock
	output wire [  5:0] o_ch_number,
	output wire [  3:0] o_ch_function,
	output wire [ 15:0] o_ch_a,
	input  wire [ 15:0] i_ch_data,      // in the clock after the strobe
	input  wire [39:12] i_ch_busy,
	input  wire [39:12] i_ch_done,
	input  wire [39:12] i_ch_ask,       // for the channels of RAW_REQUEST

	// Local Memory for the interfaces of those channels: a request is held
	// until its acknowledge; the parcel read is there in the clock after
	input  wire        i_dma_req,
	input  wire        i_dma_we,
	input  wire [15:0] i_dma_addr,
	input  wire [15:0] i_dma_wdata,
	output wire        o_dma_ack,
	output wire [15:0] o_dma_rdata,

	// for the simulation
	output wire        o_step,
	output wire [15:0] o_p
);

	// ---- the processor and Local Memory

	wire [15:0] mem_addr, mem_wdata, a;
	wire        mem_we;
	reg  [15:0] mem_q;
	wire [ 5:0] number;
	wire [ 3:0] fn;
	wire        strobe;
	reg  [15:0] data;  // what a function of channels 4 to 13 read
	reg         outer;  // the last function was for a channel from 14 on
	reg busy_now, done_now;
	reg [5:0] request;

	iop_cpu cpu (
		.clk             (clk),
		.rst             (i_master_clear),
		.o_mem_addr      (mem_addr),
		.o_mem_we        (mem_we),
		.o_mem_wdata     (mem_wdata),
		.i_mem_rdata     (mem_q),
		.o_channel       (number),
		.o_function      (fn),
		.o_strobe        (strobe),
		.o_a             (a),
		.i_data          (outer ? i_ch_data : data),
		.i_busy          (busy_now),
		.i_done          (done_now),
		.i_request       (request),
		.o_step          (o_step),
		.o_step_interrupt(),
		.o_p             (o_p),
		.o_b             (),
		.o_e             (),
		.o_flags         ()
	);

	// 65,536 parcels with two ports: the processor's, and one for the
	// Buffer Memory channel and the interfaces outside
	(* ramstyle = "no_rw_check" *) reg [15:0] mem[0:65535]  /* verilator public_flat_rw */;
	reg [15:0] port_addr, port_wdata;
	reg        port_we;
	reg [15:0] port_q;
	always @(posedge clk) begin
		if (mem_we) mem[mem_addr] <= mem_wdata;
		mem_q <= mem[mem_addr];
	end
	always @(posedge clk) begin
		if (port_we) mem[port_addr] <= port_wdata;
		port_q <= mem[port_addr];
	end

	assign o_ch_strobe   = strobe && (number >= 6'd12);
	assign o_ch_number   = number;
	assign o_ch_function = fn;
	assign o_ch_a        = a;

	wire       f_here = strobe && (number < 6'd12);
	wire       f_clock = f_here && (number == 6'd4);
	wire       f_mos = f_here && (number == 6'd5);
	wire       f_link = f_here && (number >= 6'd6);
	wire [1:0] slot = (number == 6'd6 || number == 6'd7) ? 2'd0 : (number == 6'd8 || number == 6'd9) ? 2'd1 : 2'd2;
	wire       f_in = f_link && !number[0];
	wire       f_out = f_link && number[0];

	// ---- flags

	reg [39:4] enable;
	reg        clock_done;
	reg mos_busy, mos_done;
	reg [2:0] in_done;
	reg [2:0] out_busy, out_done;

	wire [39:4] busy = {i_ch_busy, out_busy[2], 1'b0, out_busy[1], 1'b0, out_busy[0], 1'b0, mos_busy, 1'b0};
	wire [39:4] done = {
		i_ch_done, out_done[2], in_done[2], out_done[1], in_done[1], out_done[0], in_done[0], mos_done, clock_done
	};
	wire [39:4] asks = (done & enable & ~RAW_REQUEST[39:4]) | ({i_ch_ask, 8'b0} & RAW_REQUEST[39:4]);

	integer       n;
	reg     [5:0] lowest;
	always @(*) begin
		lowest = 6'd0;
		for (n = 39; n >= 4; n = n - 1) if (asks[n]) lowest = n[5:0];
	end
	always @(*) begin
		busy_now = 1'b0;
		done_now = 1'b0;
		if ((number >= 6'd4) && (number < 6'd40)) begin
			busy_now = busy[number];
			done_now = done[number];
		end
	end

	// ---- channel 4: the real-time clock.  A counter of clock periods of
	// 12.5 ns that sets Done and starts again at 80,000; clk may be faster
	// than 80 MHz.
	reg  [16:0] rtc;
	reg  [17:0] part;
	wire [17:0] part_up = part + 18'd80000;
	wire        tick = (part_up >= CLOCKS_PER_MS);

	// ---- channel 5: Buffer Memory
	localparam M_IDLE = 3'd0, M_WORD = 3'd1, M_PARCEL = 3'd2, M_LAST = 3'd3, M_WAIT = 3'd4;
	reg [ 2:0] m_state;
	reg        m_read;  // into Local Memory
	reg [15:0] m_local;
	reg [23:0] m_buffer;
	reg [14:0] m_left;  // words still to copy
	reg [ 2:0] m_n;  // parcel of the word
	reg [63:0] m_word;
	// this module's use of the second port of Local Memory
	reg m_port, m_port_we;

	// ---- channels 6 to 13: the other IOPs
	reg [15:0] word   [0:2];
	reg [ 3:0] control[0:2];
	assign o_link_word    = {word[2], word[1], word[0]};
	assign o_link_control = {control[2], control[1], control[0]};

	// the second port: the Buffer Memory channel goes first
	always @(*) begin
		port_addr  = m_port ? m_local : i_dma_addr;
		port_we    = m_port ? m_port_we : (i_dma_req && i_dma_we);
		port_wdata = m_port ? m_word[63:48] : i_dma_wdata;
	end
	assign o_dma_ack   = i_dma_req && !m_port;
	assign o_dma_rdata = port_q;

	always @(*) begin
		m_port    = 1'b0;
		m_port_we = 1'b0;
		case (m_state)
			// a word from Buffer Memory goes out over four clocks; a word for
			// it is read over four and is complete one clock later
			M_PARCEL: begin
				m_port    = 1'b1;
				m_port_we = m_read;
			end
			default: ;
		endcase
	end

	always @(posedge clk) begin
		o_link_sent  <= 3'b0;
		o_link_taken <= 3'b0;
		outer        <= strobe ? (number >= 6'd12) : outer;
		request      <= lowest;

		// the clock
		part <= tick ? (part_up - CLOCKS_PER_MS) : part_up;
		if (tick) begin
			rtc <= (rtc == 17'd79999) ? 17'd0 : (rtc + 17'd1);
			if (rtc == 17'd79999) clock_done <= 1'b1;
		end

		if (i_master_clear) begin
			enable     <= 36'd0;
			clock_done <= 1'b0;
			mos_busy   <= 1'b0;
			mos_done   <= 1'b0;
			in_done    <= 3'b0;
			out_busy   <= 3'b0;
			out_done   <= 3'b0;
			m_state    <= M_IDLE;
			m_local    <= 16'd0;
			m_buffer   <= 24'd0;
			o_bm_req   <= 1'b0;
			control[0] <= 4'd0;
			control[1] <= 4'd0;
			control[2] <= 4'd0;
		end else begin
			// ---- functions
			if (strobe && (number >= 6'd4) && (number < 6'd40) && !RAW_REQUEST[number]) begin
				if (fn == 4'o06) enable[number] <= 1'b0;
				if (fn == 4'o07) enable[number] <= 1'b1;
			end
			// what a function 10 reads; every other function reads zero
			if (f_here) data <= (fn != 4'o10) ? 16'd0 : f_clock ? rtc[16:1] : f_in ? i_link_word[16*slot+:16] : 16'd0;
			if (f_clock && (fn == 4'o00)) clock_done <= 1'b0;
			if (f_mos)
				case (fn)
					4'o00: begin
						mos_busy <= 1'b0;
						mos_done <= 1'b0;
						m_state  <= M_IDLE;
						o_bm_req <= 1'b0;
					end
					4'o01:   m_local <= {a[15:2], 2'b00};
					4'o02:   m_buffer[23:9] <= a[14:0];
					4'o03:   m_buffer[8:0] <= a[8:0];
					4'o04, 4'o05: begin
						m_read    <= (fn == 4'o04);
						m_left    <= (a[13:0] == 14'd0) ? 15'd16384 : {1'b0, a[13:0]};
						mos_busy  <= 1'b1;
						mos_done  <= 1'b0;
						m_n       <= 3'd0;
						m_state   <= (fn == 4'o04) ? M_WORD : M_PARCEL;
						o_bm_req  <= (fn == 4'o04);
						o_bm_we   <= 1'b0;
						o_bm_addr <= m_buffer;
					end
					default: ;
				endcase
			if (f_in) begin
				if (fn == 4'o00) in_done[slot] <= 1'b0;
				if (fn == 4'o10) begin
					in_done[slot]      <= 1'b0;
					o_link_taken[slot] <= 1'b1;
				end
			end
			if (f_out)
				case (fn)
					4'o00: begin
						out_busy[slot] <= 1'b0;
						out_done[slot] <= 1'b0;
					end
					4'o01:   control[slot] <= a[3:0];
					4'o14: begin
						out_busy[slot]    <= 1'b1;
						out_done[slot]    <= 1'b0;
						word[slot]        <= a;
						o_link_sent[slot] <= 1'b1;
					end
					default: ;
				endcase

			// ---- the other IOPs: a word arrives; ours was taken
			for (n = 0; n < 3; n = n + 1) begin
				if (i_link_sent[n]) in_done[n] <= 1'b1;
				if (i_link_taken[n]) begin
					out_busy[n] <= 1'b0;
					out_done[n] <= 1'b1;
				end
			end

			// ---- the dead start
			if (i_dead_start) begin
				m_local   <= 16'd0;
				m_buffer  <= 24'd0;
				m_read    <= 1'b1;
				m_left    <= i_short ? 15'd1024 : 15'd16384;
				mos_busy  <= 1'b1;
				mos_done  <= 1'b0;
				enable[5] <= 1'b1;
				m_n       <= 3'd0;
				m_state   <= M_WORD;
				o_bm_req  <= 1'b1;
				o_bm_we   <= 1'b0;
				o_bm_addr <= 24'd0;
			end

			// ---- the copy
			case (m_state)
				// Buffer Memory answers: a word read, or a word written
				M_WORD:
				if (i_bm_ack) begin
					o_bm_req <= 1'b0;
					m_buffer <= m_buffer + 24'd1;
					if (m_read) begin
						m_word  <= i_bm_rdata;
						m_n     <= 3'd0;
						m_state <= M_PARCEL;
					end else if (m_left == 15'd1) m_state <= M_LAST;
					else begin
						m_left  <= m_left - 15'd1;
						m_n     <= 3'd0;
						m_state <= M_PARCEL;
					end
				end

				// four parcels of Local Memory, the lowest address first
				M_PARCEL: begin
					m_local <= m_local + 16'd1;
					m_n     <= m_n + 3'd1;
					if (m_read) begin
						m_word <= {m_word[47:0], 16'b0};
						if (m_n == 3'd3) begin
							if (m_left == 15'd1) m_state <= M_LAST;
							else begin
								m_left    <= m_left - 15'd1;
								o_bm_req  <= 1'b1;
								o_bm_we   <= 1'b0;
								o_bm_addr <= m_buffer;
								m_state   <= M_WORD;
							end
						end
					end else begin
						// the parcel addressed in the clock before arrives
						if (m_n != 3'd0) m_word <= {m_word[47:0], port_q};
						if (m_n == 3'd3) m_state <= M_WAIT;
					end
				end

				// the fourth parcel arrives; the word goes to Buffer Memory
				M_WAIT: begin
					o_bm_req   <= 1'b1;
					o_bm_we    <= 1'b1;
					o_bm_addr  <= m_buffer;
					o_bm_wdata <= {m_word[47:0], port_q};
					m_state    <= M_WORD;
				end

				// the registers are zero when the copy ends
				M_LAST: begin
					mos_busy <= 1'b0;
					mos_done <= 1'b1;
					m_local  <= 16'd0;
					m_buffer <= 24'd0;
					m_state  <= M_IDLE;
				end

				default: ;
			endcase
		end
	end

endmodule

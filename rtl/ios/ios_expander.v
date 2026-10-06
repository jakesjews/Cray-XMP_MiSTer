// The Peripheral Expander, channel 17 octal of the MIOP, with its tape drive,
// its disk and its printer (HR-0030 pages 7-7 to 7-14).  The model is
// tools/crates/ios/src/expander.rs, which says where each part comes from:
// the channel is the manual's, the devices' registers are those the IOS
// software is known to work with.
//
// A device is selected by the address register (function 5).  It has three
// registers A, B and C, a Busy and a Done flag and an interrupt request, and
// takes Start (bit 0) and Clear (bit 1) and Pulse (bit 2) through function 17.
//
//   22  tape    A bits 5-3: 0 read a record, 1 rewind.  B the Local Memory
//               address, C the count of parcels, negative.  A read back is the
//               status: bit 0 ready, 7 at the load point, 8 file mark, 9 end
//               of tape, 15 error.  Only reading and rewinding are done here;
//               the other commands answer ready and move nothing.
//   60  disk    B goes to the register C then names: 5 cylinder, 1 head,
//               2 sector, 3 count of sectors.  With a command in C (0 read,
//               10 write, 20 format, 120 return to cylinder 0) B is the Local
//               Memory address.  A sector is 256 parcels.
//   17  printer A is a command that Pulse runs: 0 new page, 3 new line, 6 take
//               the interrupt request back, 1 graphics mode, 4 text mode.
//               B is a count of parcels, negative; writing a Local Memory
//               address to C prints that many parcels from there, two
//               characters each.  In graphics mode a parcel is sixteen dots
//               of a row of 1,056; a character stands for eight of them, X
//               if any is set, so that a row is as wide as a line of text.
//               What is printed leaves here a character at a time, a form
//               feed for a new page and a line feed for a new line.
//
// The tape is a file in .tap form in a memory of 64-bit words, the first byte
// of the file in bits 63 to 56 of word 0: a record is its length in bytes (4
// bytes, low byte first), the bytes, and the length again; a length of zero
// alone is a file mark.  The disk is read and written a sector of 512 bytes at
// a time through signals like those of the MiSTer framework's hps_io.
//
// Functions 1 to 4, 6 and 14 to 17 are delayed functions: the channel is Busy
// for DELAY clocks and then Done.

module ios_expander #(
	parameter DELAY = 82  // at least a microsecond
) (
	input wire clk,
	input wire rst,  // Master Clear of the MIOP

	// channel 17
	input  wire        i_strobe,
	input  wire [ 3:0] i_function,
	input  wire [15:0] i_a,
	output reg  [15:0] o_data,      // in the clock after a function that reads
	output reg         o_busy,
	output reg         o_done,
	output wire        o_ask,       // the channel asks for an interrupt

	// Local Memory of the MIOP
	output reg         o_dma_req,
	output reg         o_dma_we,
	output reg  [15:0] o_dma_addr,
	output reg  [15:0] o_dma_wdata,
	input  wire        i_dma_ack,
	input  wire [15:0] i_dma_rdata,

	// the tape: a word of the file
	output reg         o_tape_req,
	output reg  [20:0] o_tape_addr,
	input  wire        i_tape_ack,
	input  wire [63:0] i_tape_data,
	input  wire [23:0] i_tape_bytes, // the length of the file

	// the disk
	output reg  [31:0] o_sd_lba,
	output reg         o_sd_rd,
	output reg         o_sd_wr,
	input  wire        i_sd_ack,
	input  wire [ 8:0] i_sd_buff_addr,
	input  wire [ 7:0] i_sd_buff_dout,
	output reg  [ 7:0] o_sd_buff_din,
	input  wire        i_sd_buff_wr,

	// the printer: a character is held out until it is taken
	output reg        o_print_valid,
	output reg  [7:0] o_print,
	input  wire       i_print_ready
);

	localparam [5:0] PRINTER = 6'o17, TAPE = 6'o22, DISK = 6'o60;

	// ---- the channel
	reg [ 5:0] address;
	reg [15:0] bus;
	reg [ 2:0] masked;  // function 6: the disk (bit 6), the tape (5), the printer (3) may not interrupt
	reg channel_ints, device_ints;
	reg [7:0] delay;

	// ---- the devices
	reg [2:0] t_command;  // bits 5 to 3 of register A
	reg [15:0] t_b, t_c, t_status;
	reg t_busy, t_done, t_int;
	reg [23:0] t_pos;  // the byte of the file the heads are at
	reg [ 1:0] t_state;  // 0 load point, 1 behind a record, 2 behind a file mark, 3 end of tape
	reg [15:0] d_a, d_b, d_c;
	reg d_busy, d_done, d_int;
	reg [15:0] d_cylinder, d_head, d_sector, d_count;
	reg [15:0] p_a, p_b, p_at, p_status;
	reg p_busy, p_done, p_int;
	reg p_wait, p_text;  // something to print waits for the tape and the disk: parcels, or one character
	reg       p_graphics;  // the parcels are dots
	reg [7:0] p_low;

	wire sel_printer = (address == PRINTER);
	wire sel_tape = (address == TAPE);
	wire sel_disk = (address == DISK);
	wire exists = sel_printer || sel_tape || sel_disk;
	wire s_busy = (sel_printer && p_busy) || (sel_tape && t_busy) || (sel_disk && d_busy);
	wire s_done = (sel_printer && p_done) || (sel_tape && t_done) || (sel_disk && d_done);
	wire s_int = (sel_printer && p_int) || (sel_tape && t_int) || (sel_disk && d_int);
	wire s_masked = (sel_printer && masked[0]) || (sel_tape && masked[1]) || (sel_disk && masked[2]);
	// the selected device: bit 7 it moves data itself, 9 it may interrupt, 13
	// it asks to, 14 Busy, 15 Done; the channel: bit 8 its interrupt enable,
	// 10 and 11 Busy, 12 Done
	wire [15:0] status = {
		s_done, s_busy, s_int, o_done, o_busy, o_busy, exists && !s_masked, channel_ints, exists, 7'b0
	};
	// the highest priority device that asks: the lowest address
	wire [5:0] asking = p_int ? PRINTER : t_int ? TAPE : d_int ? DISK : 6'd0;
	wire [15:0] t_now = t_status | ((t_state == 2'd0) ? 16'h0080 : (t_state == 2'd2) ? 16'h0100 : (t_state == 2'd3) ? 16'h0200 : 16'h0000);

	assign o_ask = (channel_ints && o_done) || (device_ints && ((p_int && !masked[0]) || (t_int && !masked[1]) || (d_int && !masked[2])));

	wire        delayed = i_strobe && ((i_function >= 4'o01 && i_function <= 4'o04) || (i_function == 4'o06) || (i_function >= 4'o14));
	wire control = i_strobe && (i_function == 4'o17);
	wire t_start = control && sel_tape && i_a[0];
	wire d_start = control && sel_disk && i_a[0];

	// ---- the sector of the disk: one side is the framework's, one ours.
	// Neither reads a byte in the clock it is written in.
	(* ramstyle = "no_rw_check" *)reg  [7:0] sector                           [0:511];
	reg  [8:0] s_addr;
	reg  [7:0] s_wdata;
	reg        s_we;
	reg  [7:0] s_q;
	wire       sd_we = i_sd_buff_wr && i_sd_ack;
	always @(posedge clk) begin
		if (sd_we) sector[i_sd_buff_addr] <= i_sd_buff_dout;
		o_sd_buff_din <= sector[i_sd_buff_addr];
`ifdef RW_POISON
		if (sd_we) o_sd_buff_din <= ~i_sd_buff_dout;
		if (s_we && s_addr == i_sd_buff_addr) o_sd_buff_din <= ~s_wdata;
`endif
	end
	always @(posedge clk) begin
		if (s_we) sector[s_addr] <= s_wdata;
		s_q <= sector[s_addr];
`ifdef RW_POISON
		if (s_we) s_q <= ~s_wdata;
		if (sd_we && i_sd_buff_addr == s_addr) s_q <= ~i_sd_buff_dout;
`endif
	end

	// ---- the work of the devices
	localparam X_IDLE = 5'd0, X_T_LEN = 5'd1, X_T_FETCH = 5'd2, X_T_HIGH = 5'd3, X_T_LOW = 5'd4, X_T_PUT = 5'd5,
		X_D_NEXT = 5'd6, X_D_READ = 5'd7, X_D_WAIT = 5'd8, X_D_HIGH = 5'd9, X_D_LOW = 5'd10, X_D_PUT = 5'd11,
		X_D_GET = 5'd12, X_D_GOT = 5'd13, X_D_FILL = 5'd14, X_D_WRITE = 5'd15, X_D_SENT = 5'd16,
		X_P_GET = 5'd17, X_P_GOT = 5'd18, X_P_HIGH = 5'd19, X_P_LOW = 5'd20;
	reg [4:0] x;
	reg [4:0] x_back;  // where to go on with the byte of the tape that was not at hand
	reg t_wait, d_wait;  // a Start that waits for the other device to finish
	// the tape: the word of the file at hand
	reg [63:0] t_word;
	reg [20:0] t_word_at;
	reg        t_word_valid;
	reg [23:0] t_at;  // the byte wanted
	reg [ 1:0] t_n;
	reg [23:0] t_len, t_done_bytes;
	reg [15:0] t_left;  // parcels still to store
	reg [7:0] t_high;
	wire t_hit = t_word_valid && (t_word_at == t_at[23:3]);
	wire [7:0] t_byte = t_word[8*(7-t_at[2:0])+:8];
	// the disk
	reg [15:0] d_left;  // sectors still to move
	reg [8:0] d_n;  // parcel of the sector
	reg [7:0] d_high;
	wire [15:0] d_track = {d_cylinder[13:0], 2'b0} + d_cylinder + d_head;  // 5 heads
	wire [31:0] d_first = {11'b0, d_track, 5'b0} + {15'b0, d_track, 1'b0} + {16'b0, d_track} + {16'b0, d_sector};  // 35 sectors

	always @(*) begin
		s_addr  = {d_n[7:0], (x == X_D_LOW) || (x == X_D_FILL)};
		s_we    = 1'b0;
		s_wdata = 8'd0;
		// a parcel from Local Memory goes into the sector over two clocks
		if (x == X_D_GOT) begin
			s_we    = 1'b1;
			s_wdata = (d_c == 16'o10) ? i_dma_rdata[15:8] : 8'd0;
		end
		if (x == X_D_FILL) begin
			s_we    = 1'b1;
			s_wdata = d_high;
		end
	end

	always @(posedge clk) begin
		o_data <= 16'd0;
		if (rst) begin
			address                 <= 6'd0;
			masked                  <= 3'd0;
			channel_ints            <= 1'b0;
			device_ints             <= 1'b0;
			o_busy                  <= 1'b0;
			o_done                  <= 1'b0;
			delay                   <= 8'd0;
			{t_busy, t_done, t_int} <= 3'b0;
			{d_busy, d_done, d_int} <= 3'b0;
			{p_busy, p_done, p_int} <= 3'b0;
			p_wait                  <= 1'b0;
			p_graphics              <= 1'b0;
			o_print_valid           <= 1'b0;
			t_pos                   <= 24'd0;
			t_state                 <= 2'd0;
			t_word_valid            <= 1'b0;
			x                       <= X_IDLE;
			t_wait                  <= 1'b0;
			d_wait                  <= 1'b0;
			o_dma_req               <= 1'b0;
			o_tape_req              <= 1'b0;
			o_sd_rd                 <= 1'b0;
			o_sd_wr                 <= 1'b0;
		end else begin
			// ---- the end of a delayed function
			if (delay != 8'd0) begin
				delay <= delay - 8'd1;
				if (delay == 8'd1) begin
					o_busy <= 1'b0;
					o_done <= 1'b1;
				end
			end

			// ---- functions
			if (i_strobe)
				case (i_function)
					4'o00: begin
						o_busy       <= 1'b0;
						o_done       <= 1'b0;
						delay        <= 8'd0;
						channel_ints <= 1'b0;
					end
					4'o01:   bus <= sel_printer ? p_status : sel_tape ? t_now : sel_disk ? d_a : 16'd0;
					4'o02:   bus <= sel_tape ? t_b : sel_disk ? d_b : 16'd0;
					4'o03:   bus <= sel_tape ? t_c : sel_disk ? d_c : 16'd0;
					4'o05:   address <= i_a[5:0];
					4'o06:   masked <= {i_a[6], i_a[5], i_a[3]};
					4'o07: begin
						channel_ints <= i_a[0];
						device_ints  <= i_a[1];
					end
					4'o10:   o_data <= bus;
					4'o11:   o_data <= status | {10'b0, asking};
					4'o13:   o_data <= status | {10'b0, address};
					4'o14: begin
						if (sel_printer) p_a <= i_a;
						if (sel_tape) t_command <= i_a[5:3];
						if (sel_disk) d_a <= i_a;
					end
					4'o15: begin
						if (sel_printer) p_b <= i_a;
						if (sel_tape) t_b <= i_a;
						if (sel_disk) d_b <= i_a;
					end
					4'o16: begin
						if (sel_printer) begin
							p_at             <= i_a;
							p_text           <= 1'b1;
							p_wait           <= 1'b1;
							{p_busy, p_done} <= 2'b10;
						end
						if (sel_tape) t_c <= i_a;
						if (sel_disk) begin
							d_c <= i_a;
							case (i_a)
								16'd5:   d_cylinder <= d_b;
								16'd1:   d_head <= d_b;
								16'd2:   d_sector <= d_b;
								16'd3:   d_count <= d_b;
								default: ;
							endcase
						end
					end
					4'o17: begin
						if (sel_printer) begin
							if (i_a[1]) p_int <= 1'b0;
							if (i_a[2]) begin
								// new page and new line are printed; 6 takes the request back
								if ((p_a == 16'd0) || (p_a == 16'd3)) begin
									p_low            <= (p_a == 16'd0) ? 8'h0C : 8'h0A;
									p_text           <= 1'b0;
									p_wait           <= 1'b1;
									{p_busy, p_done} <= 2'b10;
								end else begin
									if (p_a == 16'd6) p_int <= 1'b0;
									if ((p_a == 16'd1) || (p_a == 16'd4)) p_graphics <= (p_a == 16'd1);
									p_done   <= 1'b1;
									p_status <= 16'd0;
								end
							end
						end
						if (sel_tape && i_a[1]) t_int <= 1'b0;
						if (sel_disk && i_a[1]) d_int <= 1'b0;
					end
					default: ;
				endcase
			if (delayed) begin
				o_busy <= 1'b1;
				o_done <= 1'b0;
				delay  <= DELAY[7:0];
			end
			if (t_start) begin
				t_status <= 16'd0;
				t_busy   <= 1'b1;
				t_done   <= 1'b0;
				t_wait   <= 1'b1;
			end
			if (d_start && ((d_c == 16'o0) || (d_c == 16'o10) || (d_c == 16'o20) || (d_c == 16'o120))) begin
				d_busy <= 1'b1;
				d_done <= 1'b0;
				d_wait <= 1'b1;
			end

			// ---- what the devices do
			case (x)
				X_IDLE:
				if (t_wait) begin
					t_wait <= 1'b0;
					t_n    <= 2'd0;
					t_at   <= t_pos;
					t_len  <= 24'd0;
					t_left <= 16'd0 - t_c;
					case (t_command)
						// read: nothing is left of the tape, or a record or a file mark follows
						3'd0:
						if (t_pos + 24'd4 > i_tape_bytes) begin
							t_state                 <= 2'd3;
							t_status                <= 16'h8001;
							{t_busy, t_done, t_int} <= 3'b011;
						end else x <= X_T_LEN;
						3'd1: begin
							t_pos                   <= 24'd0;
							t_state                 <= 2'd0;
							t_status                <= 16'h0001;
							{t_busy, t_done, t_int} <= 3'b011;
						end
						default: begin
							t_status                <= 16'h0001;
							{t_busy, t_done, t_int} <= 3'b011;
						end
					endcase
				end else if (d_wait) begin
					d_wait   <= 1'b0;
					d_left   <= d_count;
					o_sd_lba <= d_first;
					if (d_c == 16'o120) begin
						d_cylinder <= 16'd0;
						d_head     <= 16'd0;
						d_sector   <= 16'd0;
						d_left     <= 16'd0;
					end
					x <= X_D_NEXT;
				end else if (p_wait) begin
					p_wait <= 1'b0;
					if (!p_text) x <= X_P_LOW;
					else if (p_b == 16'd0) begin
						{p_busy, p_done, p_int} <= 3'b011;
						p_status                <= 16'h4000;
					end else x <= X_P_GET;
				end

				// a byte of the tape that is not in the word at hand
				X_T_FETCH:
				if (!o_tape_req && !t_hit) begin
					o_tape_req  <= 1'b1;
					o_tape_addr <= t_at[23:3];
				end else if (i_tape_ack) begin
					o_tape_req   <= 1'b0;
					t_word       <= i_tape_data;
					t_word_at    <= o_tape_addr;
					t_word_valid <= 1'b1;
					x            <= x_back;
				end

				// the length of the record, low byte first; its fourth byte is zero
				X_T_LEN:
				if (!t_hit) begin
					x_back <= X_T_LEN;
					x      <= X_T_FETCH;
				end else begin
					t_n  <= t_n + 2'd1;
					t_at <= t_at + 24'd1;
					if (t_n != 2'd3) t_len <= {t_byte, t_len[23:8]};
					else if (t_len == 24'd0) begin
						t_pos                   <= t_pos + 24'd4;
						t_state                 <= 2'd2;
						t_status                <= 16'h8001;
						{t_busy, t_done, t_int} <= 3'b011;
						x                       <= X_IDLE;
					end else begin
						t_done_bytes <= 24'd0;
						x            <= X_T_HIGH;
					end
				end

				// two bytes are a parcel, the high one first; the record is
				// passed over when the count is used up or the record ends
				X_T_HIGH:
				if ((t_done_bytes >= t_len) || (t_left == 16'd0)) begin
					t_pos                   <= t_pos + t_len + 24'd8;
					t_state                 <= 2'd1;
					t_status                <= 16'h0001;
					{t_busy, t_done, t_int} <= 3'b011;
					x                       <= X_IDLE;
				end else if (!t_hit) begin
					x_back <= X_T_HIGH;
					x      <= X_T_FETCH;
				end else begin
					t_high <= t_byte;
					t_at   <= t_at + 24'd1;
					x      <= X_T_LOW;
				end

				X_T_LOW:
				if ((t_done_bytes + 24'd1 < t_len) && !t_hit) begin
					x_back <= X_T_LOW;
					x      <= X_T_FETCH;
				end else begin
					o_dma_req   <= 1'b1;
					o_dma_we    <= 1'b1;
					o_dma_addr  <= t_b;
					o_dma_wdata <= {t_high, (t_done_bytes + 24'd1 < t_len) ? t_byte : 8'd0};
					t_at        <= t_at + 24'd1;
					x           <= X_T_PUT;
				end

				X_T_PUT:
				if (i_dma_ack) begin
					o_dma_req    <= 1'b0;
					t_b          <= t_b + 16'd1;
					t_c          <= t_c + 16'd1;
					t_left       <= t_left - 16'd1;
					t_done_bytes <= t_done_bytes + 24'd2;
					x            <= X_T_HIGH;
				end

				// the next sector, or the end of the operation
				X_D_NEXT:
				if (d_left == 16'd0) begin
					d_a[0]                  <= 1'b0;
					{d_busy, d_done, d_int} <= 3'b011;
					x                       <= X_IDLE;
				end else begin
					d_n <= 9'd0;
					if (d_c == 16'o0) begin
						o_sd_rd <= 1'b1;
						x       <= X_D_READ;
					end else x <= X_D_GET;
				end

				// the framework takes the request and fills the sector
				X_D_READ:
				if (i_sd_ack) begin
					o_sd_rd <= 1'b0;
					x       <= X_D_WAIT;
				end
				X_D_WAIT: if (!i_sd_ack) x <= X_D_HIGH;

				// from the sector to Local Memory: the byte asked for in one
				// clock is there in the next
				X_D_HIGH: x <= X_D_LOW;
				X_D_LOW: begin
					d_high <= s_q;
					x      <= X_D_PUT;
				end
				X_D_PUT:
				if (!o_dma_req) begin
					o_dma_req   <= 1'b1;
					o_dma_we    <= 1'b1;
					o_dma_addr  <= d_b;
					o_dma_wdata <= {d_high, s_q};
				end else if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					d_b       <= d_b + 16'd1;
					d_n       <= d_n + 9'd1;
					if (d_n == 9'd255) begin
						d_left   <= d_left - 16'd1;
						o_sd_lba <= o_sd_lba + 32'd1;
						x        <= X_D_NEXT;
					end else x <= X_D_HIGH;
				end

				// from Local Memory to the sector; a format writes zeros
				X_D_GET:
				if (!o_dma_req) begin
					o_dma_req  <= 1'b1;
					o_dma_we   <= 1'b0;
					o_dma_addr <= d_b;
				end else if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					x         <= X_D_GOT;
				end
				X_D_GOT: begin
					d_high <= (d_c == 16'o10) ? i_dma_rdata[7:0] : 8'd0;
					x      <= X_D_FILL;
				end
				X_D_FILL: begin
					d_b <= d_b + 16'd1;
					d_n <= d_n + 9'd1;
					if (d_n == 9'd255) begin
						o_sd_wr <= 1'b1;
						x       <= X_D_WRITE;
					end else x <= X_D_GET;
				end
				X_D_WRITE:
				if (i_sd_ack) begin
					o_sd_wr <= 1'b0;
					x       <= X_D_SENT;
				end
				X_D_SENT:
				if (!i_sd_ack) begin
					d_left   <= d_left - 16'd1;
					o_sd_lba <= o_sd_lba + 32'd1;
					x        <= X_D_NEXT;
				end

				// from Local Memory to the printer, the high character of a parcel first
				X_P_GET:
				if (!o_dma_req) begin
					o_dma_req  <= 1'b1;
					o_dma_we   <= 1'b0;
					o_dma_addr <= p_at;
				end else if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					x         <= X_P_GOT;
				end
				X_P_GOT: begin
					o_print       <= !p_graphics ? i_dma_rdata[15:8] : (i_dma_rdata[15:8] != 8'd0) ? "X" : " ";
					o_print_valid <= 1'b1;
					p_low         <= !p_graphics ? i_dma_rdata[7:0] : (i_dma_rdata[7:0] != 8'd0) ? "X" : " ";
					p_at          <= p_at + 16'd1;
					p_b           <= p_b + 16'd1;
					x             <= X_P_HIGH;
				end
				X_P_HIGH:
				if (i_print_ready) begin
					o_print_valid <= 1'b0;
					x             <= X_P_LOW;
				end
				// the low character, or the one that a new page or a new line is
				X_P_LOW:
				if (!o_print_valid) begin
					o_print       <= p_low;
					o_print_valid <= 1'b1;
				end else if (i_print_ready) begin
					o_print_valid <= 1'b0;
					if (p_text && (p_b != 16'd0)) x <= X_P_GET;
					else begin
						{p_busy, p_done, p_int} <= 3'b011;
						p_status                <= 16'h4000;
						x                       <= X_IDLE;
					end
				end

				default: x <= X_IDLE;
			endcase
		end
	end

endmodule

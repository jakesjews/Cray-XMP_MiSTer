// The DD-29 disk drives of the BIOP, each on a channel of its own (HR-0077,
// the disk systems manual, section 2).  The model is `Dd29` in
// tools/crates/ios/src/devices.rs.
//
// A drive has 823 cylinders of 10 head groups of 18 sectors; a sector is 512
// words, 2,048 parcels, 4,096 bytes.  Its image is the sectors in that order,
// each parcel high byte first.  Here a sector is moved as eight blocks of 512
// bytes over signals like those of the MiSTer framework's hps_io.
//
//   0   clear Busy and Done
//   1   select a mode or ask for status, by bits 11 to 9 of the accumulator:
//       0 release the unit, 1 reserve it, 3 return to cylinder 0, 7 read a
//       register into the Status Response register
//   2   read the sector in the accumulator to Local Memory
//   3   write it from Local Memory
//   4   select the head group      5   seek to the cylinder
//   10  read the Local Memory Address register     11  the Status Response
//   14  enter the Local Memory Address register    15  the Status Response
//
// From Master Clear to the first function 1 a drive is in buffer echo mode:
// a write fills the controller's buffer with 512 parcels and a read empties
// it again, with no disk involved.  The BIOP tests every drive this way.
//
// There are no faults.  One sector buffer and one mover serve all drives, a
// drive at a time; what a drive is asked while another is being served waits.
// Every drive has an echo buffer of its own: the BIOP writes to all of them
// before it reads any back.
//
// Time.  A drive answers as fast as its image does, with a short pause for a
// seek, unless i_real asks for the DD-29's own times (HR-0077 page 2-1): a
// seek of 15 ms and up to 80 ms across all cylinders, a revolution of 16.6 ms
// for 18 sectors, and a sector that is read or written while it passes under
// the heads.  Then a sector is done when it has next passed, and a seek when
// its time is over; the image is asked at once all the same, and a sector
// whose image is slower than the drive is done when the image is.  A sector
// asked for while it passes counts as caught (the manual has no time for the
// gap before its data), so the sectors of a track can follow each other at
// the 0.92 ms each takes.  All drives turn together.

module ios_disks #(
	parameter DRIVES        = 9,
	parameter SETTLE        = 200,   // clocks a mode selection or a seek takes
	parameter CLOCKS_PER_MS = 80000  // of clk, for the DD-29's own times
) (
	input wire clk,
	input wire rst,    // Master Clear of the BIOP
	input wire i_real, // seeks and sectors take the DD-29's times

	input  wire              i_strobe,    // one clock: a function for the drive i_drive
	input  wire [       3:0] i_drive,
	input  wire [       3:0] i_function,
	input  wire [      15:0] i_a,
	output reg  [      15:0] o_data,      // in the clock after a function that reads
	output reg  [DRIVES-1:0] o_busy,
	output reg  [DRIVES-1:0] o_done,

	// Local Memory of the BIOP
	output reg         o_dma_req,
	output reg         o_dma_we,
	output reg  [15:0] o_dma_addr,
	output reg  [15:0] o_dma_wdata,
	input  wire        i_dma_ack,
	input  wire [15:0] i_dma_rdata,

	// the images: eight blocks of 512 bytes from block o_sd_lba of one drive
	output reg  [      31:0] o_sd_lba,
	output reg  [DRIVES-1:0] o_sd_rd,
	output reg  [DRIVES-1:0] o_sd_wr,
	input  wire [DRIVES-1:0] i_sd_ack,
	input  wire [      11:0] i_sd_buff_addr,
	input  wire [       7:0] i_sd_buff_dout,
	output reg  [       7:0] o_sd_buff_din,
	input  wire              i_sd_buff_wr
);

	// ---- the drives
	reg [      15:0] lma      [0:DRIVES-1];  // Local Memory Address register
	reg [      15:0] response [0:DRIVES-1];  // Status Response register
	reg [       9:0] cylinder [0:DRIVES-1];
	reg [       3:0] head     [0:DRIVES-1];
	reg [DRIVES-1:0] reserved;
	reg [DRIVES-1:0] echo;
	reg [      23:0] settle   [0:DRIVES-1];  // clocks until a selection or a seek is done
	// a sector to move: asked for, not begun
	reg [DRIVES-1:0] asked;
	reg [DRIVES-1:0] ask_write, ask_zero, ask_echo;
	reg [17:0] ask_sector[0:DRIVES-1];  // its number on the drive
	reg [DRIVES-1:0] dropped;  // function 0 came while the sector was being moved

	// ---- the DD-29's times.  A seek: 15 ms and 65 ms more across all 822
	// cylinders; none for a seek that stays where the heads are.  The disk: 18
	// sectors pass in 16.6 ms.
	localparam [31:0] SEEK_BASE_32  = 15 * CLOCKS_PER_MS;
	localparam [31:0] SEEK_STEP_32  = 65 * CLOCKS_PER_MS / 822;
	localparam [31:0] SECTOR_32     = 166 * CLOCKS_PER_MS / (10 * 18);
	localparam [23:0] SEEK_BASE     = SEEK_BASE_32                    [23:0];
	localparam [13:0] SEEK_STEP     = SEEK_STEP_32                    [13:0];
	localparam [17:0] SECTOR_CLOCKS = SECTOR_32                       [17:0];
	wire [9:0] seek_from = cylinder[i_drive];
	wire [9:0] seek_span = (i_a[9:0] > seek_from) ? (i_a[9:0] - seek_from) : (seek_from - i_a[9:0]);
	wire [23:0] seek_time = SEEK_BASE + seek_span * SEEK_STEP;
	reg [17:0] turn_clock;  // clocks into the sector under the heads
	reg [4:0] turn_sector;  // that sector, 0 to 17
	wire turn_end = (turn_clock == SECTOR_CLOCKS - 18'd1);
	// a sector in real time: asked for, it is done when it has passed under the
	// heads and the image has it
	reg [DRIVES-1:0] awaited;  // it has not passed yet
	reg [DRIVES-1:0] moved;  // the image has it
	reg [4:0] want[0:DRIVES-1];  // its number on the track

	wire [ 3:0] d = i_drive;
	wire [13:0] track = {cylinder[d], 3'b0} + {2'b0, cylinder[d], 1'b0} + {10'b0, head[d]};  // 10 head groups
	wire [17:0] sector = {track, 4'b0} + {3'b0, track, 1'b0} + {13'b0, i_a[4:0]};  // 18 sectors

	// ---- the buffer: one side is the framework's, one the mover's.  Neither
	// reads a byte in the clock it is written in.
	(* ramstyle = "no_rw_check" *)reg  [ 7:0] buffer                              [0:4095];
	reg  [11:0] b_addr;
	reg  [ 7:0] b_wdata;
	reg         b_we;
	reg  [ 7:0] b_q;
	wire        sd_we = i_sd_buff_wr && (|i_sd_ack);
	always @(posedge clk) begin
		if (sd_we) buffer[i_sd_buff_addr] <= i_sd_buff_dout;
		o_sd_buff_din <= buffer[i_sd_buff_addr];
`ifdef RW_POISON
		if (sd_we) o_sd_buff_din <= ~i_sd_buff_dout;
		if (b_we && b_addr == i_sd_buff_addr) o_sd_buff_din <= ~b_wdata;
`endif
	end
	always @(posedge clk) begin
		if (b_we) buffer[b_addr] <= b_wdata;
		b_q <= buffer[b_addr];
`ifdef RW_POISON
		if (b_we) b_q <= ~b_wdata;
		if (sd_we && i_sd_buff_addr == b_addr) b_q <= ~i_sd_buff_dout;
`endif
	end

	// ---- the mover
	localparam M_IDLE = 4'd0, M_READ = 4'd1, M_WAIT = 4'd2, M_HIGH = 4'd3, M_LOW = 4'd4, M_PUT = 4'd5, M_GET = 4'd6,
		M_GOT = 4'd7, M_FILL = 4'd8, M_WRITE = 4'd9, M_SENT = 4'd10, M_END = 4'd11;
	reg [3:0] m;
	reg [3:0] cur;  // the drive being served
	reg m_zero, m_echo;
	reg [11:0] m_n;  // parcel of the sector
	reg [11:0] m_last;  // the last one
	reg [ 7:0] m_high;

	// ---- the echo buffers: 512 parcels for each drive
	reg  [15:0] echoes                   [0:8191];
	reg  [15:0] e_q;
	wire [12:0] e_addr = {cur, m_n[8:0]};
	always @(posedge clk) begin
		if ((m == M_GOT) && m_echo) echoes[e_addr] <= i_dma_rdata;
		e_q <= echoes[e_addr];
	end

	// the next drive that asks, the one after the last served first
	reg     [3:0] pick;
	reg           any;
	integer       k;
	always @(*) begin
		pick = 4'd0;
		any  = 1'b0;
		for (k = DRIVES - 1; k >= 0; k = k - 1)
		if (asked[k] && (k[3:0] <= cur)) begin
			pick = k[3:0];
			any  = 1'b1;
		end
		for (k = DRIVES - 1; k >= 0; k = k - 1)
		if (asked[k] && (k[3:0] > cur)) begin
			pick = k[3:0];
			any  = 1'b1;
		end
	end

	always @(*) begin
		b_addr  = {m_n[10:0], (m == M_LOW) || (m == M_FILL)};
		b_we    = 1'b0;
		b_wdata = 8'd0;
		if ((m == M_GOT) && !m_echo) begin
			b_we    = 1'b1;
			b_wdata = m_zero ? 8'd0 : i_dma_rdata[15:8];
		end
		if (m == M_FILL) begin
			b_we    = 1'b1;
			b_wdata = m_high;
		end
	end

	// the disks turn all the time
	always @(posedge clk) begin
		if (turn_end) begin
			turn_clock  <= 18'd0;
			turn_sector <= (turn_sector >= 5'd17) ? 5'd0 : (turn_sector + 5'd1);
		end else turn_clock <= (turn_clock >= SECTOR_CLOCKS) ? 18'd0 : (turn_clock + 18'd1);
	end

	integer n;
	always @(posedge clk) begin
		o_data <= 16'd0;
		if (rst) begin
			o_busy    <= {DRIVES{1'b0}};
			o_done    <= {DRIVES{1'b0}};
			reserved  <= {DRIVES{1'b0}};
			echo      <= {DRIVES{1'b1}};
			asked     <= {DRIVES{1'b0}};
			dropped   <= {DRIVES{1'b0}};
			m         <= M_IDLE;
			cur       <= 4'd0;
			o_dma_req <= 1'b0;
			o_sd_rd   <= {DRIVES{1'b0}};
			o_sd_wr   <= {DRIVES{1'b0}};
			awaited   <= {DRIVES{1'b0}};
			moved     <= {DRIVES{1'b0}};
			for (n = 0; n < DRIVES; n = n + 1) begin
				lma[n]      <= 16'd0;
				response[n] <= 16'd0;
				settle[n]   <= 24'd0;
			end
		end else begin
			// ---- a selection or a seek comes to its end
			for (n = 0; n < DRIVES; n = n + 1)
			if (settle[n] != 24'd0) begin
				settle[n] <= settle[n] - 24'd1;
				if (settle[n] == 24'd1) begin
					o_busy[n] <= 1'b0;
					o_done[n] <= 1'b1;
				end
			end

			// ---- a sector in the DD-29's time: it passes, and it is done when the
			// image has it as well
			for (n = 0; n < DRIVES; n = n + 1) begin
				if (awaited[n] && turn_end && (turn_sector == want[n])) awaited[n] <= 1'b0;
				if (moved[n] && !awaited[n]) begin
					moved[n]  <= 1'b0;
					o_busy[n] <= 1'b0;
					o_done[n] <= 1'b1;
				end
			end

			// ---- the mover
			case (m)
				M_IDLE:
				if (any) begin
					cur         <= pick;
					asked[pick] <= 1'b0;
					m_zero      <= ask_zero[pick];
					m_echo      <= ask_echo[pick];
					m_n         <= 12'd0;
					m_last      <= ask_echo[pick] ? 12'd511 : 12'd2047;
					o_sd_lba    <= {11'b0, ask_sector[pick], 3'b0};
					if (ask_write[pick]) m <= M_GET;
					else if (ask_echo[pick]) m <= M_HIGH;
					else begin
						o_sd_rd[pick] <= 1'b1;
						m             <= M_READ;
					end
				end

				// the framework takes the request and fills the buffer
				M_READ:
				if (i_sd_ack[cur]) begin
					o_sd_rd[cur] <= 1'b0;
					m            <= M_WAIT;
				end
				M_WAIT: if (!i_sd_ack[cur]) m <= M_HIGH;

				// from the buffer to Local Memory, a parcel high byte first;
				// from an echo buffer a parcel at a time
				M_HIGH: m <= m_echo ? M_PUT : M_LOW;
				M_LOW: begin
					m_high <= b_q;
					m      <= M_PUT;
				end
				M_PUT:
				if (!o_dma_req) begin
					o_dma_req   <= 1'b1;
					o_dma_we    <= 1'b1;
					o_dma_addr  <= lma[cur];
					o_dma_wdata <= m_echo ? e_q : {m_high, b_q};
				end else if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					lma[cur]  <= lma[cur] + 16'd1;
					m_n       <= m_n + 12'd1;
					m         <= (m_n == m_last) ? M_END : M_HIGH;
				end

				// from Local Memory to the buffer; a special mode writes zeros
				M_GET:
				if (!o_dma_req) begin
					o_dma_req  <= 1'b1;
					o_dma_we   <= 1'b0;
					o_dma_addr <= lma[cur];
				end else if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					m         <= M_GOT;
				end
				M_GOT:
				if (m_echo) begin
					lma[cur] <= lma[cur] + 16'd1;
					m_n      <= m_n + 12'd1;
					m        <= (m_n == m_last) ? M_END : M_GET;
				end else begin
					m_high <= m_zero ? 8'd0 : i_dma_rdata[7:0];
					m      <= M_FILL;
				end
				M_FILL: begin
					lma[cur] <= lma[cur] + 16'd1;
					m_n      <= m_n + 12'd1;
					if (m_n != m_last) m <= M_GET;
					else begin
						o_sd_wr[cur] <= 1'b1;
						m            <= M_WRITE;
					end
				end
				M_WRITE:
				if (i_sd_ack[cur]) begin
					o_sd_wr[cur] <= 1'b0;
					m            <= M_SENT;
				end
				M_SENT: if (!i_sd_ack[cur]) m <= M_END;

				// an echo is of the controller and takes no time of the disk's
				M_END: begin
					if (!dropped[cur]) begin
						if (i_real && !m_echo) moved[cur] <= 1'b1;
						else begin
							o_busy[cur] <= 1'b0;
							o_done[cur] <= 1'b1;
						end
					end
					dropped[cur] <= 1'b0;
					m            <= M_IDLE;
				end

				default: m <= M_IDLE;
			endcase

			// ---- functions: these come last and win
			if (i_strobe && (d < DRIVES))
				case (i_function)
					4'o00: begin
						o_busy[d]  <= 1'b0;
						o_done[d]  <= 1'b0;
						settle[d]  <= 24'd0;
						asked[d]   <= 1'b0;
						awaited[d] <= 1'b0;
						moved[d]   <= 1'b0;
						// its sector is being moved, or begins to be in this clock
						if (((m != M_IDLE) && (m != M_END) && (cur == d)) || ((m == M_IDLE) && any && (pick == d)))
							dropped[d] <= 1'b1;
					end
					4'o01: begin
						echo[d] <= 1'b0;
						case (i_a[11:9])
							3'd0: reserved[d] <= 1'b0;
							3'd1: begin
								reserved[d] <= 1'b1;
								head[d]     <= 4'd0;
							end
							3'd3: cylinder[d] <= 10'd0;
							// the sector under the heads; the error flags
							3'd5, 3'd6: response[d] <= 16'd0;
							// the cylinder; the head group with "reserved" and
							// "600 Mbyte unit"; the cylinders still to cross,
							// complemented; the interlocks
							3'd7:
							case (i_a[8:0])
								9'd0:    response[d] <= {6'b0, cylinder[d]};
								9'd1:    response[d] <= reserved[d] ? {9'b0, 2'b11, 1'b0, head[d]} : 16'd0;
								9'd2:    response[d] <= 16'o1777;
								default: response[d] <= 16'd0;
							endcase
							default: ;
						endcase
						o_busy[d] <= 1'b1;
						o_done[d] <= 1'b0;
						settle[d] <= SETTLE[23:0];
					end
					4'o02, 4'o03: begin
						o_busy[d]     <= 1'b1;
						o_done[d]     <= 1'b0;
						asked[d]      <= 1'b1;
						awaited[d]    <= i_real && !echo[d];
						moved[d]      <= 1'b0;
						want[d]       <= (i_a[4:0] > 5'd17) ? 5'd0 : i_a[4:0];
						ask_write[d]  <= (i_function == 4'o03);
						ask_zero[d]   <= (i_function == 4'o03) && (i_a[7:5] != 3'd0) && !echo[d];
						ask_echo[d]   <= echo[d];
						ask_sector[d] <= sector;
					end
					4'o04: begin
						reserved[d] <= 1'b1;
						head[d]     <= i_a[3:0];
					end
					// the sector identifier read on arrival: the cylinder
					// shifted left 5
					4'o05: begin
						cylinder[d] <= i_a[9:0];
						response[d] <= {1'b0, i_a[9:0], 5'b0};
						o_busy[d]   <= 1'b1;
						o_done[d]   <= 1'b0;
						settle[d]   <= (i_real && (seek_span != 10'd0)) ? seek_time : SETTLE[23:0];
					end
					4'o10:   o_data <= lma[d];
					4'o11:   o_data <= response[d];
					4'o14:   lma[d] <= {i_a[15:2], 2'b00};
					4'o15:   response[d] <= i_a;
					default: ;
				endcase
		end
	end

endmodule

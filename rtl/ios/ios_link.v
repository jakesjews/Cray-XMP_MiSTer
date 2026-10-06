// The MIOP's channel pair to the mainframe: channel 20 octal, CIA, takes
// parcels from the mainframe's output channel 11, and channel 21, COA, sends
// parcels to its input channel 10 and holds the lines that master clear the
// mainframe (HR-0030 pages 7-15 to 7-21).  The model is in
// tools/crates/ios/src/system.rs (`LowSpeed`, `Side::link`).
//
// Each channel has a Local Memory address and a count of parcels:
//
//   0   clear Busy and Done, abandon the transfer
//   1   enter the Local Memory address and start
//   2   enter the parcel count
//   4   COA: the external control lines, held until the next function 4:
//       bit 15 CPU Master Clear, bit 14 I/O Master Clear, bit 9 hold the
//       Disconnect.  CIA: forget a Ready that waits.
//   10  read the Local Memory address: one more than the last parcel moved
//   11  read the status: no errors
//
// A parcel crosses with Ready and is answered with Resume; the sender ends
// with Disconnect.  Ready, Resume and Disconnect are pulses of one clock and
// the data lines hold a parcel until its Resume, as in rtl/cray/xmp_channels.v
// on the other end.
//
// CIA stores a parcel as it comes and is Done when its count is used up or
// the mainframe disconnects.  A Ready that comes while CIA is idle waits, and
// that parcel is the first of the next transfer.  COA sends its count of
// parcels and then Disconnect; the mainframe's channel holds a Ready that
// finds it stopped, so COA is Busy until the program there takes the data.

module ios_link (
	input wire clk,
	input wire rst,        // Master Clear of the MIOP
	input wire i_power_on, // the machine is switched on or reset as a whole

	input  wire        i_in,        // one clock: a function for CIA
	input  wire        i_out,       // one clock: a function for COA
	input  wire [ 3:0] i_function,
	input  wire [15:0] i_a,
	output reg  [15:0] o_data,      // in the clock after a function that reads
	output reg  [ 1:0] o_busy,      // bit 0 CIA, bit 1 COA
	output reg  [ 1:0] o_done,

	// Local Memory of the MIOP
	output reg         o_dma_req,
	output reg         o_dma_we,
	output reg  [15:0] o_dma_addr,
	output reg  [15:0] o_dma_wdata,
	input  wire        i_dma_ack,
	input  wire [15:0] i_dma_rdata,

	// from the mainframe's output channel
	input  wire        i_ready,
	input  wire [15:0] i_parcel,
	output reg         o_resume,
	input  wire        i_disconnect,

	// to the mainframe's input channel
	output reg         o_ready,
	output reg  [15:0] o_parcel,
	input  wire        i_resume,
	output reg         o_disconnect,

	// the external control lines
	output wire o_cpu_master_clear,
	output wire o_io_master_clear
);

	reg [15:0] in_addr, out_addr;
	reg [16:0] in_left, out_left;  // parcels still to move
	reg [15:0] in_count, out_count;  // as entered; zero is 65,536
	reg clear_cpu, clear_io, hold;  // the external control lines: bits 15, 14 and 9
	reg waiting;  // a Ready from the mainframe that has not been answered
	reg ended;  // the mainframe has disconnected

	assign o_cpu_master_clear = clear_cpu;
	assign o_io_master_clear  = clear_io;

	localparam I_IDLE = 2'd0, I_WAIT = 2'd1, I_PUT = 2'd2;
	localparam O_IDLE = 3'd0, O_GET = 3'd1, O_GOT = 3'd2, O_SENT = 3'd3;
	reg [1:0] in_state;
	reg [2:0] out_state;
	// Local Memory serves one of the two at a time
	reg       put;  // the request under way is CIA's store

	always @(posedge clk) begin
		o_data       <= 16'd0;
		o_resume     <= 1'b0;
		o_ready      <= 1'b0;
		o_disconnect <= 1'b0;
		if (rst) begin
			o_busy    <= 2'b0;
			o_done    <= 2'b0;
			in_state  <= I_IDLE;
			out_state <= O_IDLE;
			in_count  <= 16'd0;
			out_count <= 16'd0;
			waiting   <= 1'b0;
			ended     <= 1'b0;
			o_dma_req <= 1'b0;
			put       <= 1'b0;
			// the lines to the mainframe keep what they carry
		end else begin
			if (i_ready) waiting <= 1'b1;
			if (i_disconnect) ended <= 1'b1;
			// I/O Master Clear stops the mainframe's channels: nothing waits there
			if (clear_io) begin
				waiting <= 1'b0;
				ended   <= 1'b0;
			end

			// ---- CIA
			case (in_state)
				I_WAIT:
				if (waiting && !o_dma_req) begin
					o_dma_req   <= 1'b1;
					o_dma_we    <= 1'b1;
					o_dma_addr  <= in_addr;
					o_dma_wdata <= i_parcel;
					put         <= 1'b1;
					in_state    <= I_PUT;
				end else if (ended && !waiting) begin
					ended     <= 1'b0;
					o_busy[0] <= 1'b0;
					o_done[0] <= 1'b1;
					in_state  <= I_IDLE;
				end
				I_PUT:
				if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					put       <= 1'b0;
					waiting   <= 1'b0;
					o_resume  <= 1'b1;
					in_addr   <= in_addr + 16'd1;
					in_left   <= in_left - 17'd1;
					if (in_left == 17'd1) begin
						o_busy[0] <= 1'b0;
						o_done[0] <= 1'b1;
						in_state  <= I_IDLE;
					end else in_state <= I_WAIT;
				end
				default: ;
			endcase

			// ---- COA
			case (out_state)
				// a store of CIA's goes first
				O_GET:
				if (!o_dma_req && !((in_state == I_WAIT) && waiting)) begin
					o_dma_req  <= 1'b1;
					o_dma_we   <= 1'b0;
					o_dma_addr <= out_addr;
				end else if (i_dma_ack && !put) begin
					o_dma_req <= 1'b0;
					out_state <= O_GOT;
				end
				O_GOT: begin
					o_parcel  <= i_dma_rdata;
					o_ready   <= 1'b1;
					out_state <= O_SENT;
				end
				O_SENT:
				if (i_resume) begin
					out_addr <= out_addr + 16'd1;
					out_left <= out_left - 17'd1;
					if (out_left == 17'd1) begin
						o_disconnect <= !hold;
						o_busy[1]    <= 1'b0;
						o_done[1]    <= 1'b1;
						out_state    <= O_IDLE;
					end else out_state <= O_GET;
				end
				default: ;
			endcase

			// ---- functions: these come last and win
			if (i_in)
				case (i_function)
					4'o00: begin
						o_busy[0] <= 1'b0;
						o_done[0] <= 1'b0;
						in_state  <= I_IDLE;
						if (put) begin
							o_dma_req <= 1'b0;
							put       <= 1'b0;
						end
					end
					// a Disconnect from before belongs to what was sent before
					4'o01: begin
						in_addr   <= i_a;
						in_left   <= (in_count == 16'd0) ? 17'd65536 : {1'b0, in_count};
						ended     <= 1'b0;
						o_busy[0] <= 1'b1;
						o_done[0] <= 1'b0;
						in_state  <= I_WAIT;
					end
					4'o02:   in_count <= i_a;
					4'o04:   waiting <= 1'b0;
					4'o10:   o_data <= in_addr;
					default: ;
				endcase
			if (i_out)
				case (i_function)
					4'o00: begin
						o_busy[1] <= 1'b0;
						o_done[1] <= 1'b0;
						out_state <= O_IDLE;
						if (o_dma_req && !put) o_dma_req <= 1'b0;
					end
					4'o01: begin
						out_addr  <= i_a;
						out_left  <= (out_count == 16'd0) ? 17'd65536 : {1'b0, out_count};
						o_busy[1] <= 1'b1;
						o_done[1] <= 1'b0;
						out_state <= O_GET;
					end
					4'o02:   out_count <= i_a;
					4'o04:   {clear_cpu, clear_io, hold} <= {i_a[15], i_a[14], i_a[9]};
					4'o10:   o_data <= out_addr;
					default: ;
				endcase
		end

		// CPU Master Clear holds the mainframe from power-on.  A Master Clear of
		// the MIOP alone leaves the lines as they are, so that the kernel can be
		// started again under a running mainframe.
		if (i_power_on) {clear_cpu, clear_io, hold} <= 3'b100;
	end

	initial {clear_cpu, clear_io, hold} = 3'b100;

endmodule

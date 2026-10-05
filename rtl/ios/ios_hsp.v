// The 100 Mbyte channel pair of the BIOP into central memory: channel 14
// octal, HIA, reads central memory into Local Memory and channel 15, HOA,
// writes Local Memory to central memory (HR-0030 pages 7-21 to 7-36).  The
// model is `HighSpeed` in tools/crates/ios/src/devices.rs.
//
// The mainframe has no part in a transfer: the channel reaches central memory
// by a port of its own, at absolute addresses.  Each channel has a Local
// Memory address (the low two bits are zero), a central memory address
// entered in two parts, and takes the block length in words with the function
// that starts it; a length of zero is 16,384 words.
//
//   0  clear Busy and Done     1  Local Memory address
//   2  central memory address bits 23 to 9    3  bits 8 to 0
//   4  (channel 14) read central memory       5  (channel 15) write it
//
// A word is four parcels of Local Memory, the one at the lowest address in
// bits 63 to 48.  One transfer runs at a time; the other channel's waits.

module ios_hsp (
	input wire clk,
	input wire rst,  // Master Clear of the BIOP

	input  wire        i_in,        // one clock: a function for channel 14
	input  wire        i_out,       // one clock: a function for channel 15
	input  wire [ 3:0] i_function,
	input  wire [15:0] i_a,
	output reg  [ 1:0] o_busy,      // bit 0 channel 14, bit 1 channel 15
	output reg  [ 1:0] o_done,

	// Local Memory of the BIOP
	output reg         o_dma_req,
	output reg         o_dma_we,
	output reg  [15:0] o_dma_addr,
	output reg  [15:0] o_dma_wdata,
	input  wire        i_dma_ack,
	input  wire [15:0] i_dma_rdata,

	// central memory: one word a request, held until its acknowledge
	output reg         o_cm_req,
	output reg         o_cm_we,
	output reg  [21:0] o_cm_addr,
	output reg  [63:0] o_cm_wdata,
	input  wire        i_cm_ack,
	input  wire [63:0] i_cm_rdata
);

	reg [15:0] lma                                       [0:1];  // Local Memory address
	reg [23:0] cma                                       [0:1];
	reg [14:0] left                                      [0:1];  // words still to move
	reg [ 1:0] waiting;  // a transfer that has not begun

	localparam H_IDLE = 3'd0, H_WORD = 3'd1, H_PUT = 3'd2, H_GET = 3'd3, H_GOT = 3'd4, H_NEXT = 3'd5;
	reg [ 2:0] h;
	reg        which;  // the channel at work: 0 reads central memory
	reg [ 1:0] n;  // parcel of the word
	reg [63:0] word;
	reg        abandoned;  // function 0 came for the channel at work

	wire ch = i_out;  // of a function
	wire strobe = i_in || i_out;

	always @(posedge clk) begin
		if (rst) begin
			o_busy    <= 2'b0;
			o_done    <= 2'b0;
			waiting   <= 2'b0;
			h         <= H_IDLE;
			abandoned <= 1'b0;
			o_dma_req <= 1'b0;
			o_cm_req  <= 1'b0;
		end else begin
			case (h)
				H_IDLE:
				if (waiting != 2'b0) begin
					which                <= !waiting[0];
					waiting[!waiting[0]] <= 1'b0;
					abandoned            <= 1'b0;
					n                    <= 2'd0;
					h                    <= waiting[0] ? H_WORD : H_GET;
					// reading: ask for the first word
					if (waiting[0]) begin
						o_cm_req  <= 1'b1;
						o_cm_we   <= 1'b0;
						o_cm_addr <= cma[0][21:0];
					end
				end

				// central memory answers
				H_WORD:
				if (i_cm_ack) begin
					o_cm_req   <= 1'b0;
					cma[which] <= cma[which] + 24'd1;
					word       <= i_cm_rdata;
					n          <= 2'd0;
					h          <= which ? H_NEXT : H_PUT;
				end

				// a word from central memory goes to Local Memory, four parcels
				H_PUT:
				if (!o_dma_req) begin
					o_dma_req   <= 1'b1;
					o_dma_we    <= 1'b1;
					o_dma_addr  <= lma[0];
					o_dma_wdata <= word[63:48];
				end else if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					lma[0]    <= lma[0] + 16'd1;
					word      <= {word[47:0], 16'b0};
					n         <= n + 2'd1;
					if (n == 2'd3) h <= H_NEXT;
				end

				// four parcels of Local Memory make a word for central memory
				H_GET:
				if (!o_dma_req) begin
					o_dma_req  <= 1'b1;
					o_dma_we   <= 1'b0;
					o_dma_addr <= lma[1];
				end else if (i_dma_ack) begin
					o_dma_req <= 1'b0;
					lma[1]    <= lma[1] + 16'd1;
					h         <= H_GOT;
				end
				H_GOT: begin
					word <= {word[47:0], i_dma_rdata};
					n    <= n + 2'd1;
					if (n == 2'd3) begin
						o_cm_req   <= 1'b1;
						o_cm_we    <= 1'b1;
						o_cm_addr  <= cma[1][21:0];
						o_cm_wdata <= {word[47:0], i_dma_rdata};
						h          <= H_WORD;
					end else h <= H_GET;
				end

				// the next word, or the end of the block
				H_NEXT:
				if ((left[which] == 15'd1) || abandoned) begin
					if (!abandoned) begin
						o_busy[which] <= 1'b0;
						o_done[which] <= 1'b1;
					end
					h <= H_IDLE;
				end else begin
					left[which] <= left[which] - 15'd1;
					n           <= 2'd0;
					if (which) h <= H_GET;
					else begin
						o_cm_req  <= 1'b1;
						o_cm_we   <= 1'b0;
						o_cm_addr <= cma[0][21:0];
						h         <= H_WORD;
					end
				end

				default: h <= H_IDLE;
			endcase

			// ---- functions: these come last and win
			if (strobe)
				case (i_function)
					4'o00: begin
						o_busy[ch]  <= 1'b0;
						o_done[ch]  <= 1'b0;
						waiting[ch] <= 1'b0;
						if ((h != H_IDLE) && (which == ch)) abandoned <= 1'b1;
					end
					4'o01:   lma[ch] <= {i_a[15:2], 2'b00};
					4'o02:   cma[ch][23:9] <= i_a[14:0];
					4'o03:   cma[ch][8:0] <= i_a[8:0];
					4'o04, 4'o05:
					if (ch == (i_function == 4'o05)) begin
						left[ch]    <= (i_a[13:0] == 14'd0) ? 15'd16384 : {1'b0, i_a[13:0]};
						o_busy[ch]  <= 1'b1;
						o_done[ch]  <= 1'b0;
						waiting[ch] <= 1'b1;
					end
					default: ;
				endcase
		end
	end

endmodule

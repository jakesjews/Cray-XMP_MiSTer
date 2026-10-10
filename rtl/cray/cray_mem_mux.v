// Memory port multiplexer for the Cray CPU.
//
// N requesters share one memory port that takes requests in a stream and
// answers reads in order (rtl/mister/ddr3_ports.sv has the contract).
// A requester presents req with we, len, addr and wdata; the request is
// taken in a clock where take is high, and goes out in that clock.  ack is a
// one-clock pulse per word of a read, with rdata in the same clock; the
// words come back in the order the reads were taken.  A write is done with
// its take.
//
// A requester that streams (STREAM[n] = 1) presents its next request in the
// clock after take.  Any other holds its request until the last ack of its
// read, or until the ack a write is given the clock after take, and is
// looked at again from the clock after that ack.
//
// Priority rotates behind the requester taken last, so the instruction
// buffers cannot starve data references or the reverse: a fetch takes its
// turn between two words of a vector store.

module cray_mem_mux #(
	parameter         N      = 2,
	parameter [N-1:0] STREAM = {N{1'b0}}
) (
	input wire clk,
	input wire rst,

	input  wire [   N-1:0] i_req,
	input  wire [   N-1:0] i_we,
	input  wire [ N*7-1:0] i_len,
	input  wire [N*22-1:0] i_addr,
	input  wire [N*64-1:0] i_wdata,
	output wire [   N-1:0] o_take,
	output reg  [   N-1:0] o_ack,
	output reg  [    63:0] o_rdata,

	output wire        o_mem_req,
	output reg         o_mem_we,
	output reg  [ 6:0] o_mem_len,
	output reg  [21:0] o_mem_addr,
	output reg  [63:0] o_mem_wdata,
	input  wire        i_mem_take,
	input  wire        i_mem_ack,
	input  wire [63:0] i_mem_rdata
);

	localparam RF = 16;  // reads whose words are still to come

	// rotating priority: requesters above the one taken last go first
	reg  [N-1:0] prio;
	reg  [N-1:0] mask;  // holds its request, and has been taken
	wire [N-1:0] ready = i_req & ~mask;
	wire [N-1:0] above = ready & prio;
	wire [N-1:0] pick_from = (|above) ? above : ready;

	reg     [N-1:0] pick;  // lowest set bit of pick_from
	integer         p;
	always @(*) begin
		pick = {N{1'b0}};
		for (p = N - 1; p >= 0; p = p - 1) if (pick_from[p]) pick = {{(N - 1) {1'b0}}, 1'b1} << p;
	end

	// what the picked requester asks for, gathered with its bit
	integer n;
	always @(*) begin
		o_mem_we    = 1'b0;
		o_mem_len   = 7'd0;
		o_mem_addr  = 22'd0;
		o_mem_wdata = 64'd0;
		for (n = 0; n < N; n = n + 1) begin
			o_mem_we    = o_mem_we | (pick[n] & i_we[n]);
			o_mem_len   = o_mem_len | ({7{pick[n]}} & i_len[n*7+:7]);
			o_mem_addr  = o_mem_addr | ({22{pick[n]}} & i_addr[n*22+:22]);
			o_mem_wdata = o_mem_wdata | ({64{pick[n]}} & i_wdata[n*64+:64]);
		end
	end

	// the reads taken and not yet answered, in order
	reg [N-1:0] rf_owner[0:RF-1];
	reg [  6:0] rf_len  [0:RF-1];
	reg [$clog2(RF)-1:0] rf_wr, rf_rd;
	reg  [$clog2(RF):0] rf_n;
	reg  [         6:0] rf_cnt;
	wire                rf_room = (rf_n != RF[$clog2(RF):0]);

	assign o_mem_req = (|pick) && (o_mem_we || rf_room);
	wire taken = o_mem_req && i_mem_take;
	assign o_take = pick & {N{taken}};

	wire         rf_pop = i_mem_ack && (rf_n != 0) && (rf_cnt + 7'd1 == rf_len[rf_rd]);
	reg  [N-1:0] ack_last;

	always @(posedge clk) begin
		o_ack    <= {N{1'b0}};
		ack_last <= {N{1'b0}};

		if (rst) begin
			prio   <= {N{1'b0}};
			mask   <= {N{1'b0}};
			rf_wr  <= {$clog2(RF) {1'b0}};
			rf_rd  <= {$clog2(RF) {1'b0}};
			rf_n   <= {($clog2(RF) + 1) {1'b0}};
			rf_cnt <= 7'd0;
		end else begin
			if (taken) begin
				for (n = 0; n < N; n = n + 1) if (pick[n]) prio <= ~((({{(N - 1) {1'b0}}, 1'b1} << n) << 1) - 1'b1);
				if (!o_mem_we) begin
					rf_owner[rf_wr] <= pick;
					rf_len[rf_wr]   <= o_mem_len;
					rf_wr           <= rf_wr + 1'd1;
				end
			end
			rf_n <= rf_n + {{$clog2(RF) {1'b0}}, taken && !o_mem_we} - {{$clog2(RF) {1'b0}}, rf_pop};

			// a word read: to the requester whose read is the oldest
			if (i_mem_ack && (rf_n != 0)) begin
				o_rdata <= i_mem_rdata;
				o_ack   <= rf_owner[rf_rd];
				rf_cnt  <= rf_pop ? 7'd0 : rf_cnt + 7'd1;
				if (rf_pop) begin
					rf_rd    <= rf_rd + 1'd1;
					ack_last <= rf_owner[rf_rd];
				end
			end

			// requesters that hold their request until its acknowledge
			for (n = 0; n < N; n = n + 1)
			if (!STREAM[n]) begin
				if (o_take[n]) begin
					mask[n] <= 1'b1;
					if (i_we[n]) begin
						o_ack[n]    <= 1'b1;
						ack_last[n] <= 1'b1;
					end
				end
				if (o_ack[n] && ack_last[n]) mask[n] <= 1'b0;
			end
		end
	end

endmodule

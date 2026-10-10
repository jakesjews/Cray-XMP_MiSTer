// The DE10-Nano's HPS DDR3 shared by several users, through the MiSTer DDRAM
// port.
//
// A user's address counts 64-bit words from physical 0x3000_0000, where Linux
// sees the same memory through /dev/mem and where the MiSTer menu loads files
// whose CONF_STR entry gives an address.  Bytes are reversed so a file of
// big-endian 64-bit words lands with its first byte in rdata[63:56].
//
// Port contract, for each user:
//   - a request is req with we, addr, len and wdata.  It is taken in a clock
//     where take is high, and is then carried out in its turn: requests go
//     to the DDR3 in the order they were taken, and the words of reads come
//     back in that order.
//   - a read is of len words from addr, 1 to 64, which the bridge delivers
//     one a clock; a write is of one word.
//   - ack is a registered one-clock pulse per read word.  rdata, which all
//     users share, is valid only during ack.
//   - a user that streams (STREAM[n] = 1) presents its next request in the
//     clock after take, and hears nothing more of a write once it is taken.
//   - any other user holds req, we, addr, len and wdata until the ack of a
//     write, which comes the clock after take, or the last ack of a read;
//     its request is looked at again from the clock after that ack.
//   - while user_rst[n] is high the user's requests are not taken, and the
//     words of its reads still to come are dropped: the CPU can be reset
//     while this port runs on.
//
// Taken requests wait in a queue; its head is presented to the bridge until
// the bridge is not busy.  A read is only presented when there is room to
// remember whose words come back.  The users take turns: the one served
// last is the last to be looked at again.
module ddr3_ports #(
	parameter         N      = 2,
	parameter [N-1:0] STREAM = {N{1'b0}}
) (
	input wire clk,
	input wire reset,

	input  wire [   N-1:0] req,
	input  wire [   N-1:0] we,
	input  wire [ 7*N-1:0] len,
	input  wire [25*N-1:0] addr,
	input  wire [64*N-1:0] wdata,
	input  wire [   N-1:0] user_rst,
	output wire [   N-1:0] take,
	output reg  [   N-1:0] ack,
	output reg  [    63:0] rdata,

	output wire        DDRAM_CLK,
	input  wire        DDRAM_BUSY,
	output wire [ 7:0] DDRAM_BURSTCNT,
	output wire [28:0] DDRAM_ADDR,
	input  wire [63:0] DDRAM_DOUT,
	input  wire        DDRAM_DOUT_READY,
	output wire        DDRAM_RD,
	output wire [63:0] DDRAM_DIN,
	output wire [ 7:0] DDRAM_BE,
	output wire        DDRAM_WE
);

	localparam W  = (N > 1) ? $clog2(N) : 1;
	localparam CF = 4;                        // requests waiting behind the one presented
	localparam RF = 8;                        // reads whose words are still to come

	function [63:0] bswap;
		input [63:0] d;
		bswap = {d[7:0], d[15:8], d[23:16], d[31:24], d[39:32], d[47:40], d[55:48], d[63:56]};
	endfunction

	//-----------------------------------------------------------------
	// Which request is taken
	//-----------------------------------------------------------------
	// The user to serve is the first after the one served last that asks and
	// may be served.  What depends on the requests is one bit a user, and the
	// address and the data are gathered with those bits: nothing is indexed
	// with a number that has to be worked out from the requests first.
	reg  [N-1:0] mask;  // a user that holds its request until its acknowledge, and has been taken
	reg  [W-1:0] last;  // the user served last
	wire [N-1:0] ready = req & ~mask & ~user_rst;
	reg  [N-1:0] first;
	reg  [W-1:0] pick;
	reg  [  6:0] pick_len;
	reg  [ 24:0] pick_addr;
	reg  [ 63:0] pick_wdata;
	reg          sooner;
	wire         pick_we = |(first & we);
	integer k, n;
	// how many places after the one served last a user comes, going round
	function automatic [W:0] place(input integer user, input [W-1:0] of);
		integer d;
		begin
			d     = user - 1 - {{(32 - W) {1'b0}}, of};
			place = (d < 0) ? d[W:0] + N[W:0] : d[W:0];
		end
	endfunction
	always @(*) begin
		pick       = last;
		pick_len   = 7'd0;
		pick_addr  = 25'd0;
		pick_wdata = 64'd0;
		for (n = 0; n < N; n = n + 1) begin
			sooner = 1'b0;
			for (k = 0; k < N; k = k + 1) if ((k != n) && (place(k, last) < place(n, last))) sooner = sooner | ready[k];
			first[n] = ready[n] && !sooner;
			if (first[n]) pick = n[W-1:0];
			pick_len   = pick_len | ({7{first[n]}} & len[7*n+:7]);
			pick_addr  = pick_addr | ({25{first[n]}} & addr[25*n+:25]);
			pick_wdata = pick_wdata | ({64{first[n]}} & wdata[64*n+:64]);
		end
	end

	//-----------------------------------------------------------------
	// The queue of taken requests, and the one presented to the bridge
	//-----------------------------------------------------------------
	localparam CW = 1 + W + 7 + 25 + 64;
	(* ramstyle = "MLAB, no_rw_check" *)reg [CW-1:0] cf      [0:CF-1];
	reg [ W-1:0] cf_owner[0:CF-1];  // beside the queue, so a reset user's entries can be marked
	reg [CF-1:0] cf_dead;
	reg [$clog2(CF)-1:0] cf_wr, cf_rd;
	reg  [$clog2(CF):0] cf_n;
	reg  [         8:0] drain;  // after reset, let any abandoned burst finish first
	wire                cf_room = (cf_n != CF[$clog2(CF):0]);
	wire                any_take = |first && cf_room && (drain == 9'd0);
	assign take = first & {N{cf_room && (drain == 9'd0)}};

	reg           h_valid;  // the request presented to the bridge
	reg           h_we;
	reg  [ W-1:0] h_owner;
	reg  [   6:0] h_len;
	reg  [  24:0] h_addr;
	reg  [  63:0] h_din;
	wire [CW-1:0] cf_head = cf[cf_rd];
	wire          cf_head_we = cf_head[CW-1];

	assign DDRAM_CLK      = clk;
	assign DDRAM_ADDR     = {4'b0011, h_addr};  // 0x3000_0000 + word * 8
	assign DDRAM_BURSTCNT = h_we ? 8'd1 : {1'b0, h_len};
	assign DDRAM_BE       = 8'hFF;
	assign DDRAM_RD       = h_valid && !h_we;
	assign DDRAM_WE       = h_valid && h_we;
	assign DDRAM_DIN      = h_din;

	// The reads whose words are still to come: whose, how many, and whether
	// the words are wanted at all.
	reg [ W-1:0] rf_owner[0:RF-1];
	reg [   6:0] rf_len  [0:RF-1];
	reg [RF-1:0] rf_dead;
	reg [$clog2(RF)-1:0] rf_wr, rf_rd;
	reg [$clog2(RF):0] rf_n;
	reg [         6:0] rf_cnt;  // words of the head's read delivered

	// the last word of a read: its user may ask again from the clock after
	wire         rf_pop = DDRAM_DOUT_READY && (rf_n != 0) && (rf_cnt + 7'd1 == rf_len[rf_rd]);
	reg  [N-1:0] ack_last;

	wire h_go = h_valid && !DDRAM_BUSY;  // the bridge takes the head this clock
	wire h_free = !h_valid || h_go;
	// a read behind a read being taken now needs its own place
	wire rf_room_nxt = (rf_n + {{$clog2(RF) {1'b0}}, (h_go && !h_we)}) < RF[$clog2(RF):0];
	wire cf_load = h_free && (cf_n != 0) && (cf_head_we || cf_dead[cf_rd] || rf_room_nxt);
	wire by_load = h_free && (cf_n == 0) && any_take && (pick_we || rf_room_nxt);

	integer e;
	always @(posedge clk) begin
		ack      <= '0;
		ack_last <= '0;

		if (reset) begin
			h_valid <= 1'b0;
			cf_wr   <= '0;
			cf_rd   <= '0;
			cf_n    <= '0;
			cf_dead <= '0;
			rf_wr   <= '0;
			rf_rd   <= '0;
			rf_n    <= '0;
			rf_cnt  <= 7'd0;
			rf_dead <= '0;
			drain   <= 9'd511;
			mask    <= '0;
			last    <= '0;
		end else begin
			if (drain != 9'd0) drain <= drain - 9'd1;

			// taken: into the queue, or straight to the bridge when nothing waits
			if (any_take) begin
				last <= pick;
				if (!by_load) begin
					cf[cf_wr]       <= {pick_we, pick, pick_len, pick_addr, bswap(pick_wdata)};
					cf_owner[cf_wr] <= pick;
					cf_dead[cf_wr]  <= user_rst[pick];
					cf_wr           <= cf_wr + 1'd1;
				end
			end
			if (cf_load) cf_rd <= cf_rd + 1'd1;
			cf_n <= cf_n + {{$clog2(CF) {1'b0}}, any_take && !by_load} - {{$clog2(CF) {1'b0}}, cf_load};

			// the request presented
			if (cf_load) begin
				if (cf_dead[cf_rd]) h_valid <= 1'b0;  // of a user since reset: not made
				else begin
					h_valid                               <= 1'b1;
					{h_we, h_owner, h_len, h_addr, h_din} <= cf_head;
				end
			end else if (by_load) begin
				h_valid <= 1'b1;
				h_we    <= pick_we;
				h_owner <= pick;
				h_len   <= pick_len;
				h_addr  <= pick_addr;
				h_din   <= bswap(pick_wdata);
			end else if (h_go) h_valid <= 1'b0;

			// a read taken by the bridge: its words are expected
			if (h_go && !h_we) begin
				rf_owner[rf_wr] <= h_owner;
				rf_len[rf_wr]   <= h_len;
				rf_dead[rf_wr]  <= user_rst[h_owner];
				rf_wr           <= rf_wr + 1'd1;
			end
			rf_n <= rf_n + {{$clog2(RF) {1'b0}}, h_go && !h_we} - {{$clog2(RF) {1'b0}}, rf_pop};

			// a word read: to its user, unless the user has been reset since
			if (DDRAM_DOUT_READY && (rf_n != 0)) begin
				rdata <= bswap(DDRAM_DOUT);
				if (!rf_dead[rf_rd]) begin
					ack[rf_owner[rf_rd]]      <= 1'b1;
					ack_last[rf_owner[rf_rd]] <= rf_pop;
				end
				rf_cnt <= rf_pop ? 7'd0 : rf_cnt + 7'd1;
				if (rf_pop) rf_rd <= rf_rd + 1'd1;
			end

			// a user's reset: what it still has here is not wanted any more
			for (e = 0; e < CF; e = e + 1) if (user_rst[cf_owner[e]]) cf_dead[e] <= 1'b1;
			for (e = 0; e < RF; e = e + 1) if (user_rst[rf_owner[e]]) rf_dead[e] <= 1'b1;

			// users that hold their request until its acknowledge: a write is
			// acknowledged the clock after it is taken
			for (n = 0; n < N; n = n + 1)
			if (!STREAM[n]) begin
				if (take[n]) begin
					mask[n] <= 1'b1;
					if (we[n]) begin
						ack[n]      <= 1'b1;
						ack_last[n] <= 1'b1;
					end
				end
				if (ack[n] && ack_last[n]) mask[n] <= 1'b0;
				if (user_rst[n]) mask[n] <= 1'b0;
			end
		end
	end

endmodule

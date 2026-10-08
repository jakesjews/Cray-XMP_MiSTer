// Memory port multiplexer for the Cray CPU.
//
// N requesters share one request/acknowledge memory port.  A requester raises
// req with we, burst, addr and wdata and holds them until its last ack; ack is a
// one-clock pulse per word with rdata valid in the same clock.  A burst is a
// 16-word read (half an instruction buffer, or a line of a vector load).  The
// grant is held for the whole request.  Priority rotates so the instruction
// buffers cannot starve data references or the reverse.
//
// The requester-side timing is the one the original mem_arb gave: ack arrives
// the clock after the memory answers, and the next request is looked at the
// clock after that.
//
// A requester that raises seq works ahead instead.  take tells it, in the
// clock its word is latched here, that it may present its next word; that
// word is then latched in the very clock memory answers the one before, so
// consecutive words follow each other without the idle clocks in between.
// ack still reports each word as it completes.

module cray_mem_mux #(
	parameter N = 2
) (
	input wire clk,
	input wire rst,

	input  wire [   N-1:0] i_req,
	input  wire [   N-1:0] i_we,
	input  wire [   N-1:0] i_burst,
	input  wire [   N-1:0] i_seq,
	output wire [   N-1:0] o_take,
	input  wire [N*22-1:0] i_addr,
	input  wire [N*64-1:0] i_wdata,
	output reg  [   N-1:0] o_ack,
	output reg  [    63:0] o_rdata,

	output wire        o_mem_req,
	output reg         o_mem_we,
	output reg         o_mem_burst,
	output reg  [21:0] o_mem_addr,
	output reg  [63:0] o_mem_wdata,
	input  wire        i_mem_ack,
	input  wire [63:0] i_mem_rdata
);

	localparam IDLE = 2'd0, BUSY = 2'd1, DONE = 2'd2;

	reg [  1:0] state;
	reg [N-1:0] grant;
	reg [  4:0] left;

	// rotating priority: requesters above the last grant go first
	reg  [N-1:0] mask;
	wire [N-1:0] masked = i_req & mask;
	wire [N-1:0] pick_from = (|masked) ? masked : i_req;

	reg     [N-1:0] pick;  // lowest set bit of pick_from
	integer         p;
	always @(*) begin
		pick = {N{1'b0}};
		for (p = N - 1; p >= 0; p = p - 1) if (pick_from[p]) pick = {{(N - 1) {1'b0}}, 1'b1} << p;
	end

	assign o_mem_req = (state == BUSY);

	// the granted requester has its next word waiting as memory answers this one
	wire again = (state == BUSY) && i_mem_ack && (left == 5'd1) && (|(grant & i_seq & i_req));
	assign o_take = (state == IDLE) ? pick : (again ? grant : {N{1'b0}});

	integer n;
	always @(posedge clk) begin
		o_ack <= {N{1'b0}};

		if (rst) begin
			state <= IDLE;
			grant <= {N{1'b0}};
			mask  <= {N{1'b0}};
		end else
			case (state)
				IDLE:
				if (|i_req) begin
					grant <= pick;
					for (n = 0; n < N; n = n + 1) begin
						if (pick[n]) begin
							o_mem_addr  <= i_addr[n*22+:22];
							o_mem_wdata <= i_wdata[n*64+:64];
							o_mem_we    <= i_we[n];
							o_mem_burst <= i_burst[n] & ~i_we[n];
							left        <= (i_burst[n] & ~i_we[n]) ? 5'd16 : 5'd1;
							mask        <= ~((({{(N - 1) {1'b0}}, 1'b1} << n) << 1) - 1'b1);
						end
					end
					state <= BUSY;
				end

				BUSY:
				if (i_mem_ack) begin
					o_rdata <= i_mem_rdata;
					o_ack   <= grant;
					left    <= left - 1'd1;
					if (again) begin
						for (n = 0; n < N; n = n + 1) begin
							if (grant[n]) begin
								o_mem_addr  <= i_addr[n*22+:22];
								o_mem_wdata <= i_wdata[n*64+:64];
								o_mem_we    <= i_we[n];
							end
						end
						o_mem_burst <= 1'b0;
						left        <= 5'd1;
					end else if (left == 5'd1) state <= DONE;
				end

				DONE: state <= IDLE;

				default: state <= IDLE;
			endcase
	end

endmodule

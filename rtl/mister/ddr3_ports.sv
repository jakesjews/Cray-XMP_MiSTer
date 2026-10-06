// The DE10-Nano's HPS DDR3 shared by several users, through the MiSTer DDRAM
// port.
//
// A user's address counts 64-bit words from physical 0x3000_0000, where Linux
// sees the same memory through /dev/mem and where the MiSTer menu loads files
// whose CONF_STR entry gives an address.  Bytes are reversed so a file of
// big-endian 64-bit words lands with its first byte in rdata[63:56].
//
// Port contract, for each user (it is that of the CPU's memory port):
//   - req, we, burst, addr and wdata are held until the last ack of the request.
//   - ack is a registered one-clock pulse per word, never in the same clock that
//     req first rises.  rdata, which all users share, is valid only during ack.
//   - burst reads 16 words starting at addr (addr[3:0] must be 0).
//   - a request presented during an ack clock is ignored until the next clock.
//
// The users take turns: the one served last is the last to be looked at again.
module ddr3_ports #(
	parameter N = 2
) (
	input wire clk,
	input wire reset,

	input  wire [   N-1:0] req,
	input  wire [   N-1:0] we,
	input  wire [   N-1:0] burst,
	input  wire [25*N-1:0] addr,
	input  wire [64*N-1:0] wdata,
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

	localparam W = (N > 1) ? $clog2(N) : 1;

	function [63:0] bswap;
		input [63:0] d;
		bswap = {d[7:0], d[15:8], d[23:16], d[31:24], d[39:32], d[47:40], d[55:48], d[63:56]};
	endfunction

	localparam S_IDLE = 2'd0;
	localparam S_CMD  = 2'd1;  // command presented, waiting for the bridge to take it
	localparam S_READ = 2'd2;  // waiting for read data

	reg [1:0] state;
	reg ram_rd, ram_we;
	reg [ 24:0] ram_addr;
	reg [ 63:0] ram_din;
	reg [  7:0] ram_burst;
	reg [  4:0] left;  // words still expected
	reg [  8:0] drain;  // after reset, let any abandoned burst finish first
	reg [W-1:0] owner;  // whose request is being served, and who was served last

	assign DDRAM_CLK      = clk;
	assign DDRAM_ADDR     = {4'b0011, ram_addr};  // 0x3000_0000 + word * 8
	assign DDRAM_BURSTCNT = ram_burst;
	assign DDRAM_BE       = 8'hFF;
	assign DDRAM_RD       = ram_rd;
	assign DDRAM_WE       = ram_we;
	assign DDRAM_DIN      = ram_din;

	// The user to serve is the first after the owner that asks.  What depends
	// on the requests is one bit a user, and the address and the data are
	// gathered with those bits: nothing is indexed with a number that has to
	// be worked out from the requests first.
	reg  [N-1:0] first;
	reg  [W-1:0] pick;
	reg  [ 24:0] pick_addr;
	reg  [ 63:0] pick_wdata;
	reg          sooner;
	wire         any = |req;
	wire         pick_we = |(first & we);
	wire         pick_burst = |(first & burst & ~we);
	integer k, n;
	// how many places after the owner a user comes, going round
	function automatic [W:0] place(input integer user, input [W-1:0] of);
		integer d;
		begin
			d     = user - 1 - {{(32 - W) {1'b0}}, of};
			place = (d < 0) ? d[W:0] + N[W:0] : d[W:0];
		end
	endfunction
	always @(*) begin
		pick       = owner;
		pick_addr  = 25'd0;
		pick_wdata = 64'd0;
		for (n = 0; n < N; n = n + 1) begin
			sooner = 1'b0;
			for (k = 0; k < N; k = k + 1) if ((k != n) && (place(k, owner) < place(n, owner))) sooner = sooner | req[k];
			first[n] = req[n] && !sooner;
			if (first[n]) pick = n[W-1:0];
			pick_addr  = pick_addr | ({25{first[n]}} & addr[25*n+:25]);
			pick_wdata = pick_wdata | ({64{first[n]}} & wdata[64*n+:64]);
		end
	end

	always @(posedge clk) begin
		ack <= '0;

		if (reset) begin
			state  <= S_IDLE;
			ram_rd <= 0;
			ram_we <= 0;
			drain  <= 9'd511;
			owner  <= '0;
		end else
			case (state)
				S_IDLE:
				if (drain != 0) drain <= drain - 1'd1;
				else if (any && ack == '0) begin
					owner     <= pick;
					ram_addr  <= pick_addr;
					ram_din   <= bswap(pick_wdata);
					ram_burst <= pick_burst ? 8'd16 : 8'd1;
					left      <= pick_burst ? 5'd16 : 5'd1;
					ram_we    <= pick_we;
					ram_rd    <= ~pick_we;
					state     <= S_CMD;
				end

				// The strobe stays up until a clock where the bridge is not busy.
				S_CMD:
				if (!DDRAM_BUSY) begin
					ram_rd <= 0;
					ram_we <= 0;
					if (ram_we) begin
						ack[owner] <= 1;
						state      <= S_IDLE;
					end else state <= S_READ;
				end

				S_READ: ;

				default: state <= S_IDLE;
			endcase

		// Read data can arrive from the clock the command is accepted onwards.
		if (!reset && (state == S_CMD || state == S_READ) && !ram_we && DDRAM_DOUT_READY) begin
			rdata      <= bswap(DDRAM_DOUT);
			ack[owner] <= 1;
			left       <= left - 1'd1;
			if (left == 5'd1) state <= S_IDLE;
		end
	end

endmodule

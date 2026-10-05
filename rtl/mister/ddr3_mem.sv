// Cray main memory in the DE10-Nano's HPS DDR3, through the MiSTer DDRAM port.
//
// One 64-bit beat is one Cray word.  Word 0 sits at physical 0x3000_0000, which
// Linux also sees through /dev/mem, and where the MiSTer menu loads images when
// CONF_STR gives that address.  Bytes are reversed so a file of big-endian
// 64-bit words (Cray bit 0 first) lands with Cray bit 0 in rdata[63].
//
// Port contract (shared with the CPU's memory mux):
//   - req, we, burst, addr and wdata are held until the last ack of the request.
//   - ack is a registered one-clock pulse per word, never in the same clock that
//     req first rises.  rdata is valid only during ack.
//   - burst reads 16 words starting at addr (addr[3:0] must be 0).
//   - a request presented during an ack clock is ignored until the next clock.

module ddr3_mem (
	input wire clk,
	input wire reset,

	input  wire        req,
	input  wire        we,
	input  wire        burst,
	input  wire [21:0] addr,
	input  wire [63:0] wdata,
	output reg         ack,
	output reg  [63:0] rdata,

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

	function [63:0] bswap;
		input [63:0] d;
		bswap = {d[7:0], d[15:8], d[23:16], d[31:24], d[39:32], d[47:40], d[55:48], d[63:56]};
	endfunction

	localparam S_IDLE = 2'd0;
	localparam S_CMD  = 2'd1;  // command presented, waiting for the bridge to take it
	localparam S_READ = 2'd2;  // waiting for read data

	reg [1:0] state;
	reg ram_rd, ram_we;
	reg [21:0] ram_addr;
	reg [63:0] ram_din;
	reg [ 7:0] ram_burst;
	reg [ 4:0] left;  // words still expected
	reg [ 8:0] drain;  // after reset, let any abandoned burst finish first

	assign DDRAM_CLK      = clk;
	assign DDRAM_ADDR     = {4'b0011, 3'b000, ram_addr};  // 0x3000_0000 + word * 8
	assign DDRAM_BURSTCNT = ram_burst;
	assign DDRAM_BE       = 8'hFF;
	assign DDRAM_RD       = ram_rd;
	assign DDRAM_WE       = ram_we;
	assign DDRAM_DIN      = ram_din;

	always @(posedge clk) begin
		ack <= 0;

		if (reset) begin
			state  <= S_IDLE;
			ram_rd <= 0;
			ram_we <= 0;
			drain  <= 9'd511;
		end else
			case (state)
				S_IDLE:
				if (drain != 0) drain <= drain - 1'd1;
				else if (req && !ack) begin
					ram_addr  <= addr;
					ram_din   <= bswap(wdata);
					ram_burst <= (burst && !we) ? 8'd16 : 8'd1;
					left      <= (burst && !we) ? 5'd16 : 5'd1;
					ram_we    <= we;
					ram_rd    <= ~we;
					state     <= S_CMD;
				end

				// The strobe stays up until a clock where the bridge is not busy.
				S_CMD:
				if (!DDRAM_BUSY) begin
					ram_rd <= 0;
					ram_we <= 0;
					if (ram_we) begin
						ack   <= 1;
						state <= S_IDLE;
					end else state <= S_READ;
				end

				S_READ: ;

				default: state <= S_IDLE;
			endcase

		// Read data can arrive from the clock the command is accepted onwards.
		if (!reset && (state == S_CMD || state == S_READ) && !ram_we && DDRAM_DOUT_READY) begin
			rdata <= bswap(DDRAM_DOUT);
			ack   <= 1;
			left  <= left - 1'd1;
			if (left == 5'd1) state <= S_IDLE;
		end
	end

endmodule

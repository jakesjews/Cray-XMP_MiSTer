// What the CRAY X-MP core does with its boot file before the machine starts.
//
// The menu loads the file into DDR3 (tools/py/mkboot.py makes it); in 64-bit
// words, the first byte of the file being the high byte of word 0:
//
//   word 0         "XMPBOOT1"
//   word 1         the length of the kernel in bytes
//   word 2         the length of the boot tape in bytes
//   word 3         the check sum of all words from word 8 on
//   words 4 to 7   0
//   words 8 on     the kernel of the I/O Subsystem, filled with zeros to
//                  16,384 words, which is one Local Memory
//   then           the boot tape, a .tap file, filled with zeros to a whole word
//
// The check sum starts at 0 and for each word is turned left by one bit and
// has the word added.
//
// After every reset the file is checked and the kernel is copied to the start
// of Buffer Memory, from where the MIOP loads it when it is dead started: the
// running system changes Buffer Memory, and the file stays as it was, so the
// machine can be started again without loading the file again.

module xmp_boot #(
	parameter [24:0] FILE   = 25'h0800000,  // where the file is in memory, in words
	parameter [24:0] BUFFER = 25'h0400000   // where Buffer Memory is
) (
	input wire clk,
	input wire reset,

	// memory: one word a request, held until its acknowledge
	output reg         o_req,
	output reg         o_we,
	output reg  [24:0] o_addr,
	output reg  [63:0] o_wdata,
	input  wire        i_ack,
	input  wire [63:0] i_rdata,

	output reg         o_done,        // the kernel is in Buffer Memory: the machine may start
	output reg         o_missing,     // there is no boot file in memory
	output reg  [23:0] o_tape_bytes,
	output wire [24:0] o_tape         // where the tape is in memory, in words
);

	localparam [63:0] MAGIC  = "XMPBOOT1";
	localparam [24:0] KERNEL = 25'd8;       // words before the kernel
	localparam [24:0] MEMORY = 25'd16384;   // words of a Local Memory

	assign o_tape = FILE + KERNEL + MEMORY;

	localparam B_MAGIC = 3'd0;
	localparam B_TAPE  = 3'd1;
	localparam B_SUM   = 3'd2;
	localparam B_GET   = 3'd3;  // a word of the file
	localparam B_PUT   = 3'd4;  // into Buffer Memory, if it is of the kernel
	localparam B_STOP  = 3'd5;

	reg [2:0] b;
	reg       gap;  // the clock after an acknowledge, with no request
	reg [21:0] n, words;
	reg [63:0] sum, want;

	// the words of the tape, and with the kernel's
	wire [21:0] tape_words = {1'b0, i_rdata[23:3]} + {21'd0, |i_rdata[2:0]};

	task read;
		input [24:0] a;
		begin
			o_req  <= 1'b1;
			o_we   <= 1'b0;
			o_addr <= a;
		end
	endtask

	always @(posedge clk) begin
		if (reset) begin
			b         <= B_MAGIC;
			gap       <= 1'b0;
			o_req     <= 1'b0;
			o_we      <= 1'b0;
			o_done    <= 1'b0;
			o_missing <= 1'b0;
		end else if (gap) gap <= 1'b0;
		else if (i_ack) begin
			o_req <= 1'b0;
			gap   <= 1'b1;
			case (b)
				B_MAGIC: begin
					b <= B_TAPE;
					if (i_rdata != MAGIC) begin
						o_missing <= 1'b1;
						b         <= B_STOP;
					end
				end
				B_TAPE: begin
					o_tape_bytes <= i_rdata[23:0];
					words        <= tape_words + MEMORY[21:0];
					b            <= B_SUM;
					// a length that is not a tape's: the file is not ours
					if (i_rdata[63:24] != 40'd0) begin
						o_missing <= 1'b1;
						b         <= B_STOP;
					end
				end
				B_SUM: begin
					want <= i_rdata;
					sum  <= 64'd0;
					n    <= 22'd0;
					b    <= B_GET;
				end
				B_GET: begin
					sum     <= {sum[62:0], sum[63]} + i_rdata;
					o_wdata <= i_rdata;
					b       <= (n < MEMORY[21:0]) ? B_PUT : B_GET;
					if (n >= MEMORY[21:0]) n <= n + 22'd1;
				end
				B_PUT: begin
					n <= n + 22'd1;
					b <= B_GET;
				end
				default: ;
			endcase
		end else if (!o_req)
			case (b)
				B_MAGIC: read(FILE);
				B_TAPE:  read(FILE + 25'd2);
				B_SUM:   read(FILE + 25'd3);
				B_GET:
				if (n == words) begin
					b <= B_STOP;
					if (sum == want) o_done <= 1'b1;
					else o_missing <= 1'b1;
				end else read(FILE + KERNEL + {3'd0, n});
				B_PUT: begin
					o_req  <= 1'b1;
					o_we   <= 1'b1;
					o_addr <= BUFFER + {3'd0, n};
				end
				default: ;
			endcase
	end

endmodule

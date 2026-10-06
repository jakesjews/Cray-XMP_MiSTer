// Where the printer of the CRAY X-MP core prints: a text file on the SD card,
// mounted in an image slot of the MiSTer framework.
//
// The file has a fixed length and is all line feeds when it is new
// (tools/py/mkcos.py makes it).  When it is mounted the first block of 512
// bytes that is still all line feeds is looked for, by halving, and printing
// goes on from there: what earlier sessions printed stays.  Characters fill a
// block; a full block is written.  A block that has stood part full for a
// quarter of a second is written as well, filled up with blanks and a line
// feed, and written again when more has come, so the file is never far behind
// the printer.  When the file is full, or none is mounted, what is printed
// here is lost.
//
// The framework's side is that of hps_io: a request for one block is held
// until it is acknowledged, and the bytes come or go while the acknowledge
// lasts.

module print_spool #(
	parameter QUIET = 24  // a part full block waits 2**QUIET clocks for more
) (
	input wire clk,

	// what is printed: a character is taken when o_ready
	input  wire [7:0] i_char,
	input  wire       i_valid,
	output wire       o_ready,

	// the file
	input  wire        i_mounted,    // one clock: a file has been mounted
	input  wire [31:0] i_blocks,     // its length in blocks, with i_mounted
	output reg  [31:0] o_lba,
	output reg         o_rd,
	output reg         o_wr,
	input  wire        i_ack,
	input  wire [ 8:0] i_buff_addr,
	input  wire [ 7:0] i_buff_dout,
	output reg  [ 7:0] o_buff_din,
	input  wire        i_buff_wr
);

	localparam S_IDLE = 3'd0, S_SEEK = 3'd1, S_ASK = 3'd2, S_LOOK = 3'd3, S_PAD = 3'd4, S_WRITE = 3'd5, S_SENT = 3'd6;

	reg [ 2:0] s = S_IDLE;
	reg [31:0] blocks = 32'd0;  // of the file; none if there is no file
	reg [31:0] low, high;  // the first unused block is from low to high
	reg             used;  // the block looked at holds something
	reg [     31:0] at = 32'd0;  // the block being filled
	reg [      9:0] fill = 10'd0;  // characters in it
	reg             late = 1'b0;  // characters in it that the file does not have
	reg             whole;  // the block is written because it is full
	reg [QUIET-1:0] quiet;

	// the block: written here, read by the framework.  Never both at once.
	(* ramstyle = "no_rw_check" *)reg [7:0] block  [0:511];
	reg [8:0] b_addr;
	reg [7:0] b_data;
	reg       b_we;
	always @(posedge clk) begin
		if (b_we) block[b_addr] <= b_data;
		o_buff_din <= block[i_buff_addr];
`ifdef RW_POISON
		if (b_we && (b_addr == i_buff_addr)) o_buff_din <= ~b_data;
`endif
	end

	wire lost = (at >= blocks);  // nowhere to print to
	assign o_ready = (s == S_IDLE) && !i_mounted && (lost || !fill[9]);
	wire take = i_valid && o_ready && !lost && (i_char != 8'd0);

	wire [31:0] middle = low + ((high - low) >> 1);

	always @(*) begin
		b_we   = 1'b0;
		b_addr = fill[8:0];
		b_data = i_char;
		if (take) b_we = 1'b1;
		if (s == S_PAD) begin
			b_we   = 1'b1;
			b_data = (fill == 10'd511) ? 8'h0A : 8'h20;
		end
	end

	always @(posedge clk) begin
		quiet <= quiet + 1'd1;
		case (s)
			S_IDLE:
			if (take) begin
				fill  <= fill + 10'd1;
				late  <= 1'b1;
				quiet <= 0;
			end else if (!lost && fill[9]) begin
				whole <= 1'b1;
				o_lba <= at;
				o_wr  <= 1'b1;
				s     <= S_WRITE;
			end else if (!lost && late && (&quiet)) begin
				whole <= 1'b0;
				high  <= {22'd0, fill};  // where the characters end
				s     <= S_PAD;
			end

			// the first unused block: all before `low` are used, all from `high` are not
			S_SEEK:
			if (low == high) begin
				at <= low;
				s  <= S_IDLE;
			end else begin
				o_lba <= middle;
				o_rd  <= 1'b1;
				used  <= 1'b0;
				s     <= S_ASK;
			end
			S_ASK:
			if (i_ack) begin
				o_rd <= 1'b0;
				s    <= S_LOOK;
			end
			S_LOOK: begin
				if (i_buff_wr && (i_buff_dout != 8'h0A)) used <= 1'b1;
				if (!i_ack) begin
					if (used) low <= o_lba + 32'd1;
					else high <= o_lba;
					s <= S_SEEK;
				end
			end

			// blanks and a line feed behind the characters; the count of them is kept
			S_PAD: begin
				fill <= fill + 10'd1;
				if (fill == 10'd511) begin
					fill  <= high[9:0];
					o_lba <= at;
					o_wr  <= 1'b1;
					s     <= S_WRITE;
				end
			end

			S_WRITE:
			if (i_ack) begin
				o_wr <= 1'b0;
				s    <= S_SENT;
			end
			S_SENT:
			if (!i_ack) begin
				late <= 1'b0;
				if (whole) begin
					at   <= at + 32'd1;
					fill <= 10'd0;
				end
				s <= S_IDLE;
			end

			default: s <= S_IDLE;
		endcase

		// a file, or another one: find where to go on
		if (i_mounted) begin
			blocks <= i_blocks;
			low    <= 32'd0;
			high   <= i_blocks;
			fill   <= 10'd0;
			late   <= 1'b0;
			o_rd   <= 1'b0;
			o_wr   <= 1'b0;
			s      <= S_SEEK;
		end
	end

	initial begin
		o_rd = 1'b0;
		o_wr = 1'b0;
	end

endmodule

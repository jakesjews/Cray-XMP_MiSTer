// The reel on the tape drive of the Peripheral Expander.
//
// It is the boot tape, a .tap file in memory that has no write ring, until a
// tape file is mounted: then that file is the tape, read and written a block
// of 512 bytes at a time through signals like those of the MiSTer framework's
// hps_io.  The file's whole blocks are the tape, the first 16 Mbytes of them
// at most.  A reset of the whole machine puts the boot tape back on, as an
// operator does for a dead start; a tape file has to be mounted again then.
//
// The drive reads a word of eight bytes at a time, the first byte in bits 63
// to 56, and writes a byte at a time.  One block of the file is kept here.  A
// block that was written to goes back to the file when another is wanted and
// when the drive asks for it (i_sync), which it does at the end of every
// write.

module ios_reel (
	input wire clk,
	input wire rst,  // the machine is switched on or reset as a whole

	// the drive
	input  wire        i_req,     // read the word i_addr; held until o_ack
	input  wire [20:0] i_addr,
	output wire        o_ack,
	output wire [63:0] o_data,
	input  wire        i_put,     // write the byte i_wdata at i_at; held until o_wack
	input  wire        i_sync,    // what was written has to be in the file; held until o_wack
	input  wire [23:0] i_at,
	input  wire [ 7:0] i_wdata,
	output reg         o_wack,
	output wire [23:0] o_bytes,   // the length of the tape
	output wire        o_locked,  // it has no write ring
	output reg         o_change,  // one clock: another tape is on the drive

	// the boot tape
	output wire        o_boot_req,
	output wire [20:0] o_boot_addr,
	input  wire        i_boot_ack,
	input  wire [63:0] i_boot_data,
	input  wire [23:0] i_boot_bytes,

	// the tape file
	input  wire        i_mounted,       // one clock: a file of i_blocks blocks was chosen; none if that is zero
	input  wire [54:0] i_blocks,
	input  wire        i_readonly,
	output reg  [31:0] o_sd_lba,
	output reg         o_sd_rd,
	output reg         o_sd_wr,
	input  wire        i_sd_ack,
	input  wire [ 8:0] i_sd_buff_addr,
	input  wire [ 7:0] i_sd_buff_dout,
	output reg  [ 7:0] o_sd_buff_din,
	input  wire        i_sd_buff_wr
);

	reg        file;  // a tape file is on the drive
	reg [14:0] blocks;  // its length
	reg        readonly;
	reg        ack;
	reg [63:0] word;

	assign o_boot_req  = i_req && !file;
	assign o_boot_addr = i_addr;
	assign o_ack       = file ? ack : i_boot_ack;
	assign o_data      = file ? word : i_boot_data;
	assign o_bytes     = file ? {blocks, 9'b0} : i_boot_bytes;
	assign o_locked    = !file || readonly;

	// ---- the block at hand: one side is the framework's, one ours.  Neither
	// reads a byte in the clock it is written in.
	(* ramstyle = "no_rw_check" *)reg  [7:0] block                            [0:511];
	reg  [8:0] m_addr;
	reg        m_we;
	reg  [7:0] m_q;
	wire       sd_we = i_sd_buff_wr && i_sd_ack;
	always @(posedge clk) begin
		if (sd_we) block[i_sd_buff_addr] <= i_sd_buff_dout;
		o_sd_buff_din <= block[i_sd_buff_addr];
`ifdef RW_POISON
		if (sd_we) o_sd_buff_din <= ~i_sd_buff_dout;
		if (m_we && m_addr == i_sd_buff_addr) o_sd_buff_din <= ~i_wdata;
`endif
	end
	always @(posedge clk) begin
		if (m_we) block[m_addr] <= i_wdata;
		m_q <= block[m_addr];
`ifdef RW_POISON
		if (m_we) m_q <= ~i_wdata;
		if (sd_we && i_sd_buff_addr == m_addr) m_q <= ~i_sd_buff_dout;
`endif
	end

	localparam S_IDLE = 3'd0, S_FLUSH = 3'd1, S_FLUSHED = 3'd2, S_LOAD = 3'd3, S_LOADED = 3'd4, S_WORD = 3'd5, S_BYTE = 3'd6;
	reg [2:0] s;
	reg valid, dirty;  // a block is at hand; it was written to
	reg [14:0] at;  // which
	reg [14:0] goal;  // the block that is wanted
	reg [ 3:0] n;  // byte of the word

	wire [14:0] read_block = i_addr[20:6];
	wire [14:0] put_block = i_at[23:9];

	always @(*) begin
		m_addr = (s == S_BYTE) ? i_at[8:0] : {i_addr[5:0], n[2:0]};
		m_we   = (s == S_BYTE);
	end

	always @(posedge clk) begin
		ack      <= 1'b0;
		o_wack   <= 1'b0;
		o_change <= 1'b0;
		if (rst || i_mounted) begin
			// the boot tape again, or the file that was chosen
			file     <= !rst && (i_blocks != 55'd0);
			blocks   <= (i_blocks[54:15] != 40'd0) ? 15'h7FFF : i_blocks[14:0];
			readonly <= i_readonly;
			valid    <= 1'b0;
			dirty    <= 1'b0;
			o_sd_rd  <= 1'b0;
			o_sd_wr  <= 1'b0;
			o_change <= !rst;
			s        <= S_IDLE;
		end else
			case (s)
				S_IDLE:
				if (!file) begin
					// the boot tape takes no writing
					if ((i_put || i_sync) && !o_wack) o_wack <= 1'b1;
				end else if (i_req && !ack) begin
					n    <= 4'd0;
					goal <= read_block;
					if (valid && (at == read_block)) s <= S_WORD;
					else s <= (valid && dirty) ? S_FLUSH : S_LOAD;
				end else if (i_put && !o_wack) begin
					goal <= put_block;
					if (valid && (at == put_block)) s <= S_BYTE;
					else s <= (valid && dirty) ? S_FLUSH : S_LOAD;
				end else if (i_sync && !o_wack) begin
					if (valid && dirty) s <= S_FLUSH;
					else o_wack <= 1'b1;
				end

				// the block at hand goes back to the file
				S_FLUSH:
				if (i_sd_ack) begin
					o_sd_wr <= 1'b0;
					s       <= S_FLUSHED;
				end else begin
					o_sd_lba <= {17'd0, at};
					o_sd_wr  <= 1'b1;
				end
				S_FLUSHED:
				if (!i_sd_ack) begin
					dirty <= 1'b0;
					s     <= S_IDLE;
				end

				// the block that is wanted comes from the file
				S_LOAD:
				if (i_sd_ack) begin
					o_sd_rd <= 1'b0;
					s       <= S_LOADED;
				end else begin
					o_sd_lba <= {17'd0, goal};
					o_sd_rd  <= 1'b1;
				end
				S_LOADED:
				if (!i_sd_ack) begin
					valid <= 1'b1;
					dirty <= 1'b0;
					at    <= goal;
					s     <= S_IDLE;
				end

				// eight bytes of the block are a word: the byte asked for in
				// one clock is there in the next
				S_WORD: begin
					n <= n + 4'd1;
					if (n != 4'd0) word <= {word[55:0], m_q};
					if (n == 4'd8) begin
						ack <= 1'b1;
						s   <= S_IDLE;
					end
				end

				S_BYTE: begin
					dirty  <= 1'b1;
					o_wack <= 1'b1;
					s      <= S_IDLE;
				end

				default: s <= S_IDLE;
			endcase
	end

endmodule

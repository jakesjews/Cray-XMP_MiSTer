//////////////////////////////////////////////////////////////////
//        Vector Register                                       //
//////////////////////////////////////////////////////////////////
//
// One 64-element, 64-bit V register: a block RAM, one read address and the
// bookkeeping for the two ways an instruction can hold the register.
//
// As an operand (i_rd_start) it sends elements 0, 1, 2 ... to a functional unit,
// one per clock, starting the clock after the instruction issues.
//
// As a result (i_wr_start) it is reserved until i_len elements have been
// written.  The write port itself is driven from outside, by whichever unit,
// memory transfer or 077 instruction has data for this register.
//
// When one instruction uses the register as both operand and result the
// manual's recursive behaviour applies (pages 3-14 to 3-16): the register's
// element counter stays at zero until the first result arrives, functional
// unit time plus two clocks after issue, and only then starts to count.  So the
// operation for element n reads element 0 while n is less than that delay and
// element n minus the delay afterwards, by which time that element already
// holds its new value.
//
// Outside those uses the read address follows i_elem_idx, so a 076 (element to
// S register) finds its element two clocks after it issues, and a 177 (vector
// store) reads the element the memory unit asks for.

module v_regfile (
	input wire clk,
	input wire rst,

	// operand stream
	input wire       i_rd_start,
	input wire [6:0] i_len,        // number of elements, 1 to 64
	input wire       i_recursive,  // this register is the result of the same instruction
	input wire [4:0] i_rec_delay,  // functional unit time + 2

	// single element read (076) and vector store (177)
	input wire [5:0] i_elem_idx,
	input wire       i_mem_rd,    // a 177 is storing this register
	input wire [5:0] i_mem_idx,

	output reg [63:0] o_rd_data,  // element addressed in the previous clock

	// result reservation and write port
	input wire        i_wr_start,
	input wire        i_wr_en,
	input wire [ 5:0] i_wr_idx,
	input wire [63:0] i_wr_data,

	output wire o_busy
);

	reg [63:0] data[0:63];

	reg       rd_active;
	reg [6:0] rd_n;  // operation whose address is being presented
	reg [6:0] rd_len;
	reg       recursive;
	reg [4:0] rec_delay;
	reg [5:0] raddr;

	reg       res_busy;
	reg [6:0] wr_left;

	assign o_busy = rd_active | res_busy | i_mem_rd;

	// element an operation reads
	function [5:0] map;
		input [6:0] n;
		input rec;
		input [4:0] delay;
		begin
			if (!rec) map = n[5:0];
			else if (n < {2'b00, delay}) map = 6'd0;
			else map = n[5:0] - {1'b0, delay};
		end
	endfunction

	wire [6:0] rd_next = rd_n + 7'd1;

	always @(posedge clk) begin
		if (rst) begin
			rd_active <= 1'b0;
			res_busy  <= 1'b0;
		end else begin
			// read side
			if (i_rd_start) begin
				rd_active <= 1'b1;
				rd_n      <= 7'd0;
				rd_len    <= i_len;
				recursive <= i_recursive;
				rec_delay <= i_rec_delay;
				raddr     <= 6'd0;
			end else if (rd_active) begin
				rd_n  <= rd_next;
				raddr <= map(rd_next, recursive, rec_delay);
				if (rd_next == rd_len) rd_active <= 1'b0;
			end else if (i_mem_rd) raddr <= i_mem_idx;
			else raddr <= i_elem_idx;

			// result reservation
			if (i_wr_start) begin
				res_busy <= 1'b1;
				wr_left  <= i_len;
			end else if (res_busy && i_wr_en) begin
				wr_left <= wr_left - 7'd1;
				if (wr_left == 7'd1) res_busy <= 1'b0;
			end
		end
	end

	always @(posedge clk) begin
		if (i_wr_en) data[i_wr_idx] <= i_wr_data;
		o_rd_data <= data[raddr];
	end

endmodule

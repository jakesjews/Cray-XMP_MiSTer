//////////////////////////////////////////////////////////////////
//        Vector Register                                       //
//////////////////////////////////////////////////////////////////
//
// One 64-element, 64-bit V register: a block RAM, one read address and the
// bookkeeping for the two ways an instruction can hold the register.
//
// As an operand (i_rd_start) it sends elements 0, 1, 2 ... to a functional unit.
// Element 0 is addressed from the clock after the instruction issues; the
// operation's tracker says when the next one is wanted (i_rd_step), at the
// earliest two clocks later and then one a clock, so the register is busy for
// (VL) + 3 clocks from issue: the X-MP's "Vj or Vk ready" (CSM-0111000 section 5).
//
// As a result (i_wr_start) it is reserved until whatever delivers the result
// says it has delivered the last of it (i_wr_last).  The write port itself is
// driven from outside, by whichever unit, memory transfer or 077 instruction
// has data for this register.
//
// A register that is a result may be an operand of a later instruction at the
// same time (chaining, CSM-0111000 page 4-12): that instruction takes each
// element when it is there.  o_filling and o_filled say how far the result has
// come, two clocks late: going by those an element is addressed three clocks
// after it arrived at the register at the earliest and is at a unit four
// clocks after, which is what an instruction that issues in the clock the
// element arrives finds as well.  So an instruction that issues by the time
// element 0 arrives loses nothing against the result stream, as the manual
// says of the X-MP.  o_coming and o_come are the same a clock earlier, for the
// one unit that looks an element ahead.
//
// One instruction may use the register as both operand and result
// (CSM-0111000 page 3-33): every element is read before its result arrives,
// so the operation sees what the register held before the instruction.
//
// Outside those uses the read address follows i_elem_idx, so a 076 (element to
// S register) finds its element two clocks after it issues, and a 177 (vector
// store) reads the element the memory unit asks for.

module v_regfile (
	input wire clk,
	input wire rst,

	// operand stream
	input wire       i_rd_start,
	input wire       i_rd_step,   // the element addressed now is taken: address the next
	input wire [6:0] i_len,       // number of elements, 1 to 64

	// single element read (076) and vector store (177)
	input wire [5:0] i_elem_idx,
	input wire       i_mem_rd,    // a 177 is storing this register
	input wire [5:0] i_mem_idx,

	output reg [63:0] o_rd_data,  // element addressed in the previous clock

	// result reservation and write port
	input wire        i_wr_start,
	input wire        i_wr_last,   // the result is complete with this clock
	input wire        i_wr_en,
	input wire [ 5:0] i_wr_idx,
	input wire [63:0] i_wr_data,

	output wire       o_busy,
	output wire       o_reading,  // an operation or a vector store is reading the register
	output wire       o_result,   // reserved as a result
	output reg        o_coming,   // it was a clock ago,
	output reg  [6:0] o_come,     // with so many elements of the result in
	output reg        o_filling,  // and the same two clocks ago
	output reg  [6:0] o_filled
);

	reg [63:0] data[0:63];

	reg       rd_active;
	reg [6:0] rd_n;  // operation whose address is being presented
	reg [6:0] rd_len;
	reg [5:0] raddr;

	reg       res_busy;
	reg [6:0] wr_done;

	assign o_busy    = rd_active | res_busy | i_mem_rd;
	assign o_reading = rd_active | i_mem_rd;
	assign o_result  = res_busy;

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
				raddr     <= 6'd0;
			end else if (rd_active) begin
				if (i_rd_step) begin
					rd_n  <= rd_next;
					raddr <= rd_next[5:0];
					if (rd_next == rd_len) rd_active <= 1'b0;
				end
			end else if (i_mem_rd) raddr <= i_mem_idx;
			else raddr <= i_elem_idx;

			// result reservation
			if (i_wr_start) begin
				res_busy <= 1'b1;
				wr_done  <= 7'd0;
			end else if (res_busy) begin
				if (i_wr_en) wr_done <= wr_done + 7'd1;
				if (i_wr_last) res_busy <= 1'b0;
			end
		end
		o_coming  <= !rst && res_busy;
		o_come    <= wr_done;
		o_filling <= !rst && o_coming;
		o_filled  <= o_come;
	end

	always @(posedge clk) begin
		if (i_wr_en) data[i_wr_idx] <= i_wr_data;
		o_rd_data <= data[raddr];
	end

endmodule

//////////////////////////////////////////////////////////////////
//        Vector Register                                       //
//////////////////////////////////////////////////////////////////
//
// One 64-element, 64-bit V register: a block RAM, one read address and the
// bookkeeping for the two ways an instruction can hold the register.
//
// As an operand (i_rd_start) it sends elements 0, 1, 2 ... to a functional unit,
// one per clock.  Element 0 is sent from the clock after the instruction issues
// and the next ones follow three clocks after that, so the register is busy for
// (VL) + 3 clocks from issue: the X-MP's "Vj or Vk ready" (CSM-0111000 section 5).
//
// As a result (i_wr_start) it is reserved until i_len elements have been
// written.  The write port itself is driven from outside, by whichever unit,
// memory transfer or 077 instruction has data for this register.
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
	input wire [6:0] i_len,       // number of elements, 1 to 64

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

	output wire o_busy,
	output wire o_reading  // an operation or a vector store is reading the register
);

	reg [63:0] data[0:63];

	reg       rd_active;
	reg [6:0] rd_n;  // operation whose address is being presented
	reg [6:0] rd_len;
	reg [1:0] rd_lead;  // clocks element 0 is still held
	reg [5:0] raddr;

	reg       res_busy;
	reg [6:0] wr_left;

	assign o_busy    = rd_active | res_busy | i_mem_rd;
	assign o_reading = rd_active | i_mem_rd;

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
				rd_lead   <= 2'd2;
				raddr     <= 6'd0;
			end else if (rd_active) begin
				if (rd_lead != 2'd0) rd_lead <= rd_lead - 2'd1;
				else begin
					rd_n  <= rd_next;
					raddr <= rd_next[5:0];
					if (rd_next == rd_len) rd_active <= 1'b0;
				end
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

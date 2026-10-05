// Exchange sequencer.
//
// An exchange swaps the 16-word exchange package at address XA*16 with the
// machine's registers (manual 2240004 pages 3-35 to 3-42).  The sequence runs at
// dead start (XA = 0), on an exit instruction and on an interrupt.
//
// This block only sequences.  The registers stay where they are in func_top,
// which composes each outgoing word from the word counter and loads the
// registers a word carries when o_load pulses:
//
//   WAIT  issue is blocked and everything issued earlier runs to completion
//   RD    read package word n into a temporary
//   WR    write the outgoing word n (composed from the registers as they are)
//   LOAD  load the registers that word n carries from the temporary
//   ...   16 times, then one clock of DONE in which the instruction buffers are
//         voided, and back to RUN
//
// Reading before writing means the two packages can be the same 16 words, which
// they always are.  Nothing here depends on how long memory takes.

module exchange_ctl (
	input wire clk,
	input wire rst,

	input wire       i_request,  // start an exchange (sampled in RUN)
	input wire       i_idle,     // all earlier instructions done, memory unit and buffers idle
	input wire [7:0] i_xa,       // exchange address, sampled when the swap starts

	output wire        o_run,   // instructions may issue
	output wire        o_swap,  // the sequence owns the registers and the data memory port
	output reg  [ 3:0] o_cnt,   // package word in hand
	output wire        o_load,  // load word o_cnt's registers from o_data (one clock)
	output reg  [63:0] o_data,
	output wire        o_done,  // one clock at the end: void the instruction buffers

	output wire        o_mem_req,
	output wire        o_mem_we,
	output wire [21:0] o_mem_addr,
	input  wire        i_mem_ack,
	input  wire [63:0] i_mem_rdata
);

	localparam RUN = 3'd0, WAIT = 3'd1, RD = 3'd2, WR = 3'd3, LOAD = 3'd4, DONE = 3'd5;

	reg [2:0] state;
	reg [7:0] xa;

	assign o_run  = (state == RUN);
	assign o_swap = (state == RD) || (state == WR) || (state == LOAD) || (state == DONE);
	assign o_load = (state == LOAD);
	assign o_done = (state == DONE);

	assign o_mem_req  = (state == RD) || (state == WR);
	assign o_mem_we   = (state == WR);
	assign o_mem_addr = {10'b0, xa, o_cnt};

	always @(posedge clk) begin
		if (rst) begin
			// dead start: exchange with the package at address 0
			state <= WAIT;
			o_cnt <= 4'd0;
		end else
			case (state)
				RUN: if (i_request) state <= WAIT;

				WAIT:
				if (i_idle) begin
					xa    <= i_xa;
					o_cnt <= 4'd0;
					state <= RD;
				end

				RD:
				if (i_mem_ack) begin
					o_data <= i_mem_rdata;
					state  <= WR;
				end

				WR: if (i_mem_ack) state <= LOAD;

				LOAD: begin
					o_cnt <= o_cnt + 4'd1;
					state <= (o_cnt == 4'd15) ? DONE : RD;
				end

				DONE: state <= RUN;

				default: state <= RUN;
			endcase
	end

endmodule

// Screen handling of an Ampex Dialogue 80, the terminal the I/O Subsystem
// software drives its consoles as: 24 lines of 80 characters.
//
// Only what the software was seen to send is acted on:
//   BS, LF, CR;  FF moves the cursor one column to the right
//   ESC = row column   cursor address (each with 0x20 added)
//   ESC *              clear the screen and home the cursor
//   ESC T              erase from the cursor to the end of the line
//   ESC R              delete the cursor's line: the lines below move up
//   ESC G c            line drawing, dropped
// Every other control character and escape sequence is dropped.  A character
// written in the last column moves the cursor to the start of the next line.
//
// The screen scrolls by moving its top row, so only a deleted line makes this
// copy characters; for that the screen memory is read and written through one
// port.  tools/crates/ios/src/screen.rs is the same terminal in the model, and
// sim/tb/term_ampex_tb.cpp checks one against the other.

module term_ampex (
	input wire clk,
	input wire reset,

	input  wire [6:0] din,
	input  wire       din_valid,
	output wire       din_ready,

	// screen memory: rdata is the character at addr, one clock later
	output reg  [10:0] addr,
	output reg  [ 7:0] wdata,
	output reg         we,
	input  wire [ 7:0] rdata,

	output reg [4:0] top_row,
	output reg [6:0] cur_x,
	output reg [4:0] cur_y
);

	localparam COLS = 7'd80;
	localparam ROWS = 5'd24;

	localparam S_IDLE  = 4'd0;
	localparam S_PUT   = 4'd1;
	localparam S_CLEAR = 4'd2;
	localparam S_ESC   = 4'd3;
	localparam S_ROW   = 4'd4;
	localparam S_COL   = 4'd5;
	localparam S_SKIP  = 4'd6;
	localparam S_FROM  = 4'd7;  // a deleted line: address the character below
	localparam S_WAIT  = 4'd8;
	localparam S_TO    = 4'd9;  // and write it one line up

	reg [3:0] state;
	reg [6:0] held;
	reg [6:0] clr_x;
	reg [4:0] clr_y, clr_end_y;
	reg [6:0] cp_x;
	reg [4:0] cp_y;

	assign din_ready = (state == S_IDLE) || (state == S_ESC) || (state == S_ROW) || (state == S_COL) || (state == S_SKIP);
	wire accept = din_valid && din_ready;

	function [10:0] cell_addr;
		input [4:0] y;
		input [6:0] x;
		input [4:0] top;
		reg [5:0] sum;
		reg [4:0] phys;
		begin
			sum       = {1'b0, y} + {1'b0, top};
			phys      = (sum >= 6'd24) ? sum[4:0] - 5'd24 : sum[4:0];
			cell_addr = {phys, 6'd0} + {2'd0, phys, 4'd0} + {4'd0, x};
		end
	endfunction

	// the row and the column of a cursor address: 0x20 less, kept on the screen
	wire [6:0] row_n = held - 7'h20;
	wire [6:0] col_n = din - 7'h20;
	wire [4:0] row = (held < 7'h20) ? 5'd0 : (row_n > {2'd0, ROWS - 1'd1}) ? ROWS - 1'd1 : row_n[4:0];
	wire [6:0] col = (din < 7'h20) ? 7'd0 : (col_n > COLS - 1'd1) ? COLS - 1'd1 : col_n;

	wire [4:0] top_next = (top_row == ROWS - 1'd1) ? 5'd0 : top_row + 1'd1;

	task start_clear;
		input [4:0] y0;
		input [6:0] x0;
		input [4:0] y1;
		begin
			clr_x     <= x0;
			clr_y     <= y0;
			clr_end_y <= y1;
			state     <= S_CLEAR;
		end
	endtask

	// Move down one line; at the bottom the view scrolls and the new line is erased.
	task line_feed;
		begin
			if (cur_y == ROWS - 1'd1) begin
				top_row <= top_next;
				start_clear(ROWS - 1'd1, 7'd0, ROWS - 1'd1);
			end else begin
				cur_y <= cur_y + 1'd1;
				state <= S_IDLE;
			end
		end
	endtask

	always @(posedge clk) begin
		we <= 0;
		if (reset) begin
			top_row <= 0;
			cur_x   <= 0;
			cur_y   <= 0;
			start_clear(5'd0, 7'd0, ROWS - 1'd1);
		end else
			case (state)
				S_IDLE:
				if (accept) begin
					if (din >= 7'h20 && din != 7'h7F) begin
						held  <= din;
						state <= S_PUT;
					end else
						case (din)
							7'h08:   if (cur_x != 0) cur_x <= cur_x - 1'd1;
							7'h0A:   line_feed;
							7'h0C:   if (cur_x != COLS - 1'd1) cur_x <= cur_x + 1'd1;
							7'h0D:   cur_x <= 0;
							7'h1B:   state <= S_ESC;
							default: ;
						endcase
				end

				S_PUT: begin
					addr  <= cell_addr(cur_y, cur_x, top_row);
					wdata <= {1'b0, held};
					we    <= 1;
					if (cur_x == COLS - 1'd1) begin
						cur_x <= 0;
						line_feed;
					end else begin
						cur_x <= cur_x + 1'd1;
						state <= S_IDLE;
					end
				end

				S_CLEAR: begin
					addr  <= cell_addr(clr_y, clr_x, top_row);
					wdata <= 8'h20;
					we    <= 1;
					if (clr_x == COLS - 1'd1) begin
						clr_x <= 0;
						clr_y <= clr_y + 1'd1;
						if (clr_y == clr_end_y) state <= S_IDLE;
					end else clr_x <= clr_x + 1'd1;
				end

				S_ESC:
				if (accept) begin
					state <= S_IDLE;
					case ({
						1'b0, din
					})
						"=":     state <= S_ROW;
						"G":     state <= S_SKIP;
						"*": begin
							cur_x <= 0;
							cur_y <= 0;
							start_clear(5'd0, 7'd0, ROWS - 1'd1);
						end
						"T":     start_clear(cur_y, cur_x, cur_y);
						"R":
						if (cur_y == ROWS - 1'd1) start_clear(ROWS - 1'd1, 7'd0, ROWS - 1'd1);
						else begin
							cp_x  <= 0;
							cp_y  <= cur_y;
							state <= S_FROM;
						end
						default: ;
					endcase
				end

				S_ROW:
				if (accept) begin
					held  <= din;
					state <= S_COL;
				end

				S_COL:
				if (accept) begin
					cur_y <= row;
					cur_x <= col;
					state <= S_IDLE;
				end

				S_SKIP: if (accept) state <= S_IDLE;

				S_FROM: begin
					addr  <= cell_addr(cp_y + 1'd1, cp_x, top_row);
					state <= S_WAIT;
				end

				S_WAIT: state <= S_TO;

				S_TO: begin
					addr  <= cell_addr(cp_y, cp_x, top_row);
					wdata <= rdata;
					we    <= 1;
					state <= S_FROM;
					if (cp_x == COLS - 1'd1) begin
						cp_x <= 0;
						cp_y <= cp_y + 1'd1;
						if (cp_y == ROWS - 5'd2) start_clear(ROWS - 1'd1, 7'd0, ROWS - 1'd1);
					end else cp_x <= cp_x + 1'd1;
				end

				default: state <= S_IDLE;
			endcase
	end

endmodule

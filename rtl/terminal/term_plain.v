// The screen that shows what the printer prints: 24 lines of 80 characters
// that take characters, carriage return and line feed and nothing else
// (rtl/mister/print_screen.v sends no more).  A character in the last column
// goes on in the next line; a line feed on the last line moves the page up.
//
// It writes the screen memory of a console (xmp_terminal.v), and like a
// console it scrolls by moving its top row.  The status line, which a
// console has above its text, is left empty.

module term_plain (
	input wire clk,
	input wire reset,

	input  wire [6:0] din,
	input  wire       din_valid,
	output wire       din_ready,

	// screen memory: memory row * 80 + column; a cell is four attributes, the
	// protect bit and the character
	output reg [11:0] addr,
	output reg [11:0] wdata,
	output reg        we,

	output reg  [4:0] top_row,  // the memory row of the first line
	output reg  [6:0] cur_x,
	output wire [4:0] cur_y     // the memory row of the cursor
);

	localparam S_CLEAR = 2'd0;  // every cell, the status line's too
	localparam S_IDLE  = 2'd1;
	localparam S_LINE  = 2'd2;  // the line that has come up at the bottom

	reg [ 1:0] state;
	reg [ 4:0] line;  // the cursor's line on the screen
	reg [10:0] n;
	reg [ 6:0] left;

	wire [5:0] sum = {1'b0, line} + {1'b0, top_row};
	assign cur_y = (sum >= 6'd24) ? sum[4:0] - 5'd24 : sum[4:0];
	wire [10:0] here = {cur_y, 6'd0} + {2'd0, cur_y, 4'd0} + {4'd0, cur_x};
	wire [ 4:0] top_next = (top_row == 5'd23) ? 5'd0 : top_row + 5'd1;

	assign din_ready = (state == S_IDLE);

	// Down one line; from the last line the page moves up, and the row that
	// was its first is cleared to be its last.
	task feed;
		begin
			if (line == 5'd23) begin
				top_row <= top_next;
				n       <= {top_row, 6'd0} + {2'd0, top_row, 4'd0};
				left    <= 7'd79;
				state   <= S_LINE;
			end else line <= line + 5'd1;
		end
	endtask

	always @(posedge clk) begin
		we <= 1'b0;
		if (reset) begin
			top_row <= 5'd0;
			cur_x   <= 7'd0;
			line    <= 5'd0;
			n       <= 11'd0;
			state   <= S_CLEAR;
		end else
			case (state)
				S_CLEAR: begin
					addr  <= {1'b0, n};
					wdata <= 12'h020;
					we    <= 1'b1;
					n     <= n + 11'd1;
					if (n == 11'd1999) state <= S_IDLE;
				end

				S_IDLE:
				if (din_valid) begin
					if ((din >= 7'h20) && (din != 7'h7F)) begin
						addr  <= {1'b0, here};
						wdata <= {5'd0, din};
						we    <= 1'b1;
						if (cur_x == 7'd79) begin
							cur_x <= 7'd0;
							feed;
						end else cur_x <= cur_x + 7'd1;
					end else if (din == 7'h0D) cur_x <= 7'd0;
					else if (din == 7'h0A) feed;
				end

				S_LINE: begin
					addr  <= {1'b0, n};
					wdata <= 12'h020;
					we    <= 1'b1;
					n     <= n + 11'd1;
					left  <= left - 7'd1;
					if (left == 7'd0) state <= S_IDLE;
				end

				default: state <= S_IDLE;
			endcase
	end

endmodule

// Character handling for the Cray console screen: 80x24, scrolling by moving the
// top row instead of copying, so the screen buffer can live in block RAM.
//
// Control characters: BS, TAB, LF, FF (clear and home), CR.
// VT52 escapes:       ESC A/B/C/D (cursor), H (home), J (erase to end of screen),
//                     K (erase to end of line), Y row col (direct address), E (clear).
// A character written in column 80 leaves the cursor there; the wrap happens when
// the next printable character arrives, so 80-column lines followed by CR LF do
// not produce a blank line.

module term_ctrl (
	input wire clk,
	input wire reset,

	input  wire [7:0] din,
	input  wire       din_valid,
	output wire       din_ready,

	output reg [10:0] waddr,
	output reg [ 7:0] wdata,
	output reg        we,

	output reg [4:0] top_row,
	output reg [6:0] cur_x,
	output reg [4:0] cur_y
);

	localparam COLS = 7'd80;
	localparam ROWS = 5'd24;

	localparam S_IDLE  = 3'd0;
	localparam S_PUT   = 3'd1;
	localparam S_CLEAR = 3'd2;
	localparam S_ESC   = 3'd3;
	localparam S_ROW   = 3'd4;
	localparam S_COL   = 3'd5;

	reg [2:0] state, clr_return;
	reg [7:0] held;
	reg       wrap_pending;

	reg [6:0] clr_x;
	reg [4:0] clr_y, clr_end_y;

	assign din_ready = (state == S_IDLE) || (state == S_ESC) || (state == S_ROW) || (state == S_COL);
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

	wire [4:0] top_next = (top_row == ROWS - 1'd1) ? 5'd0 : top_row + 1'd1;

	// Move down one line; at the bottom the view scrolls and the new line is erased.
	task newline;
		input [2:0] ret;
		begin
			if (cur_y == ROWS - 1'd1) begin
				top_row    <= top_next;
				clr_x      <= 0;
				clr_y      <= ROWS - 1'd1;
				clr_end_y  <= ROWS - 1'd1;
				clr_return <= ret;
				state      <= S_CLEAR;
			end else begin
				cur_y <= cur_y + 1'd1;
				state <= ret;
			end
		end
	endtask

	task start_clear;
		input [4:0] y0;
		input [6:0] x0;
		input [4:0] y1;
		begin
			clr_x      <= x0;
			clr_y      <= y0;
			clr_end_y  <= y1;
			clr_return <= S_IDLE;
			state      <= S_CLEAR;
		end
	endtask

	always @(posedge clk) begin
		we <= 0;

		if (reset) begin
			top_row      <= 0;
			cur_x        <= 0;
			cur_y        <= 0;
			wrap_pending <= 0;
			clr_x        <= 0;
			clr_y        <= 0;
			clr_end_y    <= ROWS - 1'd1;
			clr_return   <= S_IDLE;
			state        <= S_CLEAR;
		end else
			case (state)
				S_IDLE:
				if (accept) begin
					if (din >= 8'h20 && din != 8'h7F) begin
						held <= din;
						if (wrap_pending) begin
							wrap_pending <= 0;
							cur_x        <= 0;
							newline(S_PUT);
						end else state <= S_PUT;
					end else begin
						case (din)
							8'h08: begin
								wrap_pending <= 0;
								if (cur_x != 0) cur_x <= cur_x - 1'd1;
							end
							8'h09: begin
								wrap_pending <= 0;
								cur_x        <= (cur_x[6:3] >= 4'd9) ? COLS - 1'd1 : {cur_x[6:3] + 1'd1, 3'd0};
							end
							8'h0A: begin
								wrap_pending <= 0;
								newline(S_IDLE);
							end
							8'h0C: begin
								wrap_pending <= 0;
								cur_x        <= 0;
								cur_y        <= 0;
								start_clear(5'd0, 7'd0, ROWS - 1'd1);
							end
							8'h0D: begin
								wrap_pending <= 0;
								cur_x        <= 0;
							end
							8'h1B:   state <= S_ESC;
							default: ;
						endcase
					end
				end

				S_PUT: begin
					waddr <= cell_addr(cur_y, cur_x, top_row);
					wdata <= held;
					we    <= 1;
					if (cur_x == COLS - 1'd1) wrap_pending <= 1;
					else cur_x <= cur_x + 1'd1;
					state <= S_IDLE;
				end

				S_CLEAR: begin
					waddr <= cell_addr(clr_y, clr_x, top_row);
					wdata <= 8'h20;
					we    <= 1;
					if (clr_x == COLS - 1'd1) begin
						clr_x <= 0;
						clr_y <= clr_y + 1'd1;
						if (clr_y == clr_end_y) state <= clr_return;
					end else clr_x <= clr_x + 1'd1;
				end

				S_ESC:
				if (accept) begin
					state        <= S_IDLE;
					wrap_pending <= 0;
					case (din)
						"A":     if (cur_y != 0) cur_y <= cur_y - 1'd1;
						"B":     if (cur_y != ROWS - 1'd1) cur_y <= cur_y + 1'd1;
						"C":     if (cur_x != COLS - 1'd1) cur_x <= cur_x + 1'd1;
						"D":     if (cur_x != 0) cur_x <= cur_x - 1'd1;
						"H": begin
							cur_x <= 0;
							cur_y <= 0;
						end
						"E": begin
							cur_x <= 0;
							cur_y <= 0;
							start_clear(5'd0, 7'd0, ROWS - 1'd1);
						end
						"J":     start_clear(cur_y, cur_x, ROWS - 1'd1);
						"K":     start_clear(cur_y, cur_x, cur_y);
						"Y":     state <= S_ROW;
						default: ;
					endcase
				end

				S_ROW:
				if (accept) begin
					if (din >= 8'h20) cur_y <= (din - 8'h20 >= {3'd0, ROWS}) ? ROWS - 1'd1 : din[4:0];
					state <= S_COL;
				end

				S_COL:
				if (accept) begin
					if (din >= 8'h20) cur_x <= (din - 8'h20 >= {1'd0, COLS}) ? COLS - 1'd1 : din[6:0] - 7'h20;
					state <= S_IDLE;
				end

				default: state <= S_IDLE;
			endcase
	end

endmodule

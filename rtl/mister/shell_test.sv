// Hardware self-test that stands in for the Cray CPU.
//
// It exercises everything around the CPU: the console (screen, keyboard and
// serial mirror) and the DDR3 memory port.  On reset it prints a banner, shows
// memory words 0-3 (so a loaded image can be checked), then writes, reads back
// and burst-reads 64K words, reporting how many clocks each request took, and
// finally echoes whatever is typed.  Latencies are in system clocks, in hex.

module shell_test (
	input wire clk,
	input wire reset,

	output reg         mem_req,
	output reg         mem_we,
	output reg         mem_burst,
	output reg  [21:0] mem_addr,
	output reg  [63:0] mem_wdata,
	input  wire        mem_ack,
	input  wire [63:0] mem_rdata,

	output reg  [7:0] tx_data,
	output reg        tx_valid,
	input  wire       tx_ready,

	input  wire [7:0] rx_data,
	input  wire       rx_valid,
	output reg        rx_pop,

	output reg running,  // test in progress
	output reg failed    // read-back errors seen
);

	localparam [21:0] BASE  = 22'h010000;
	localparam [16:0] WORDS = 17'h10000;

	function [63:0] pattern;
		input [21:0] a;
		pattern = {a ^ 22'h2AAAAA, 10'h3C5, ~a, 10'h0A3};
	endfunction

	typedef enum logic [3:0] {
		S_DISPATCH,
		S_DELAY,
		S_LBL,
		S_HEX,
		S_PUT,
		S_RD1,
		S_WR_REQ,
		S_WR_WAIT,
		S_RD_REQ,
		S_RD_WAIT,
		S_BU_REQ,
		S_BU_WAIT,
		S_ECHO
	} state_t;

	state_t state, put_ret;
	logic [ 6:0] step;
	logic [22:0] delay;

	logic [63:0] lbl;  // up to 8 characters, zero bytes are skipped
	logic [ 3:0] lbl_cnt;
	logic [63:0] hex_val;
	logic [ 4:0] hex_cnt;
	logic [ 7:0] put_ch;

	logic [63:0] rd_q;
	logic [16:0] idx;
	logic [ 4:0] beat;
	logic [15:0] timer;
	logic [31:0] errors;

	// statistics: [0] write, [1] read, [2] burst first word, [3] burst all 16
	logic [15:0] t_min[4];
	logic [15:0] t_max[4];
	logic [31:0] t_sum[4];

	task automatic stat(input logic [1:0] n, input logic [15:0] t);
		if (t < t_min[n]) t_min[n] <= t;
		if (t > t_max[n]) t_max[n] <= t;
		t_sum[n] <= t_sum[n] + {16'd0, t};
	endtask

	task automatic do_lbl(input logic [63:0] s);
		lbl     <= s;
		lbl_cnt <= 4'd8;
		state   <= S_LBL;
	endtask

	task automatic do_hex(input logic [63:0] v, input logic [4:0] n);
		hex_val <= v << (7'd64 - {n, 2'b00});
		hex_cnt <= n;
		state   <= S_HEX;
	endtask

	task automatic do_rd1(input logic [21:0] a);
		mem_addr  <= a;
		mem_we    <= 0;
		mem_burst <= 0;
		mem_req   <= 1;
		state     <= S_RD1;
	endtask

	function automatic [7:0] hexdigit(input logic [3:0] n);
		hexdigit = (n < 4'd10) ? (8'h30 + {4'd0, n}) : (8'h37 + {4'd0, n});
	endfunction

	wire [21:0] cur_addr = BASE + {6'd0, idx[15:0]};
	localparam [15:0] CRLF = 16'h0D0A;

	always @(posedge clk) begin
		rx_pop <= 0;
		if (tx_valid && tx_ready) tx_valid <= 0;
		timer <= timer + 1'd1;

		if (reset) begin
			state    <= S_DISPATCH;
			step     <= 0;
			mem_req  <= 0;
			tx_valid <= 0;
			running  <= 1;
			failed   <= 0;
			errors   <= 0;
			for (int i = 0; i < 4; i++) begin
				t_min[i] <= 16'hFFFF;
				t_max[i] <= 0;
				t_sum[i] <= 0;
			end
		end else
			case (state)
				S_DISPATCH: begin
					step <= step + 1'd1;
					case (step)
						0: begin
							delay <= '1;
							state <= S_DELAY;
						end
						1:  do_lbl({8'h0C, "CRAY-1 "});
						2:  do_lbl("MiSTer s");
						3:  do_lbl("hell tes");
						4:  do_lbl({"t", CRLF, 40'd0});
						5:  do_lbl("IMAGE   ");
						6:  do_rd1(22'd0);
						7:  do_hex(rd_q, 5'd16);
						8:  do_lbl({" ", 56'd0});
						9:  do_rd1(22'd1);
						10: do_hex(rd_q, 5'd16);
						11: do_lbl({" ", 56'd0});
						12: do_rd1(22'd2);
						13: do_hex(rd_q, 5'd16);
						14: do_lbl({" ", 56'd0});
						15: do_rd1(22'd3);
						16: do_hex(rd_q, 5'd16);
						17: do_lbl({CRLF, 48'd0});
						18: begin
							idx   <= 0;
							state <= S_WR_REQ;
						end
						19: begin
							idx   <= 0;
							state <= S_RD_REQ;
						end
						20: begin
							idx   <= 0;
							state <= S_BU_REQ;
						end
						21: do_lbl("        ");
						22: do_lbl("MIN  MAX");
						23: do_lbl({"  AVG", CRLF, 8'd0});
						24: do_lbl("WRITE   ");
						25: do_hex({48'd0, t_min[0]}, 5'd4);
						26: do_lbl({" ", 56'd0});
						27: do_hex({48'd0, t_max[0]}, 5'd4);
						28: do_lbl({" ", 56'd0});
						29: do_hex({48'd0, t_sum[0][31:16]}, 5'd4);
						30: do_lbl({CRLF, "READ  "});
						31: do_lbl({"  ", 48'd0});
						32: do_hex({48'd0, t_min[1]}, 5'd4);
						33: do_lbl({" ", 56'd0});
						34: do_hex({48'd0, t_max[1]}, 5'd4);
						35: do_lbl({" ", 56'd0});
						36: do_hex({48'd0, t_sum[1][31:16]}, 5'd4);
						37: do_lbl({CRLF, "BURST1"});
						38: do_lbl({"  ", 48'd0});
						39: do_hex({48'd0, t_min[2]}, 5'd4);
						40: do_lbl({" ", 56'd0});
						41: do_hex({48'd0, t_max[2]}, 5'd4);
						42: do_lbl({" ", 56'd0});
						43: do_hex({48'd0, t_sum[2][27:12]}, 5'd4);
						44: do_lbl({CRLF, "BURST1"});
						45: do_lbl({"6 ", 48'd0});
						46: do_hex({48'd0, t_min[3]}, 5'd4);
						47: do_lbl({" ", 56'd0});
						48: do_hex({48'd0, t_max[3]}, 5'd4);
						49: do_lbl({" ", 56'd0});
						50: do_hex({48'd0, t_sum[3][27:12]}, 5'd4);
						51: do_lbl({CRLF, "ERRORS"});
						52: do_lbl({"  ", 48'd0});
						53: do_hex({32'd0, errors}, 5'd8);
						54: do_lbl({CRLF, "ECHO", CRLF});
						default: begin
							running <= 0;
							failed  <= (errors != 0);
							state   <= S_ECHO;
						end
					endcase
				end

				S_DELAY: begin
					delay <= delay - 1'd1;
					if (delay == 0) state <= S_DISPATCH;
				end

				// print the non-zero bytes of lbl, most significant first
				S_LBL: begin
					if (lbl_cnt == 0) state <= S_DISPATCH;
					else begin
						lbl     <= {lbl[55:0], 8'd0};
						lbl_cnt <= lbl_cnt - 1'd1;
						if (lbl[63:56] != 0) begin
							put_ch  <= lbl[63:56];
							put_ret <= S_LBL;
							state   <= S_PUT;
						end
					end
				end

				S_HEX: begin
					if (hex_cnt == 0) state <= S_DISPATCH;
					else begin
						hex_val <= {hex_val[59:0], 4'd0};
						hex_cnt <= hex_cnt - 1'd1;
						put_ch  <= hexdigit(hex_val[63:60]);
						put_ret <= S_HEX;
						state   <= S_PUT;
					end
				end

				S_PUT:
				if (!tx_valid) begin
					tx_data  <= put_ch;
					tx_valid <= 1;
					state    <= put_ret;
				end

				S_RD1:
				if (mem_ack) begin
					rd_q    <= mem_rdata;
					mem_req <= 0;
					state   <= S_DISPATCH;
				end

				// ---- single word writes ----
				S_WR_REQ: begin
					if (idx == WORDS) state <= S_DISPATCH;
					else begin
						mem_addr  <= cur_addr;
						mem_wdata <= pattern(cur_addr);
						mem_we    <= 1;
						mem_burst <= 0;
						mem_req   <= 1;
						timer     <= 0;
						state     <= S_WR_WAIT;
					end
				end
				S_WR_WAIT:
				if (mem_ack) begin
					mem_req <= 0;
					stat(2'd0, timer);
					idx   <= idx + 1'd1;
					state <= S_WR_REQ;
				end

				// ---- single word reads ----
				S_RD_REQ: begin
					if (idx == WORDS) state <= S_DISPATCH;
					else begin
						mem_addr  <= cur_addr;
						mem_we    <= 0;
						mem_burst <= 0;
						mem_req   <= 1;
						timer     <= 0;
						state     <= S_RD_WAIT;
					end
				end
				S_RD_WAIT:
				if (mem_ack) begin
					mem_req <= 0;
					stat(2'd1, timer);
					if (mem_rdata != pattern(cur_addr)) errors <= errors + 1'd1;
					idx   <= idx + 1'd1;
					state <= S_RD_REQ;
				end

				// ---- 16-word burst reads ----
				S_BU_REQ: begin
					if (idx == WORDS) state <= S_DISPATCH;
					else begin
						mem_addr  <= cur_addr;
						mem_we    <= 0;
						mem_burst <= 1;
						mem_req   <= 1;
						timer     <= 0;
						beat      <= 0;
						state     <= S_BU_WAIT;
					end
				end
				S_BU_WAIT:
				if (mem_ack) begin
					if (mem_rdata != pattern(cur_addr + {18'd0, beat[3:0]})) errors <= errors + 1'd1;
					if (beat == 0) stat(2'd2, timer);
					beat <= beat + 1'd1;
					if (beat == 5'd15) begin
						mem_req <= 0;
						stat(2'd3, timer);
						idx   <= idx + 17'd16;
						state <= S_BU_REQ;
					end
				end

				S_ECHO:
				if (rx_valid && !rx_pop && !tx_valid) begin
					rx_pop <= 1;
					if (rx_data == 8'h0D) begin
						lbl     <= {CRLF, 48'd0};
						lbl_cnt <= 4'd8;
						step    <= 7'd127;  // dispatch falls through to echo again
						state   <= S_LBL;
					end else begin
						tx_data  <= rx_data;
						tx_valid <= 1;
					end
				end

				default: state <= S_DISPATCH;
			endcase
	end

endmodule

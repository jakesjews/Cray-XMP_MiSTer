// Minimal 8N1 UART for the console mirror on the HPS serial port.

module uart_tx #(
	parameter CLK_HZ = 29400000,
	parameter BAUD   = 115200
) (
	input  wire       clk,
	input  wire       reset,
	input  wire [7:0] din,
	input  wire       din_valid,
	output wire       din_ready,
	output reg        txd
);

	localparam integer DIV = (CLK_HZ + BAUD / 2) / BAUD;

	reg [15:0] cnt;
	reg [ 3:0] bits;  // bits left to send, including start and stop
	reg [ 8:0] sh;

	assign din_ready = (bits == 0);

	always @(posedge clk) begin
		if (reset) begin
			bits <= 0;
			txd  <= 1;
		end else if (bits == 0) begin
			txd <= 1;
			if (din_valid) begin
				sh   <= {1'b1, din};
				txd  <= 0;  // start bit
				bits <= 4'd10;
				cnt  <= DIV[15:0] - 1'd1;
			end
		end else if (cnt == 0) begin
			cnt  <= DIV[15:0] - 1'd1;
			bits <= bits - 1'd1;
			if (bits != 4'd1) begin
				txd <= sh[0];
				sh  <= {1'b1, sh[8:1]};
			end
		end else cnt <= cnt - 1'd1;
	end

endmodule


module uart_rx #(
	parameter CLK_HZ = 29400000,
	parameter BAUD   = 115200
) (
	input  wire       clk,
	input  wire       reset,
	input  wire       rxd,
	output reg  [7:0] dout,
	output reg        dout_valid,  // one clock pulse
	output wire       brk          // high for as long as a BREAK lasts
);

	localparam integer DIV = (CLK_HZ + BAUD / 2) / BAUD;

	reg  [2:0] sync;
	wire       rx = sync[2];
	always @(posedge clk) sync <= {sync[1:0], rxd};

	reg [15:0] cnt;
	reg [ 3:0] bits;  // 0 idle, 1 start, 2..9 data, 10 stop
	reg [ 7:0] sh;

	// BREAK: line low for more than two character times.  It is reported for as
	// long as the line stays low, and only after the line has been seen idle once,
	// so a line that is low at power-up is not taken as a request.
	localparam integer BRK_LEN = DIV * 24;
	reg [19:0] low_cnt;
	reg        seen_idle;
	reg        in_break;

	assign brk = in_break;

	always @(posedge clk) begin
		dout_valid <= 0;

		if (reset) begin
			bits      <= 0;
			low_cnt   <= 0;
			seen_idle <= 0;
			in_break  <= 0;
		end else begin
			// break detector
			if (rx) begin
				seen_idle <= 1;
				low_cnt   <= 0;
				in_break  <= 0;
			end else if (low_cnt < BRK_LEN[19:0]) low_cnt <= low_cnt + 1'd1;
			else if (seen_idle) in_break <= 1;

			// receiver
			if (bits == 0) begin
				if (!rx) begin
					bits <= 4'd1;
					cnt  <= DIV[15:0] / 2;
				end
			end else if (cnt == 0) begin
				cnt <= DIV[15:0] - 1'd1;
				if (bits == 4'd1) begin
					bits <= rx ? 4'd0 : 4'd2;  // false start
				end else if (bits == 4'd10) begin
					bits <= 0;
					if (rx) begin  // valid stop bit
						dout       <= sh;
						dout_valid <= 1;
					end
				end else begin
					sh   <= {rx, sh[7:1]};
					bits <= bits + 1'd1;
				end
			end else cnt <= cnt - 1'd1;
		end
	end

endmodule

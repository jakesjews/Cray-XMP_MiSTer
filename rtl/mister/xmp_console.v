// Console plumbing of the CRAY X-MP core, in the clock domain of the video.
//
// There are two consoles: 0 is the operator's console of the I/O Subsystem,
// 1 the station.  What they print goes to their screens and, both together, to
// the HPS serial port, which sets the pace (115200 baud); there bit 7 of a
// byte says which console it is from.  Keys typed on the keyboard go to the
// console whose screen is shown; one that is not a character (bit 7) is
// dropped.  A byte received on the serial port goes to the console its bit 7
// names, so a test script can work both at once.
//
// The core's own two messages are put on the operator's console the same way
// the machine's characters are: `no_file` when there is no boot file to start
// from, `started` when the I/O Subsystem has been dead started.  Each is sent
// once, when its signal rises; the machine is in reset or has not printed
// anything yet at those times.

module xmp_console #(
	parameter CLK_HZ = 29400000,
	parameter BAUD   = 115200
) (
	input wire clk,
	input wire reset,
	input wire uart_reset, // reset for the serial receiver only

	input wire visible,  // the console whose screen is shown
	input wire no_file,
	input wire started,

	// what the machine prints
	input  wire [13:0] tx_data,
	input  wire [ 1:0] tx_valid,
	output wire [ 1:0] tx_ready,

	// keys for the machine
	output wire [13:0] rx_data,
	output wire [ 1:0] rx_valid,
	input  wire [ 1:0] rx_ready,

	// the screens
	output wire [13:0] term_data,
	output wire [ 1:0] term_valid,
	input  wire [ 1:0] term_ready,

	// the keyboard
	input  wire [7:0] kbd_data,
	input  wire       kbd_valid,
	output wire       kbd_ready,

	// HPS serial port
	input  wire uart_rxd,
	output wire uart_txd,
	output wire uart_break
);

	// ---- the core's messages ----
	// 64 characters each, the text first and zeros after it
	localparam [511:0] NO_FILE_TEXT = {
		8'h1B, "*CRAY X-MP", 8'h0D, 8'h0A, 8'h0A, "Load a boot file from the menu to start it.", 8'h0D, 8'h0A, 40'd0
	};
	localparam [511:0] STARTED_TEXT = {"I/O SUBSYSTEM DEAD START", 8'h0D, 8'h0A, 304'd0};

	reg no_file_d, started_d;
	reg msg_on, msg_which;
	reg  [5:0] msg_n;  // the character to send
	wire [8:0] msg_bit = {~msg_n, 3'd0};
	wire [6:0] msg_char = msg_which ? STARTED_TEXT[msg_bit+:7] : NO_FILE_TEXT[msg_bit+:7];
	wire       msg_taken;

	always @(posedge clk) begin
		no_file_d <= no_file;
		started_d <= started;
		if (reset) begin
			msg_on    <= 1'b0;
			no_file_d <= 1'b0;
			started_d <= 1'b0;
		end else if (msg_on) begin
			if (msg_char == 7'd0) msg_on <= 1'b0;
			else if (msg_taken) msg_n <= msg_n + 6'd1;
		end else if (no_file && !no_file_d) begin
			msg_on    <= 1'b1;
			msg_which <= 1'b0;
			msg_n     <= 6'd0;
		end else if (started && !started_d) begin
			msg_on    <= 1'b1;
			msg_which <= 1'b1;
			msg_n     <= 6'd0;
		end
	end

	// ---- output: the two consoles take turns at the serial port ----
	wire [6:0] out_data[0:1];
	assign out_data[0] = msg_on ? msg_char : tx_data[6:0];
	assign out_data[1] = tx_data[13:7];
	wire [1:0] out_valid = {tx_valid[1], msg_on ? (msg_char != 7'd0) : tx_valid[0]};

	// a console can send when its screen can take the character
	wire [1:0] can = out_valid & term_ready;
	reg        last;  // the console served last waits for the other
	wire       sel = can[~last] ? ~last : last;
	wire       uart_ready;
	wire       go = can[sel] && uart_ready;
	wire [1:0] took = go ? (sel ? 2'b10 : 2'b01) : 2'b00;

	always @(posedge clk) if (go) last <= sel;

	assign term_data  = {out_data[1], out_data[0]};
	assign term_valid = took;
	assign tx_ready   = {took[1], took[0] & ~msg_on};
	assign msg_taken  = took[0] & msg_on;

	uart_tx #(
		.CLK_HZ(CLK_HZ),
		.BAUD  (BAUD)
	) utx (
		.clk      (clk),
		.reset    (reset),
		.din      ({sel, out_data[sel]}),
		.din_valid(can[sel]),
		.din_ready(uart_ready),
		.txd      (uart_txd)
	);

	// ---- input: serial bytes cannot wait, so they go first ----
	wire [7:0] ser_data;
	wire       ser_valid;

	uart_rx #(
		.CLK_HZ(CLK_HZ),
		.BAUD  (BAUD)
	) urx (
		.clk       (clk),
		.reset     (uart_reset),
		.rxd       (uart_rxd),
		.dout      (ser_data),
		.dout_valid(ser_valid),
		.brk       (uart_break)
	);

	wire [1:0] full;
	wire [1:0] ser_wr = ser_valid ? (ser_data[7] ? 2'b10 : 2'b01) : 2'b00;
	wire [1:0] kbd_wr = (kbd_valid && kbd_ready && !kbd_data[7]) ? (visible ? 2'b10 : 2'b01) : 2'b00;
	assign kbd_ready = ~full[visible] && ~ser_wr[visible];

	genvar g;
	generate
		for (g = 0; g < 2; g = g + 1) begin : g_keys
			con_fifo #(
				.AW(6),
				.DW(7)
			) fifo (
				.clk  (clk),
				.reset(reset),
				.din  (ser_wr[g] ? ser_data[6:0] : kbd_data[6:0]),
				.wr   (ser_wr[g] || kbd_wr[g]),
				.full (full[g]),
				.dout (rx_data[7*g+:7]),
				.valid(rx_valid[g]),
				.rd   (rx_ready[g])
			);
		end
	endgenerate

endmodule

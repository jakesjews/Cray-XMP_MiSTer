// Console plumbing for the Cray core.
//
// Bytes the machine prints go to the on-screen terminal and to the HPS serial
// port together, so the serial port sets the pace (115200 baud).  Bytes typed on
// the keyboard or received on the serial port are merged into one input queue.
// A serial BREAK is reported separately for as long as it lasts; the core holds
// the machine in reset during it, so memory can be loaded and then dead started.

module console_io #(
	parameter CLK_HZ = 29400000,
	parameter BAUD   = 115200
) (
	input wire clk,
	input wire reset,
	input wire uart_reset, // reset for the serial receiver only

	// machine side
	input  wire [7:0] tx_data,
	input  wire       tx_valid,
	output wire       tx_ready,

	output wire [7:0] rx_data,
	output wire       rx_valid,
	input  wire       rx_pop,
	output wire [7:0] in_data,   // a byte as it enters the input queue
	output wire       in_stb,

	// terminal side
	output wire [7:0] term_data,
	output wire       term_valid,
	input  wire       term_ready,

	input  wire [7:0] kbd_data,
	input  wire       kbd_valid,
	output wire       kbd_ready,

	// HPS serial port
	input  wire uart_rxd,
	output wire uart_txd,
	output wire uart_break
);

	// ---- output: one queue feeding two sinks that must both accept ----
	wire [7:0] out_data;
	wire out_valid, out_full, uart_ready;
	wire out_pop = out_valid && term_ready && uart_ready;

	assign tx_ready   = ~out_full;
	assign term_data  = out_data;
	assign term_valid = out_valid && uart_ready;

	con_fifo #(
		.AW(6)
	) out_fifo (
		.clk  (clk),
		.reset(reset),
		.din  (tx_data),
		.wr   (tx_valid),
		.full (out_full),
		.dout (out_data),
		.valid(out_valid),
		.rd   (out_pop)
	);

	uart_tx #(
		.CLK_HZ(CLK_HZ),
		.BAUD  (BAUD)
	) utx (
		.clk      (clk),
		.reset    (reset),
		.din      (out_data),
		.din_valid(out_valid && term_ready),
		.din_ready(uart_ready),
		.txd      (uart_txd)
	);

	// ---- input: serial bytes cannot wait, so they go first ----
	wire [7:0] ser_data;
	wire ser_valid, in_full;

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

	assign kbd_ready = ~ser_valid && ~in_full;
	assign in_data   = ser_valid ? ser_data : kbd_data;
	assign in_stb    = ser_valid || kbd_valid;

	con_fifo #(
		.AW(5)
	) in_fifo (
		.clk  (clk),
		.reset(reset),
		.din  (ser_valid ? ser_data : kbd_data),
		.wr   (ser_valid || kbd_valid),
		.full (in_full),
		.dout (rx_data),
		.valid(rx_valid),
		.rd   (rx_pop)
	);

endmodule

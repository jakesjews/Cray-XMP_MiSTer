// A console of the I/O Subsystem: a keyboard channel and a display channel,
// seven bits each way (HR-0030 pages 7-5 and 7-6).  The model is `Console`
// in tools/crates/ios/src/devices.rs.
//
// Keyboard: a key that arrives sets Done.  Function 10 reads it and clears
// Done; function 0 clears Done, and a key that was not read is lost.
// Display: function 14 sends the low seven bits of the accumulator; the
// channel is Busy until the terminal has taken the character and then Done.
// Function 0 clears Busy and Done.  A character always goes out, whether or
// not anything shows it: the kernel stops if its display never answers.
//
// The Interrupt Enable flags of both channels are kept with those of the
// other channels, in iop.v.

module ios_console (
	input wire clk,
	input wire rst,  // Master Clear of the I/O Processor

	// from the I/O Processor: a function for one of the two channels
	input  wire        i_keyboard,      // one clock: for the keyboard channel
	input  wire        i_display,       // one clock: for the display channel
	input  wire [ 3:0] i_function,
	input  wire [ 6:0] i_a,             // the low seven bits of the accumulator
	output reg  [15:0] o_data,          // what function 10 of the keyboard read, in the next clock
	output reg         o_key_done,
	output reg         o_display_busy,
	output reg         o_display_done,

	// the terminal
	input  wire       i_key_valid,   // a key is held out until o_key_ready takes it
	input  wire [6:0] i_key,
	output wire       o_key_ready,
	output reg        o_char_valid,  // a character, held until i_char_ready takes it
	output reg  [6:0] o_char,
	input  wire       i_char_ready
);

	reg [6:0] key;

	assign o_key_ready = !o_key_done && !rst;

	always @(posedge clk) begin
		o_data <= 16'd0;
		if (rst) begin
			o_key_done     <= 1'b0;
			o_display_busy <= 1'b0;
			o_display_done <= 1'b0;
			o_char_valid   <= 1'b0;
		end else begin
			if (i_key_valid && o_key_ready) begin
				key        <= i_key;
				o_key_done <= 1'b1;
			end
			if (o_char_valid && i_char_ready) begin
				o_char_valid   <= 1'b0;
				o_display_busy <= 1'b0;
				o_display_done <= 1'b1;
			end
			if (i_keyboard && ((i_function == 4'o00) || (i_function == 4'o10))) begin
				o_key_done <= 1'b0;
				if (i_function == 4'o10) o_data <= {9'b0, key};
			end
			if (i_display)
				case (i_function)
					4'o00: begin
						o_display_busy <= 1'b0;
						o_display_done <= 1'b0;
					end
					4'o14: begin
						o_char         <= i_a;
						o_char_valid   <= 1'b1;
						o_display_busy <= 1'b1;
						o_display_done <= 1'b0;
					end
					default: ;
				endcase
		end
	end

endmodule

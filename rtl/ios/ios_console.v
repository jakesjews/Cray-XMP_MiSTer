// A console of the I/O Subsystem: a keyboard channel and a display channel,
// seven bits each way (HR-0030 pages 7-5 and 7-6).  The model is `Console`
// in tools/crates/ios/src/devices.rs.
//
// Keyboard: a key that arrives sets Done.  Function 10 reads it and clears
// Done; function 0 clears Done, and a key that was not read is lost.  The
// software reads a key and then clears the channel, so the next key must not
// come in between: it is taken KEY_GAP clocks after the one before was read
// at the earliest, as keys on a serial line follow each other.
// Display: function 14 sends the low seven bits of the accumulator; the
// channel is Busy until the terminal has taken the character and then Done.
// Function 0 clears Busy and Done.  A character always goes out, whether or
// not anything shows it: the kernel stops if its display never answers.
// With i_real the channel is also Busy for the time the character takes on
// its serial line, CHAR_TIME: ten bits at 9,600 baud.  HR-0030 has no rate
// for these channels; 9,600 is what those of the later Models C and D are set
// to by Master Clear (CSM-1009-000 page 4-40).
//
// The Interrupt Enable flags of both channels are kept with those of the
// other channels, in iop.v.

module ios_console #(
	parameter KEY_GAP   = 400000,  // 5 ms
	parameter CHAR_TIME = 83333    // clocks a character to the display takes with i_real
) (
	input wire clk,
	input wire rst,    // Master Clear of the I/O Processor
	input wire i_real, // a character to the display takes the time of its serial line

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

	reg  [ 6:0] key;
	reg  [19:0] gap;  // clocks until the next key may come
	reg  [16:0] sending;  // clocks the character is still on its line
	wire        char_out = o_char_valid && i_char_ready;

	assign o_key_ready = !o_key_done && (gap == 20'd0) && !rst;

	always @(posedge clk) begin
		o_data <= 16'd0;
		if (gap != 20'd0) gap <= gap - 20'd1;
		if (sending != 17'd0) sending <= sending - 17'd1;
		if (rst) begin
			gap            <= 20'd0;
			sending        <= 17'd0;
			o_key_done     <= 1'b0;
			o_display_busy <= 1'b0;
			o_display_done <= 1'b0;
			o_char_valid   <= 1'b0;
		end else begin
			if (i_key_valid && o_key_ready) begin
				key        <= i_key;
				o_key_done <= 1'b1;
			end
			if (char_out) o_char_valid <= 1'b0;
			if (o_display_busy && (char_out || !o_char_valid) && (sending == 17'd0)) begin
				o_display_busy <= 1'b0;
				o_display_done <= 1'b1;
			end
			if (i_keyboard && ((i_function == 4'o00) || (i_function == 4'o10))) begin
				if (o_key_done) gap <= KEY_GAP[19:0];
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
						if (i_real) sending <= CHAR_TIME[16:0];
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

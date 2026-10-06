// What the printer of the CRAY X-MP core prints, for an Ampex screen: a new
// line is a carriage return and a line feed there, a new page is shown as an
// empty line, and what is not a character is left out.

module print_screen (
	input wire clk,
	input wire reset,

	input  wire [7:0] i_char,
	input  wire       i_valid,
	output wire       o_ready,

	output reg  [6:0] o_char,
	output reg        o_valid,
	input  wire       i_ready
);

	reg [1:0] feeds;  // line feeds still to send behind a carriage return

	assign o_ready = !o_valid && (feeds == 2'd0);

	always @(posedge clk) begin
		if (o_valid && i_ready) o_valid <= 1'b0;
		if (reset) begin
			o_valid <= 1'b0;
			feeds   <= 2'd0;
		end else if (i_valid && o_ready) begin
			if ((i_char == 8'h0A) || (i_char == 8'h0C)) begin
				o_char  <= 7'h0D;
				o_valid <= 1'b1;
				feeds   <= (i_char == 8'h0C) ? 2'd2 : 2'd1;
			end else if ((i_char >= 8'h20) && (i_char < 8'h7F)) begin
				o_char  <= i_char[6:0];
				o_valid <= 1'b1;
			end
		end else if (!o_valid && (feeds != 2'd0)) begin
			o_char  <= 7'h0A;
			o_valid <= 1'b1;
			feeds   <= feeds - 2'd1;
		end
	end

endmodule

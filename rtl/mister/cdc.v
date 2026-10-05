// Clock domain crossings between the machine's clock and the clock of the
// console, video and HPS interface.  Nothing here assumes how the two clocks
// are related.

// A level into another clock domain, through two flip-flops.
module cdc_bit (
	input  wire clk,
	input  wire d,
	output wire q
);

	reg [1:0] sync = 2'b00;
	always @(posedge clk) sync <= {sync[0], d};
	assign q = sync[1];

endmodule

// A stream of words from one clock domain to another, one word at a time.
// The source holds the word and flips req; the destination sees the flip two
// clocks later, takes the word, and flips ack once the word has been accepted
// there.  The word is steady the whole time the other side looks at it.
module cdc_stream #(
	parameter W = 8
) (
	input  wire         src_clk,
	input  wire         src_reset,
	input  wire [W-1:0] src_data,
	input  wire         src_valid,
	output wire         src_ready,

	input  wire         dst_clk,
	input  wire         dst_reset,
	output reg  [W-1:0] dst_data,
	output reg          dst_valid,
	input  wire         dst_ready
);

	reg [W-1:0] hold;
	reg         req = 1'b0;
	reg         ack = 1'b0;
	reg         seen = 1'b0;  // the req value the destination has acted on
	wire ack_s, req_d;

	cdc_bit sync_ack (
		.clk(src_clk),
		.d  (ack),
		.q  (ack_s)
	);
	cdc_bit sync_req (
		.clk(dst_clk),
		.d  (req),
		.q  (req_d)
	);

	// source side: free when the last word has been acknowledged
	assign src_ready = (req == ack_s) && !src_reset;

	always @(posedge src_clk) begin
		if (src_reset) req <= ack_s;
		else if (src_valid && src_ready) begin
			hold <= src_data;
			req  <= ~req;
		end
	end

	// destination side
	always @(posedge dst_clk) begin
		if (dst_reset) begin
			dst_valid <= 1'b0;
			seen      <= req_d;
			ack       <= req_d;
		end else begin
			if ((req_d != seen) && !dst_valid) begin
				dst_data  <= hold;
				dst_valid <= 1'b1;
				seen      <= req_d;
			end
			if (dst_valid && dst_ready) begin
				dst_valid <= 1'b0;
				ack       <= seen;
			end
		end
	end

endmodule

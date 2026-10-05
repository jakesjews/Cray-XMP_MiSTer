// Small synchronous first-word-fall-through FIFO, kept in registers: these
// queues are a few dozen bytes, and LUT RAM brought a hold-time violation on
// its write address in one fit.

module con_fifo #(
	parameter AW = 5,
	parameter DW = 8
) (
	input wire clk,
	input wire reset,

	input  wire [DW-1:0] din,
	input  wire          wr,   // ignored when full
	output wire          full,

	output wire [DW-1:0] dout,
	output wire          valid,
	input  wire          rd      // ignored when empty
);

	(* ramstyle = "logic" *) reg [DW-1:0] mem[0:(1<<AW)-1];
	reg [AW:0] wp, rp;

	assign valid = (wp != rp);
	assign full  = (wp[AW] != rp[AW]) && (wp[AW-1:0] == rp[AW-1:0]);
	assign dout  = mem[rp[AW-1:0]];

	always @(posedge clk) begin
		if (reset) begin
			wp <= 0;
			rp <= 0;
		end else begin
			if (wr && !full) begin
				mem[wp[AW-1:0]] <= din;
				wp              <= wp + 1'd1;
			end
			if (rd && valid) rp <= rp + 1'd1;
		end
	end

endmodule

// What passes between the CPU and the I/O Subsystem, which have clocks of
// their own as the two cabinets of the real machine have: 9.5 ns the one,
// 12.5 ns the other, and nothing known about how the two stand to each other.
//
//   xmp_sync        a level, through two flip-flops of the clock it goes to
//   xmp_pulse       a pulse of one period of one clock becomes a pulse of one
//                   period of the other.  Two pulses must be further apart
//                   than three periods of the slower clock, which the channel
//                   pair sees to: every pulse there waits for its answer.
//   xmp_mem_bridge  a memory port of the I/O Subsystem, carried to a port of
//                   the same kind in the CPU's clock
//
// A group of data lines crosses without flip-flops of its own where the side
// that sends it holds it still until the other side has answered: the other
// side looks at it only after a pulse or a level that came through here.
//
// The same modules serve when both sides are given one clock, as the benches
// of the machine without the MiSTer do.

module xmp_sync (
	input  wire clk,
	input  wire i_d,
	output wire o_q
);

	(* altera_attribute = "-name SYNCHRONIZER_IDENTIFICATION FORCED_IF_ASYNCHRONOUS" *)
	reg [1:0] s = 2'b00;
	always @(posedge clk) s <= {s[0], i_d};
	assign o_q = s[1];

endmodule

module xmp_pulse (
	input  wire clk_from,
	input  wire i_pulse,
	input  wire clk_to,
	output reg  o_pulse
);

	// each pulse turns a level over; the other side sees the level change
	reg  t = 1'b0;
	wire t_to;
	reg  t_seen = 1'b0;
	always @(posedge clk_from) if (i_pulse) t <= ~t;

	xmp_sync sync (
		.clk(clk_to),
		.i_d(t),
		.o_q(t_to)
	);

	always @(posedge clk_to) begin
		t_seen  <= t_to;
		o_pulse <= (t_to != t_seen);
	end

endmodule

// The port is the one rtl/mister/ddr3_ports.sv serves: a request with its
// address and data is held until the acknowledge, which is one clock with the
// word read.  A request that is taken back before its acknowledge is carried
// out all the same, as it is there, but is not acknowledged.
module xmp_mem_bridge #(
	parameter AW = 24
) (
	input wire clk,
	input wire clk_ios,
	input wire rst,      // of clk
	input wire rst_ios,  // the same, of clk_ios

	// the port the I/O Subsystem has, in clk_ios
	input  wire          i_req,
	input  wire          i_we,
	input  wire [AW-1:0] i_addr,
	input  wire [  63:0] i_wdata,
	output reg           o_ack,
	output reg  [  63:0] o_rdata,

	// the port to memory, in clk
	output reg           o_req,
	output reg           o_we,
	output reg  [AW-1:0] o_addr,
	output reg  [  63:0] o_wdata,
	input  wire          i_ack,
	input  wire [  63:0] i_rdata
);

	// Each request turns t_req over; the CPU's side has carried a request out
	// when its t_done is the same again.  Address and data stand in registers
	// of the I/O Subsystem's clock all the while, and the word read stands in a
	// register of the CPU's clock from before t_done moves.
	reg        t_req = 1'b0;
	reg        t_done = 1'b0;
	reg        busy = 1'b0;
	reg        orphan;  // the request in hand has been taken back
	reg [63:0] rdata_f;
	wire t_req_f, t_done_s;

	xmp_sync sync_req (
		.clk(clk),
		.i_d(t_req),
		.o_q(t_req_f)
	);
	xmp_sync sync_done (
		.clk(clk_ios),
		.i_d(t_done),
		.o_q(t_done_s)
	);

	always @(posedge clk_ios) begin
		o_ack <= 1'b0;
		if (rst_ios) begin
			busy  <= 1'b0;
			t_req <= 1'b0;
		end else if (!busy) begin
			// a request still standing in the clock of its acknowledge is not looked at
			// yet: it may be the one just carried out, or the next
			if (i_req && !o_ack && (t_done_s == t_req)) begin
				busy    <= 1'b1;
				orphan  <= 1'b0;
				t_req   <= ~t_req;
				o_we    <= i_we;
				o_addr  <= i_addr;
				o_wdata <= i_wdata;
			end
		end else begin
			if (!i_req) orphan <= 1'b1;
			if (t_done_s == t_req) begin
				busy    <= 1'b0;
				o_ack   <= i_req && !orphan;
				o_rdata <= rdata_f;
			end
		end
	end

	always @(posedge clk) begin
		if (rst) begin
			o_req  <= 1'b0;
			t_done <= 1'b0;
		end else if (o_req) begin
			if (i_ack) begin
				o_req   <= 1'b0;
				rdata_f <= i_rdata;
				t_done  <= t_req_f;
			end
		end else if (t_req_f != t_done) o_req <= 1'b1;
	end

endmodule

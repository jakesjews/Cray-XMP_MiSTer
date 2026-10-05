// The I/O Processors of the I/O Subsystem and what joins them: the MIOP
// (IOP 0), the BIOP (IOP 1) and the XIOP (IOP 3), their channels to each
// other, and their ports to Buffer Memory (HR-0030 pages 5-16 to 5-18).
// There is no IOP 2: the channels that would lead to it are dead ends.
//
// Each IOP numbers the others in ascending order, so the channel pairs are
//
//             6, 7     10, 11    12, 13
//   IOP 0     IOP 1    (IOP 2)   IOP 3
//   IOP 1     IOP 0    (IOP 2)   IOP 3
//   IOP 3     IOP 0    IOP 1     (IOP 2)
//
// One IOP starts another through function 1 of its output channel to it:
// bit 0 of the control register is Master Clear, and when that drops with bit
// 1 set the other IOP loads its Local Memory from Buffer Memory (4,096 parcels
// of it with bit 3) and starts.  The MIOP is started by rst: it loads when rst
// drops.
//
// In the buses below element 0 is the MIOP, 1 the BIOP and 2 the XIOP.

module ios_core #(
	parameter [39:0] RAW_REQUEST_0 = 40'd0,  // channels of the MIOP that make their own interrupt request
	parameter        CLOCKS_PER_MS = 80000
) (
	input wire clk,
	input wire rst,

	// Buffer Memory, shared: one word a request, held until its acknowledge
	output reg         o_bm_req,
	output reg         o_bm_we,
	output reg  [23:0] o_bm_addr,
	output reg  [63:0] o_bm_wdata,
	input  wire        i_bm_ack,
	input  wire [63:0] i_bm_rdata,

	// Master Clear of each IOP, for the interfaces on its channels
	output wire [2:0] o_master_clear,

	// channels 14 to 47 octal of each IOP, and its Local Memory for them
	output wire [ 2:0] o_ch_strobe,
	output wire [17:0] o_ch_number,
	output wire [11:0] o_ch_function,
	output wire [47:0] o_ch_a,
	input  wire [47:0] i_ch_data,
	input  wire [83:0] i_ch_busy,
	input  wire [83:0] i_ch_done,
	input  wire [83:0] i_ch_ask,
	input  wire [ 2:0] i_dma_req,
	input  wire [ 2:0] i_dma_we,
	input  wire [47:0] i_dma_addr,
	input  wire [47:0] i_dma_wdata,
	output wire [ 2:0] o_dma_ack,
	output wire [47:0] o_dma_rdata,

	// for the simulation
	output wire [ 2:0] o_step,
	output wire [47:0] o_p
);

	wire [2:0] bm_req, bm_we;
	wire [23:0] bm_addr [0:2];
	wire [63:0] bm_wdata[0:2];
	reg  [ 2:0] bm_ack;

	wire [47:0] word    [0:2];
	wire [ 2:0] sent    [0:2];
	wire [ 2:0] taken   [0:2];
	wire [11:0] control [0:2];
	reg  [47:0] word_in [0:2];
	reg  [ 2:0] sent_in [0:2];
	reg  [ 2:0] taken_in[0:2];

	// the control bits each IOP holds over each other one: [from][to]
	wire [3:0] over_01 = control[0][3:0];
	wire [3:0] over_03 = control[0][11:8];
	wire [3:0] over_10 = control[1][3:0];
	wire [3:0] over_13 = control[1][11:8];
	wire [3:0] over_30 = control[2][3:0];
	wire [3:0] over_31 = control[2][7:4];

	always @(*) begin
		// to the MIOP: from the BIOP's and the XIOP's first pair
		word_in[0]  = {word[2][15:0], 16'b0, word[1][15:0]};
		sent_in[0]  = {sent[2][0], 1'b0, sent[1][0]};
		taken_in[0] = {taken[2][0], 1'b0, taken[1][0]};
		// to the BIOP: from the MIOP's first pair and the XIOP's second
		word_in[1]  = {word[2][31:16], 16'b0, word[0][15:0]};
		sent_in[1]  = {sent[2][1], 1'b0, sent[0][0]};
		taken_in[1] = {taken[2][1], 1'b0, taken[0][0]};
		// to the XIOP: from the MIOP's third pair and the BIOP's third
		word_in[2]  = {16'b0, word[1][47:32], word[0][47:32]};
		sent_in[2]  = {1'b0, sent[1][2], sent[0][2]};
		taken_in[2] = {1'b0, taken[1][2], taken[0][2]};
	end

	// Master Clear of each IOP, and its dead start when that drops
	wire [2:0] clear = {rst | over_03[0] | over_13[0], rst | over_01[0] | over_31[0], rst | over_10[0] | over_30[0]};
	assign o_master_clear = clear;
	reg  [2:0] clear_r;
	reg  [2:0] load;  // the dead start bit was set while Master Clear was
	reg  [2:0] short;
	wire [2:0] start = clear_r & ~clear & load;
	always @(posedge clk) begin
		clear_r <= clear;
		if (rst) begin
			load  <= 3'b001;
			short <= 3'b000;
		end else begin
			if (over_10[0] | over_30[0]) begin
				load[0]  <= (over_10[0] & over_10[1]) | (over_30[0] & over_30[1]);
				short[0] <= (over_10[0] & over_10[3]) | (over_30[0] & over_30[3]);
			end
			if (over_01[0] | over_31[0]) begin
				load[1]  <= (over_01[0] & over_01[1]) | (over_31[0] & over_31[1]);
				short[1] <= (over_01[0] & over_01[3]) | (over_31[0] & over_31[3]);
			end
			if (over_03[0] | over_13[0]) begin
				load[2]  <= (over_03[0] & over_03[1]) | (over_13[0] & over_13[1]);
				short[2] <= (over_03[0] & over_03[3]) | (over_13[0] & over_13[3]);
			end
		end
	end

	genvar g;
	generate
		for (g = 0; g < 3; g = g + 1) begin : g_iop
			iop #(
				.RAW_REQUEST  ((g == 0) ? RAW_REQUEST_0 : 40'd0),
				.CLOCKS_PER_MS(CLOCKS_PER_MS)
			) u (
				.clk           (clk),
				.i_master_clear(clear[g]),
				.i_dead_start  (start[g]),
				.i_short       (short[g]),
				.o_bm_req      (bm_req[g]),
				.o_bm_we       (bm_we[g]),
				.o_bm_addr     (bm_addr[g]),
				.o_bm_wdata    (bm_wdata[g]),
				.i_bm_ack      (bm_ack[g]),
				.i_bm_rdata    (i_bm_rdata),
				.o_link_word   (word[g]),
				.o_link_sent   (sent[g]),
				.o_link_taken  (taken[g]),
				.o_link_control(control[g]),
				.i_link_word   (word_in[g]),
				.i_link_sent   (sent_in[g]),
				.i_link_taken  (taken_in[g]),
				.o_ch_strobe   (o_ch_strobe[g]),
				.o_ch_number   (o_ch_number[6*g+:6]),
				.o_ch_function (o_ch_function[4*g+:4]),
				.o_ch_a        (o_ch_a[16*g+:16]),
				.i_ch_data     (i_ch_data[16*g+:16]),
				.i_ch_busy     (i_ch_busy[28*g+:28]),
				.i_ch_done     (i_ch_done[28*g+:28]),
				.i_ch_ask      (i_ch_ask[28*g+:28]),
				.i_dma_req     (i_dma_req[g]),
				.i_dma_we      (i_dma_we[g]),
				.i_dma_addr    (i_dma_addr[16*g+:16]),
				.i_dma_wdata   (i_dma_wdata[16*g+:16]),
				.o_dma_ack     (o_dma_ack[g]),
				.o_dma_rdata   (o_dma_rdata[16*g+:16]),
				.o_step        (o_step[g]),
				.o_p           (o_p[16*g+:16])
			);
		end
	endgenerate

	// Buffer Memory serves one IOP at a time, the next one in turn
	reg  [1:0] owner;
	reg        serving;
	wire [1:0] after_1 = (owner == 2'd2) ? 2'd0 : (owner + 2'd1);
	wire [1:0] after_2 = (owner == 2'd0) ? 2'd2 : (owner - 2'd1);
	reg  [1:0] pick;
	always @(*) begin
		pick = owner;
		if (bm_req[after_1]) pick = after_1;
		else if (bm_req[after_2]) pick = after_2;
	end
	always @(*) begin
		bm_ack = 3'b0;
		if (serving) bm_ack[owner] = i_bm_ack;
	end
	always @(posedge clk) begin
		if (rst) begin
			serving  <= 1'b0;
			o_bm_req <= 1'b0;
			owner    <= 2'd0;
		end else if (!serving) begin
			if (|bm_req) begin
				owner      <= pick;
				serving    <= 1'b1;
				o_bm_req   <= 1'b1;
				o_bm_we    <= bm_we[pick];
				o_bm_addr  <= bm_addr[pick];
				o_bm_wdata <= bm_wdata[pick];
			end
		end else if (i_bm_ack) begin
			serving  <= 1'b0;
			o_bm_req <= 1'b0;
		end
	end

endmodule

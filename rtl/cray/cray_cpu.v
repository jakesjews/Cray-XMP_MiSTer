// Cray CPU: instruction buffers, functional units and the memory multiplexer.
//
// XMP = 0 builds a CRAY-1 (hardware reference manual 2240004 rev C).
// XMP = 1 keeps the X-MP additions of the source this came from (channels,
// shared registers and semaphores); those paths are compiled but not verified.
//
// Memory port: req, we, burst, addr and wdata are held until the last ack of a
// request.  ack is a one-clock pulse per word with rdata valid in that clock.
// burst is a 16-word read with addr[3:0] = 0.  Bit 63 is Cray bit 0, so parcel 0
// of a word is rdata[63:48].

module cray_cpu #(
	parameter XMP = 0
) (
	input wire clk,
	input wire rst,  // released to dead start from the exchange package at address 0
	input wire i_single_step,  // issue one instruction at a time (test aid)
	input wire i_mcu_int,  // request of the maintenance control unit: sets the MCU interrupt flag outside monitor mode

	output wire        o_mem_req,
	output wire        o_mem_we,
	output wire        o_mem_burst,
	output wire [21:0] o_mem_addr,
	output wire [63:0] o_mem_wdata,
	input  wire        i_mem_ack,
	input  wire [63:0] i_mem_rdata
);

	// instruction buffer signals
	wire        instr_buf_mem_ce;
	wire        instr_buf_mem_burst;
	wire [21:0] instr_buf_mem_addr;
	wire        instr_buf_mem_vld;

	// functional unit signals
	wire [21:0] fu_mem_addr;
	wire [63:0] fu_mem_wr_data;
	wire        fu_mem_wr_en;
	wire        fu_mem_ce;
	wire        fu_mem_burst;
	wire        fu_mem_seq;
	wire [ 1:0] mux_take;
	wire        fu_mem_ack;

	wire [63:0] mem_read_data;

	wire [23:0] p_addr;
	wire [15:0] nip_nxt;
	wire [63:0] word_nxt;
	wire        nip_vld;
	wire        clear_ibufs;
	wire ibuf_busy, ibuf_hold;

	//////////////////////////////////////////////////
	//               Memory multiplexer             //
	//////////////////////////////////////////////////
	// requester 0: instruction buffers, 1: memory functional unit

	wire [1:0] mux_ack;

	cray_mem_mux #(
		.N(2)
	) mem_mux (
		.clk(clk),
		.rst(rst),

		.i_req  ({fu_mem_ce, instr_buf_mem_ce}),
		.i_we   ({fu_mem_wr_en, 1'b0}),
		.i_burst({fu_mem_burst, instr_buf_mem_burst}),
		.i_seq  ({fu_mem_seq, 1'b0}),
		.o_take (mux_take),
		.i_addr ({fu_mem_addr, instr_buf_mem_addr}),
		.i_wdata({fu_mem_wr_data, 64'b0}),
		.o_ack  (mux_ack),
		.o_rdata(mem_read_data),

		.o_mem_req  (o_mem_req),
		.o_mem_we   (o_mem_we),
		.o_mem_burst(o_mem_burst),
		.o_mem_addr (o_mem_addr),
		.o_mem_wdata(o_mem_wdata),
		.i_mem_ack  (i_mem_ack),
		.i_mem_rdata(i_mem_rdata)
	);

	assign instr_buf_mem_vld = mux_ack[0];
	assign fu_mem_ack        = mux_ack[1];

	//////////////////////////////////////////////////
	//             Instruction buffers              //
	//////////////////////////////////////////////////

	i_buf instr_buf (
		.clk        (clk),
		.rst        (rst || clear_ibufs),
		.i_p_addr   (p_addr),
		.o_nip_nxt  (nip_nxt),
		.o_word_nxt (word_nxt),
		.o_nip_vld  (nip_vld),
		.o_mem_ce   (instr_buf_mem_ce),
		.o_mem_burst(instr_buf_mem_burst),
		.o_mem_addr (instr_buf_mem_addr),
		.i_mem_data (mem_read_data),
		.i_mem_vld  (instr_buf_mem_vld),
		.i_hold     (ibuf_hold),
		.o_busy     (ibuf_busy)
	);

	//////////////////////////////////////////////////
	//       Registers and functional units         //
	//////////////////////////////////////////////////

	func_top #(
		.XMP(XMP)
	) cpu (
		.clk            (clk),
		.rst            (rst),
		// instruction buffer interface
		.i_nip_nxt      (nip_nxt),
		.i_word_nxt     (word_nxt),
		.i_nip_vld      (nip_vld),
		.o_clear_ibufs  (clear_ibufs),
		.o_p_addr       (p_addr),
		// memory interface
		.o_mem_addr     (fu_mem_addr),
		.i_data_from_mem(mem_read_data),
		.o_data_to_mem  (fu_mem_wr_data),
		.o_mem_wr_en    (fu_mem_wr_en),
		.o_mem_ce       (fu_mem_ce),
		.o_mem_burst    (fu_mem_burst),
		.o_mem_seq      (fu_mem_seq),
		.i_mem_take     (mux_take[1]),
		.i_mem_ack      (fu_mem_ack),
		.o_debug        (),
		.i_debug_full   (1'b0),
		.i_single_step  (i_single_step),
		.i_mcu_int      (i_mcu_int),
		.i_ibuf_busy    (ibuf_busy),
		.o_ibuf_hold    (ibuf_hold)
	);

endmodule

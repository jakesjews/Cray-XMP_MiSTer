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
	input wire rst,            // released to dead start from the exchange package at address 0
	input wire i_single_step,  // issue one instruction at a time (test aid)
	input wire i_console_int,  // CRAY-1: sets the console interrupt flag outside monitor mode

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
	wire        fu_mem_ack;

	wire [63:0] mem_read_data;

	wire [23:0] p_addr;
	wire [15:0] nip_nxt;
	wire [63:0] word_nxt;
	wire        nip_vld;
	wire        clear_ibufs;
	wire ibuf_busy, ibuf_hold;

	// DMA (I/O channel) signals
	wire [63:0] dma_wr_data;
	wire [21:0] dma_addr;
	wire        dma_req;
	wire        dma_wr;
	wire        dma_ack;

	wire [15:0] dma_instr;
	wire        dma_mon_mode;
	wire        dma_instr_vld;
	wire [23:0] dma_ak;
	wire [23:0] dma_aj;
	wire [23:0] dma_ai;
	wire        dma_int;

	// inter-CPU signals
	wire [ 2:0] cln;
	wire [15:0] intercpu_instr;
	wire        intercpu_mon_mode;
	wire        intercpu_instr_vld;
	wire        intercpu_issue;
	wire [63:0] intercpu_sj;
	wire [63:0] intercpu_si_i, intercpu_si_o;
	wire [23:0] intercpu_ai_i, intercpu_ai_o;

	//////////////////////////////////////////////////
	//               Memory multiplexer             //
	//////////////////////////////////////////////////
	// requester 0: instruction buffers, 1: memory functional unit, 2: channels

	wire [2:0] mux_ack;

	cray_mem_mux #(
		.N(3)
	) mem_mux (
		.clk(clk),
		.rst(rst),

		.i_req  ({dma_req, fu_mem_ce, instr_buf_mem_ce}),
		.i_we   ({dma_wr, fu_mem_wr_en, 1'b0}),
		.i_burst({2'b00, instr_buf_mem_burst}),
		.i_addr ({dma_addr, fu_mem_addr, instr_buf_mem_addr}),
		.i_wdata({dma_wr_data, fu_mem_wr_data, 64'b0}),
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
	assign dma_ack           = mux_ack[2];

	//////////////////////////////////////////////////
	//        I/O channels and shared registers     //
	//////////////////////////////////////////////////

	generate
		if (XMP) begin : g_xmp

			dma_fu dma (
				.clk                      (clk),
				.rst                      (rst),
				.i_cpu0_instr             (dma_instr),
				.i_cpu0_mon_mode          (dma_mon_mode),
				.i_cpu0_instr_vld         (dma_instr_vld),
				.i_cpu0_ak                (dma_ak),
				.i_cpu0_aj                (dma_aj),
				.o_cpu0_ai                (dma_ai),
				.o_cpu0_int               (dma_int),
				.i_cpu1_instr             (16'b0),
				.i_cpu1_mon_mode          (1'b0),
				.i_cpu1_instr_vld         (1'b0),
				.i_cpu1_ak                (24'b0),
				.i_cpu1_aj                (24'b0),
				.o_cpu1_ai                (),
				.o_cpu1_int               (),
				.i_cpu2_instr             (16'b0),
				.i_cpu2_mon_mode          (1'b0),
				.i_cpu2_instr_vld         (1'b0),
				.i_cpu2_ak                (24'b0),
				.i_cpu2_aj                (24'b0),
				.o_cpu2_ai                (),
				.o_cpu2_int               (),
				.i_cpu3_instr             (16'b0),
				.i_cpu3_mon_mode          (1'b0),
				.i_cpu3_instr_vld         (1'b0),
				.i_cpu3_ak                (24'b0),
				.i_cpu3_aj                (24'b0),
				.o_cpu3_ai                (),
				.o_cpu3_int               (),
				.ch06_p_channel_data      (16'b0),
				.ch06_p_channel_srdy      (1'b0),
				.ch06_p_channel_disconnect(1'b0),
				.ch06_p_channel_data_valid(1'b0),
				.ch06_p_channel_drdy      (),
				.ch07_p_channel_data      (),
				.ch07_p_channel_srdy      (),
				.ch07_p_channel_disconnect(),
				.ch07_p_channel_data_valid(),
				.ch07_p_channel_drdy      (1'b0),
				.ch10_p_channel_data      (16'b0),
				.ch10_p_channel_srdy      (1'b0),
				.ch10_p_channel_disconnect(1'b0),
				.ch10_p_channel_data_valid(1'b0),
				.ch10_p_channel_drdy      (),
				.ch11_p_channel_data      (),
				.ch11_p_channel_srdy      (),
				.ch11_p_channel_disconnect(),
				.ch11_p_channel_data_valid(),
				.ch11_p_channel_drdy      (1'b0),
				.ch12_p_channel_data      (16'b0),
				.ch12_p_channel_srdy      (1'b0),
				.ch12_p_channel_disconnect(1'b0),
				.ch12_p_channel_data_valid(1'b0),
				.ch12_p_channel_drdy      (),
				.ch13_p_channel_data      (),
				.ch13_p_channel_srdy      (),
				.ch13_p_channel_disconnect(),
				.ch13_p_channel_data_valid(),
				.ch13_p_channel_drdy      (1'b0),
				.ch14_p_channel_data      (16'b0),
				.ch14_p_channel_srdy      (1'b0),
				.ch14_p_channel_disconnect(1'b0),
				.ch14_p_channel_data_valid(1'b0),
				.ch14_p_channel_drdy      (),
				.ch15_p_channel_data      (),
				.ch15_p_channel_srdy      (),
				.ch15_p_channel_disconnect(),
				.ch15_p_channel_data_valid(),
				.ch15_p_channel_drdy      (1'b0),
				.ch16_p_channel_data      (16'b0),
				.ch16_p_channel_srdy      (1'b0),
				.ch16_p_channel_disconnect(1'b0),
				.ch16_p_channel_data_valid(1'b0),
				.ch16_p_channel_drdy      (),
				.ch17_p_channel_data      (),
				.ch17_p_channel_srdy      (),
				.ch17_p_channel_disconnect(),
				.ch17_p_channel_data_valid(),
				.ch17_p_channel_drdy      (1'b0),
				.o_mem_addr               (dma_addr),
				.o_mem_data               (dma_wr_data),
				.i_mem_data               (mem_read_data),
				.o_mem_req                (dma_req),
				.o_mem_wr                 (dma_wr),
				.i_mem_ack                (dma_ack)
			);

			intercpu_comms intercpu (
				.clk                   (clk),
				.reset                 (rst),
				.i_cln_0               (cln),
				.i_intercpu_instr_0    (intercpu_instr),
				.i_intercpu_mon_mode_0 (intercpu_mon_mode),
				.i_intercpu_instr_vld_0(intercpu_instr_vld),
				.i_intercpu_sj_0       (intercpu_sj),
				.i_intercpu_si_0       (intercpu_si_i),
				.i_intercpu_ai_0       (intercpu_ai_i),
				.o_intercpu_si_0       (intercpu_si_o),
				.o_intercpu_ai_0       (intercpu_ai_o),
				.o_intercpu_issue_0    (intercpu_issue),
				.i_cln_1               (3'b0),
				.i_intercpu_instr_1    (16'b0),
				.i_intercpu_mon_mode_1 (1'b0),
				.i_intercpu_instr_vld_1(1'b0),
				.i_intercpu_sj_1       (64'b0),
				.i_intercpu_si_1       (64'b0),
				.i_intercpu_ai_1       (24'b0),
				.o_intercpu_si_1       (),
				.o_intercpu_ai_1       (),
				.o_intercpu_issue_1    (),
				.i_cln_2               (3'b0),
				.i_intercpu_instr_2    (16'b0),
				.i_intercpu_mon_mode_2 (1'b0),
				.i_intercpu_instr_vld_2(1'b0),
				.i_intercpu_sj_2       (64'b0),
				.i_intercpu_si_2       (64'b0),
				.i_intercpu_ai_2       (24'b0),
				.o_intercpu_si_2       (),
				.o_intercpu_ai_2       (),
				.o_intercpu_issue_2    (),
				.i_cln_3               (3'b0),
				.i_intercpu_instr_3    (16'b0),
				.i_intercpu_mon_mode_3 (1'b0),
				.i_intercpu_instr_vld_3(1'b0),
				.i_intercpu_sj_3       (64'b0),
				.i_intercpu_si_3       (64'b0),
				.i_intercpu_ai_3       (24'b0),
				.o_intercpu_si_3       (),
				.o_intercpu_ai_3       (),
				.o_intercpu_issue_3    ()
			);

		end else begin : g_cray1

			// CRAY-1: nothing is attached to the channels yet and there are no shared
			// registers.  Channel instructions pass and channel status reads as zero.
			assign dma_addr       = 22'b0;
			assign dma_wr_data    = 64'b0;
			assign dma_req        = 1'b0;
			assign dma_wr         = 1'b0;
			assign dma_ai         = 24'b0;
			assign dma_int        = 1'b0;
			assign intercpu_si_o  = 64'b0;
			assign intercpu_ai_o  = 24'b0;
			assign intercpu_issue = 1'b1;

		end
	endgenerate

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
		.clk                 (clk),
		.rst                 (rst),
		.i_cpu_num           (2'b0),
		// instruction buffer interface
		.i_nip_nxt           (nip_nxt),
		.i_word_nxt          (word_nxt),
		.i_nip_vld           (nip_vld),
		.o_clear_ibufs       (clear_ibufs),
		.o_p_addr            (p_addr),
		// I/O interface
		.o_dma_instr         (dma_instr),
		.o_dma_mon_mode      (dma_mon_mode),
		.o_dma_instr_vld     (dma_instr_vld),
		.o_dma_ak            (dma_ak),
		.o_dma_aj            (dma_aj),
		.i_dma_ai            (dma_ai),
		.i_dma_int           (dma_int),
		// memory interface
		.o_mem_addr          (fu_mem_addr),
		.i_data_from_mem     (mem_read_data),
		.o_data_to_mem       (fu_mem_wr_data),
		.o_mem_wr_en         (fu_mem_wr_en),
		.o_mem_ce            (fu_mem_ce),
		.i_mem_ack           (fu_mem_ack),
		// inter-CPU interface
		.o_cln               (cln),
		.o_intercpu_instr    (intercpu_instr),
		.o_intercpu_monmode  (intercpu_mon_mode),
		.o_intercpu_instr_vld(intercpu_instr_vld),
		.i_intercpu_issue    (intercpu_issue),
		.o_intercpu_sj       (intercpu_sj),
		.o_intercpu_si       (intercpu_si_i),
		.o_intercpu_ai       (intercpu_ai_i),
		.i_intercpu_si       (intercpu_si_o),
		.i_intercpu_ai       (intercpu_ai_o),
		.o_debug             (),
		.i_debug_full        (1'b0),
		.i_single_step       (i_single_step),
		.i_console_int       (i_console_int),
		.i_ibuf_busy         (ibuf_busy),
		.o_ibuf_hold         (ibuf_hold)
	);

endmodule

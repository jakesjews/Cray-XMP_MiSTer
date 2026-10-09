// The machine that runs COS: the CPU and the I/O
// Subsystem, joined as they are in a CRAY X-MP.
//
//   - Channel 11 octal of the CPU (output) leads to the MIOP's channel 20, and
//     the MIOP's channel 21 to the CPU's channel 10 (input): the 6 Mbyte pair
//     that carries the packets between COS and the I/O Subsystem.
//   - The MIOP holds the CPU with CPU Master Clear and stops its channels with
//     I/O Master Clear.  The CPU is held from power-on; when the MIOP lets go,
//     it dead starts from the exchange package at address 0, which the I/O
//     Subsystem has put there.
//   - The BIOP's 100 Mbyte channel reaches central memory by a port of its
//     own.  Central memory therefore has two users here, o_mem_* (the CPU)
//     and o_cm_* (the channel); what is outside gives each its turn.
//
// Everything else is the I/O Subsystem's: Buffer Memory, the consoles, the
// tape and the disk of the Peripheral Expander, and the nine disk drives.
//
// The two have clocks of their own, as the two cabinets of the real machine
// have, and nothing is assumed about how the clocks stand to each other.  What
// passes between them goes through rtl/xmp_bridge.v: the pulses of the channel
// pair, the two Master Clear lines, and the I/O Subsystem's three memory ports,
// which arrive outside in the CPU's clock like the CPU's own.  The consoles,
// the disks and the printer are in the I/O Subsystem's clock.

module xmp_machine #(
	parameter CLOCKS_PER_MS  = 80000,  // of clk_ios
	parameter EXPANDER_DELAY = 82,
	parameter DISK_SETTLE    = 200
) (
	input wire clk,      // the CPU's clock; the four memory ports are in it
	input wire clk_ios,  // the I/O Subsystem's; all other ports are in it
	input wire rst,      // of clk
	input wire i_real,   // of clk_ios: the devices take the times of the real ones

	// central memory for the CPU: a word, or a burst of 16 words read
	output wire        o_mem_req,
	output wire        o_mem_we,
	output wire        o_mem_burst,
	output wire [21:0] o_mem_addr,
	output wire [63:0] o_mem_wdata,
	input  wire        i_mem_ack,
	input  wire [63:0] i_mem_rdata,

	// central memory for the 100 Mbyte channel: a word a request
	output wire        o_cm_req,
	output wire        o_cm_we,
	output wire [21:0] o_cm_addr,
	output wire [63:0] o_cm_wdata,
	input  wire        i_cm_ack,
	input  wire [63:0] i_cm_rdata,

	// Buffer Memory
	output wire        o_bm_req,
	output wire        o_bm_we,
	output wire [23:0] o_bm_addr,
	output wire [63:0] o_bm_wdata,
	input  wire        i_bm_ack,
	input  wire [63:0] i_bm_rdata,

	// the consoles: 0 to 3 the MIOP's (3 the kernel's, 0 the station's), 4
	// the BIOP's, 5 the XIOP's
	input  wire [ 5:0] i_key_valid,
	input  wire [41:0] i_key,
	output wire [ 5:0] o_key_ready,
	output wire [ 5:0] o_char_valid,
	output wire [41:0] o_char,
	input  wire [ 5:0] i_char_ready,

	// the boot tape, a .tap file in 64-bit words
	output wire        o_tape_req,
	output wire [20:0] o_tape_addr,
	input  wire        i_tape_ack,
	input  wire [63:0] i_tape_data,
	input  wire [23:0] i_tape_bytes,

	// a tape file for that drive in place of the boot tape (rtl/ios/ios_reel.v),
	// of clk_ios:
	// blocks of 512 bytes, as hps_io moves them
	input  wire        i_reel_mounted,    // one clock: a file of i_reel_blocks blocks was chosen; none if that is zero
	input  wire [54:0] i_reel_blocks,
	input  wire        i_reel_readonly,
	output wire [31:0] o_reel_lba,
	output wire        o_reel_rd,
	output wire        o_reel_wr,
	input  wire        i_reel_ack,
	input  wire [ 8:0] i_reel_buff_addr,
	input  wire [ 7:0] i_reel_buff_dout,
	output wire [ 7:0] o_reel_buff_din,
	input  wire        i_reel_buff_wr,

	// the disk of the Peripheral Expander
	output wire [31:0] o_sd_lba,
	output wire        o_sd_rd,
	output wire        o_sd_wr,
	input  wire        i_sd_ack,
	input  wire [ 8:0] i_sd_buff_addr,
	input  wire [ 7:0] i_sd_buff_dout,
	output wire [ 7:0] o_sd_buff_din,
	input  wire        i_sd_buff_wr,

	// the printer on the Peripheral Expander: a character, held out until it
	// is taken; a form feed is a new page, a line feed a new line
	output wire       o_print_valid,
	output wire [7:0] o_print,
	input  wire       i_print_ready,

	// the nine DD-29 drives
	output wire [31:0] o_drive_lba,
	output wire [ 8:0] o_drive_rd,
	output wire [ 8:0] o_drive_wr,
	input  wire [ 8:0] i_drive_ack,
	input  wire [11:0] i_drive_buff_addr,
	input  wire [ 7:0] i_drive_buff_dout,
	output wire [ 7:0] o_drive_buff_din,
	input  wire        i_drive_buff_wr,

	// for the simulation
	output wire        o_cpu_held,  // CPU Master Clear
	output wire [ 2:0] o_step,
	output wire [47:0] o_p
);

	// the reset as the I/O Subsystem sees it, and its two Master Clear lines as
	// the CPU sees them
	wire rst_ios, cpu_clear, io_clear, cpu_clear_f, io_clear_f;
	xmp_sync sync_rst (
		.clk(clk_ios),
		.i_d(rst),
		.o_q(rst_ios)
	);
	xmp_sync sync_cpu_clear (
		.clk(clk),
		.i_d(cpu_clear),
		.o_q(cpu_clear_f)
	);
	xmp_sync sync_io_clear (
		.clk(clk),
		.i_d(io_clear),
		.o_q(io_clear_f)
	);

	// The CPU is held from the reset until the MIOP lets go.  (The line is set
	// while the I/O Subsystem is in reset; this covers the clocks until that is
	// seen here.)
	reg       cpu_rst;
	reg [3:0] settle;
	always @(posedge clk) begin
		if (rst) settle <= 4'd15;
		else if (settle != 4'd0) settle <= settle - 4'd1;
		cpu_rst <= rst || (settle != 4'd0) || cpu_clear_f;
	end
	assign o_cpu_held = cpu_rst;

	// The channel pair between them.  A parcel stands on its lines from its
	// Ready to its Resume, so it is there when the Ready arrives on the other
	// side.
	wire [3:0] out_ready, out_disconnect, in_resume;
	wire [63:0] out_data;
	wire to_cpu_ready, to_cpu_disconnect, from_cpu_resume;
	wire [15:0] to_cpu_parcel;
	wire ios_ready, ios_disconnect, ios_resume;
	wire cpu_ready, cpu_disconnect, cpu_resume;

	xmp_pulse ready_out (
		.clk_from(clk),
		.i_pulse (out_ready[0]),
		.clk_to  (clk_ios),
		.o_pulse (cpu_ready)
	);
	xmp_pulse disconnect_out (
		.clk_from(clk),
		.i_pulse (out_disconnect[0]),
		.clk_to  (clk_ios),
		.o_pulse (cpu_disconnect)
	);
	xmp_pulse resume_out (
		.clk_from(clk),
		.i_pulse (in_resume[0]),
		.clk_to  (clk_ios),
		.o_pulse (cpu_resume)
	);
	xmp_pulse ready_in (
		.clk_from(clk_ios),
		.i_pulse (ios_ready),
		.clk_to  (clk),
		.o_pulse (to_cpu_ready)
	);
	xmp_pulse disconnect_in (
		.clk_from(clk_ios),
		.i_pulse (ios_disconnect),
		.clk_to  (clk),
		.o_pulse (to_cpu_disconnect)
	);
	xmp_pulse resume_in (
		.clk_from(clk_ios),
		.i_pulse (ios_resume),
		.clk_to  (clk),
		.o_pulse (from_cpu_resume)
	);

	// the I/O Subsystem's memory ports
	wire cm_req, cm_we, cm_ack, bm_req, bm_we, bm_ack, tape_req, tape_ack;
	wire [21:0] cm_addr;
	wire [23:0] bm_addr;
	wire [20:0] tape_addr;
	wire [63:0] cm_wdata, cm_rdata, bm_wdata, bm_rdata, tape_rdata;
	reg [23:0] tape_bytes;
	always @(posedge clk_ios) tape_bytes <= i_tape_bytes;

	xmp_mem_bridge #(
		.AW(22)
	) cm_bridge (
		.clk    (clk),
		.clk_ios(clk_ios),
		.rst    (rst),
		.rst_ios(rst_ios),
		.i_req  (cm_req),
		.i_we   (cm_we),
		.i_addr (cm_addr),
		.i_wdata(cm_wdata),
		.o_ack  (cm_ack),
		.o_rdata(cm_rdata),
		.o_req  (o_cm_req),
		.o_we   (o_cm_we),
		.o_addr (o_cm_addr),
		.o_wdata(o_cm_wdata),
		.i_ack  (i_cm_ack),
		.i_rdata(i_cm_rdata)
	);
	xmp_mem_bridge #(
		.AW(24)
	) bm_bridge (
		.clk    (clk),
		.clk_ios(clk_ios),
		.rst    (rst),
		.rst_ios(rst_ios),
		.i_req  (bm_req),
		.i_we   (bm_we),
		.i_addr (bm_addr),
		.i_wdata(bm_wdata),
		.o_ack  (bm_ack),
		.o_rdata(bm_rdata),
		.o_req  (o_bm_req),
		.o_we   (o_bm_we),
		.o_addr (o_bm_addr),
		.o_wdata(o_bm_wdata),
		.i_ack  (i_bm_ack),
		.i_rdata(i_bm_rdata)
	);
	xmp_mem_bridge #(
		.AW(21)
	) tape_bridge (
		.clk    (clk),
		.clk_ios(clk_ios),
		.rst    (rst),
		.rst_ios(rst_ios),
		.i_req  (tape_req),
		.i_we   (1'b0),
		.i_addr (tape_addr),
		.i_wdata(64'd0),
		.o_ack  (tape_ack),
		.o_rdata(tape_rdata),
		.o_req  (o_tape_req),
		.o_we   (),
		.o_addr (o_tape_addr),
		.o_wdata(),
		.i_ack  (i_tape_ack),
		.i_rdata(i_tape_data)
	);

	cray_cpu cpu (
		.clk          (clk),
		.rst          (cpu_rst),
		.i_single_step(1'b0),
		.i_mcu_int    (1'b0),
		.i_io_clear   (io_clear_f),

		.o_mem_req  (o_mem_req),
		.o_mem_we   (o_mem_we),
		.o_mem_burst(o_mem_burst),
		.o_mem_addr (o_mem_addr),
		.o_mem_wdata(o_mem_wdata),
		.i_mem_ack  (i_mem_ack),
		.i_mem_rdata(i_mem_rdata),

		// pair 0 is channels 10 and 11; nothing is on the other three
		.i_ch_in_ready      ({3'b0, to_cpu_ready}),
		.i_ch_in_data       ({48'b0, to_cpu_parcel}),
		.o_ch_in_resume     (in_resume),
		.i_ch_in_disconnect ({3'b0, to_cpu_disconnect}),
		.o_ch_out_ready     (out_ready),
		.o_ch_out_data      (out_data),
		.i_ch_out_resume    ({3'b0, from_cpu_resume}),
		.o_ch_out_disconnect(out_disconnect),
		.o_ch_out_mc        ()
	);

	ios #(
		.CLOCKS_PER_MS (CLOCKS_PER_MS),
		.EXPANDER_DELAY(EXPANDER_DELAY),
		.DISK_SETTLE   (DISK_SETTLE)
	) subsystem (
		.clk               (clk_ios),
		.rst               (rst_ios),
		.i_real            (i_real),
		.o_bm_req          (bm_req),
		.o_bm_we           (bm_we),
		.o_bm_addr         (bm_addr),
		.o_bm_wdata        (bm_wdata),
		.i_bm_ack          (bm_ack),
		.i_bm_rdata        (bm_rdata),
		.i_key_valid       (i_key_valid),
		.i_key             (i_key),
		.o_key_ready       (o_key_ready),
		.o_char_valid      (o_char_valid),
		.o_char            (o_char),
		.i_char_ready      (i_char_ready),
		.o_tape_req        (tape_req),
		.o_tape_addr       (tape_addr),
		.i_tape_ack        (tape_ack),
		.i_tape_data       (tape_rdata),
		.i_tape_bytes      (tape_bytes),
		.i_reel_mounted    (i_reel_mounted),
		.i_reel_blocks     (i_reel_blocks),
		.i_reel_readonly   (i_reel_readonly),
		.o_reel_lba        (o_reel_lba),
		.o_reel_rd         (o_reel_rd),
		.o_reel_wr         (o_reel_wr),
		.i_reel_ack        (i_reel_ack),
		.i_reel_buff_addr  (i_reel_buff_addr),
		.i_reel_buff_dout  (i_reel_buff_dout),
		.o_reel_buff_din   (o_reel_buff_din),
		.i_reel_buff_wr    (i_reel_buff_wr),
		.o_sd_lba          (o_sd_lba),
		.o_sd_rd           (o_sd_rd),
		.o_sd_wr           (o_sd_wr),
		.i_sd_ack          (i_sd_ack),
		.i_sd_buff_addr    (i_sd_buff_addr),
		.i_sd_buff_dout    (i_sd_buff_dout),
		.o_sd_buff_din     (o_sd_buff_din),
		.i_sd_buff_wr      (i_sd_buff_wr),
		.o_print_valid     (o_print_valid),
		.o_print           (o_print),
		.i_print_ready     (i_print_ready),
		.i_cpu_ready       (cpu_ready),
		.i_cpu_parcel      (out_data[15:0]),
		.o_cpu_resume      (ios_resume),
		.i_cpu_disconnect  (cpu_disconnect),
		.o_cpu_ready       (ios_ready),
		.o_cpu_parcel      (to_cpu_parcel),
		.i_cpu_resume      (cpu_resume),
		.o_cpu_disconnect  (ios_disconnect),
		.o_cpu_master_clear(cpu_clear),
		.o_io_master_clear (io_clear),
		.o_cm_req          (cm_req),
		.o_cm_we           (cm_we),
		.o_cm_addr         (cm_addr),
		.o_cm_wdata        (cm_wdata),
		.i_cm_ack          (cm_ack),
		.i_cm_rdata        (cm_rdata),
		.o_drive_lba       (o_drive_lba),
		.o_drive_rd        (o_drive_rd),
		.o_drive_wr        (o_drive_wr),
		.i_drive_ack       (i_drive_ack),
		.i_drive_buff_addr (i_drive_buff_addr),
		.i_drive_buff_dout (i_drive_buff_dout),
		.o_drive_buff_din  (o_drive_buff_din),
		.i_drive_buff_wr   (i_drive_buff_wr),
		.o_step            (o_step),
		.o_p               (o_p)
	);

endmodule

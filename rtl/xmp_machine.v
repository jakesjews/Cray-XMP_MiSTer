// The machine that runs COS: the CPU with the X-MP features and the I/O
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

module xmp_machine #(
	parameter CLOCKS_PER_MS  = 81667,  // of clk
	parameter EXPANDER_DELAY = 82,
	parameter DISK_SETTLE    = 200
) (
	input wire clk,
	input wire rst,

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

	wire cpu_clear, io_clear;
	reg cpu_rst;
	always @(posedge clk) cpu_rst <= rst || cpu_clear;
	assign o_cpu_held = cpu_rst;

	// the channel pair between them
	wire [3:0] out_ready, out_disconnect, in_resume;
	wire [63:0] out_data;
	wire to_cpu_ready, to_cpu_disconnect, from_cpu_resume;
	wire [15:0] to_cpu_parcel;

	cray_cpu #(
		.XMP(1)
	) cpu (
		.clk          (clk),
		.rst          (cpu_rst),
		.i_single_step(1'b0),
		.i_mcu_int    (1'b0),
		.i_io_clear   (io_clear),

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
		.clk               (clk),
		.rst               (rst),
		.o_bm_req          (o_bm_req),
		.o_bm_we           (o_bm_we),
		.o_bm_addr         (o_bm_addr),
		.o_bm_wdata        (o_bm_wdata),
		.i_bm_ack          (i_bm_ack),
		.i_bm_rdata        (i_bm_rdata),
		.i_key_valid       (i_key_valid),
		.i_key             (i_key),
		.o_key_ready       (o_key_ready),
		.o_char_valid      (o_char_valid),
		.o_char            (o_char),
		.i_char_ready      (i_char_ready),
		.o_tape_req        (o_tape_req),
		.o_tape_addr       (o_tape_addr),
		.i_tape_ack        (i_tape_ack),
		.i_tape_data       (i_tape_data),
		.i_tape_bytes      (i_tape_bytes),
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
		.i_cpu_ready       (out_ready[0]),
		.i_cpu_parcel      (out_data[15:0]),
		.o_cpu_resume      (from_cpu_resume),
		.i_cpu_disconnect  (out_disconnect[0]),
		.o_cpu_ready       (to_cpu_ready),
		.o_cpu_parcel      (to_cpu_parcel),
		.i_cpu_resume      (in_resume[0]),
		.o_cpu_disconnect  (to_cpu_disconnect),
		.o_cpu_master_clear(cpu_clear),
		.o_io_master_clear (io_clear),
		.o_cm_req          (o_cm_req),
		.o_cm_we           (o_cm_we),
		.o_cm_addr         (o_cm_addr),
		.o_cm_wdata        (o_cm_wdata),
		.i_cm_ack          (i_cm_ack),
		.i_cm_rdata        (i_cm_rdata),
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

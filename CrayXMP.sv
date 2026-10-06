//============================================================================
//
//  This program is free software; you can redistribute it and/or modify it
//  under the terms of the GNU General Public License as published by the Free
//  Software Foundation; either version 2 of the License, or (at your option)
//  any later version.
//
//  This program is distributed in the hope that it will be useful, but WITHOUT
//  ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
//  FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License for
//  more details.
//
//  You should have received a copy of the GNU General Public License along
//  with this program; if not, write to the Free Software Foundation, Inc.,
//  51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
//
//============================================================================

// CRAY X-MP with its I/O Subsystem: the machine COS 1.17 runs on.

module emu (
	`include "sys/emu_ports.vh"
);

	///////// Default values for ports not used in this core /////////

	assign ADC_BUS = 'Z;
	assign USER_OUT = '1;
	assign {UART_RTS, UART_DTR} = 0;
	assign {SD_SCK, SD_MOSI, SD_CS} = 'Z;
	assign {SDRAM_DQ, SDRAM_A, SDRAM_BA, SDRAM_CLK, SDRAM_CKE, SDRAM_DQML, SDRAM_DQMH, SDRAM_nWE, SDRAM_nCAS, SDRAM_nRAS, SDRAM_nCS} = 'Z;

	assign VGA_SL         = 0;
	assign VGA_F1         = 0;
	// The 936x524 raster is not a standard VGA mode, so the analog output goes
	// through the scaler.  With direct video the scaler must not be forced, or HDMI
	// would carry the scaler's output instead of the core's own timing.
	assign VGA_SCALER     = ~direct_video;
	assign VGA_DISABLE    = 0;
	assign HDMI_FREEZE    = 0;
	assign HDMI_BLACKOUT  = 0;
	assign HDMI_BOB_DEINT = 0;

	assign AUDIO_S   = 0;
	assign AUDIO_L   = 0;
	assign AUDIO_R   = 0;
	assign AUDIO_MIX = 0;

	assign LED_POWER = 0;
	assign BUTTONS   = 0;

	//////////////////////////////////////////////////////////////////

	wire [1:0] ar = status[122:121];

	assign VIDEO_ARX = (ar == 2'd0) ? 13'd4 : {11'd0, ar - 2'd1};
	assign VIDEO_ARY = (ar == 2'd0) ? 13'd3 : 13'd0;

	// The boot file (tools/py/mkboot.py) holds the kernel of the I/O Subsystem and
	// its boot tape.  The load address makes the MiSTer write it straight into
	// DDR3, above central memory and Buffer Memory.  The disks are images that
	// stay on the SD card: slot 0 the disk on the Peripheral Expander, slot 1 the
	// nine DD-29 drives on the BIOP's channels 20 to 32 (octal), one after
	// another in one file.  Slot 2 is a text file that takes what the printer
	// on the Peripheral Expander prints.
	`include "build_id.v"
	localparam CONF_STR = {
		"CrayXMP;;",
		"-;",
		"F1,IOS,Load boot file,34000000;",
		"S0,IMG,Expander disk;",
		"S1,IMG,Disk drives;",
		"S2,TXT,Printer file;",
		"-;",
		"O[122:121],Aspect ratio,Original,Full Screen,[ARC1],[ARC2];",
		"O[4:3],Text color,White,Green,Amber,Cyan;",
		"O[14],Font,8x16 (31kHz),8x8 (15kHz);",
		"-;",
		"T[0],Reset;",
		"R[0],Reset and close OSD;",
		"v,0;",  // [optional] config version 0-99.
				 // If CONF_STR options are changed in incompatible way, then change version number too,
				 // so all options will get default values on first start.
		"V,v",
		`BUILD_DATE
	};

	///////////////////////   CLOCKS   ///////////////////////////////

	// clk_sys is the video clock and runs the two screens, the keyboard and the
	// serial port.  The machine, its memory and the HPS interface, which brings
	// the disks' sectors, have their own, faster clock; what passes between the
	// two goes through rtl/mister/cdc.v.
	localparam CLK_HZ = 29400000;
	localparam CPU_HZ = 73500000;

	wire clk_sys, clk_cpu;
	pll pll (
		.refclk  (CLK_50M),
		.rst     (1'b0),
		.outclk_0(clk_sys),
		.outclk_1(clk_cpu),
		.locked  ()
	);

	///////////////////////   HPS   //////////////////////////////////

	wire         direct_video;
	wire [  1:0] buttons;
	wire [127:0] status;
	wire [ 10:0] ps2_key;
	wire         ioctl_download;

	// the disks: 512-byte blocks, one for a sector of the expander disk and
	// eight for a sector of a DD-29
	wire [31:0] sd_lba    [3];
	wire [ 5:0] sd_blk_cnt[3];
	wire [2:0] sd_rd, sd_wr, sd_ack;
	wire [13:0] sd_buff_addr;
	wire [ 7:0] sd_buff_dout;
	wire [ 7:0] sd_buff_din  [3];
	wire        sd_buff_wr;
	wire [ 2:0] img_mounted;
	wire [63:0] img_size;

	hps_io #(
		.CONF_STR(CONF_STR),
		.VDNUM   (3),
		.BLKSZ   (2)
	) hps_io (
		.clk_sys  (clk_cpu),
		.HPS_BUS  (HPS_BUS),
		.EXT_BUS  (),
		.gamma_bus(),

		.direct_video(direct_video),

		.buttons        (buttons),
		.status         (status),
		.status_menumask(16'd0),

		.ioctl_download(ioctl_download),

		.img_mounted (img_mounted),
		.img_size    (img_size),
		.sd_lba      (sd_lba),
		.sd_blk_cnt  (sd_blk_cnt),
		.sd_rd       (sd_rd),
		.sd_wr       (sd_wr),
		.sd_ack      (sd_ack),
		.sd_buff_addr(sd_buff_addr),
		.sd_buff_dout(sd_buff_dout),
		.sd_buff_din (sd_buff_din),
		.sd_buff_wr  (sd_buff_wr),

		.ps2_key(ps2_key)
	);

	///////////////////////   RESET   ////////////////////////////////

	// Everything except the video raster is held in reset while the menu asks for
	// it, while the menu is writing the boot file into memory, and for as long as
	// a serial BREAK lasts, which is how a test script starts the machine again.
	// The serial receiver has to keep running through that reset, so it has its own.
	wire uart_break, uart_break_cpu;
	cdc_bit sync_break (
		.clk(clk_cpu),
		.d  (uart_break),
		.q  (uart_break_cpu)
	);

	wire host_reset = RESET | status[0] | buttons[1] | ioctl_download;
	reg reset_cpu, uart_reset_cpu;
	reg [15:0] reset_cnt;

	always @(posedge clk_cpu) begin
		uart_reset_cpu <= host_reset;
		if (host_reset | uart_break_cpu) begin
			reset_cpu <= 1;
			reset_cnt <= '1;
		end else if (reset_cnt != 0) reset_cnt <= reset_cnt - 1'd1;
		else reset_cpu <= 0;
	end

	// the same in the video clock's domain
	wire reset, uart_reset;
	cdc_bit sync_reset (
		.clk(clk_sys),
		.d  (reset_cpu),
		.q  (reset)
	);
	cdc_bit sync_uart_reset (
		.clk(clk_sys),
		.d  (uart_reset_cpu),
		.q  (uart_reset)
	);

	///////////////////////   MEMORY   ///////////////////////////////

	// All of the machine's large memories are in DDR3, in 64-bit words from
	// physical 0x3000_0000:
	//   0x000_0000  central memory, 4 million words
	//   0x040_0000  Buffer Memory of the I/O Subsystem, 4 million words
	//   0x080_0000  the boot file, which holds the boot tape
	// They have five users: the CPU, the 100 Mbyte channel of the BIOP, the
	// I/O Processors' Buffer Memory channels, the tape drive on the Peripheral
	// Expander, and the part of the core that prepares a start.
	localparam [2:0] CENTRAL = 3'b000;
	localparam [2:0] BUFFER  = 3'b001;

	wire mem_req, mem_we, mem_burst, mem_ack;
	wire [21:0] mem_addr;
	wire [63:0] mem_wdata;

	wire cm_req, cm_we, cm_ack;
	wire [21:0] cm_addr;
	wire [63:0] cm_wdata;

	wire bm_req, bm_we, bm_ack;
	wire [23:0] bm_addr;
	wire [63:0] bm_wdata;

	wire tape_req, tape_ack;
	wire [20:0] tape_addr;
	wire [24:0] tape_base;
	wire [23:0] tape_bytes;

	wire boot_req, boot_we, boot_ack;
	wire [24:0] boot_addr;
	wire [63:0] boot_wdata;
	wire boot_done, boot_missing;

	wire [63:0] mem_rdata;

	ddr3_ports #(
		.N(5)
	) ddr3 (
		.clk  (clk_cpu),
		.reset(reset_cpu),

		.req  ({boot_req, tape_req, bm_req, cm_req, mem_req}),
		.we   ({boot_we, 1'b0, bm_we, cm_we, mem_we}),
		.burst({4'b0000, mem_burst}),
		.addr ({boot_addr, tape_base + {4'd0, tape_addr}, BUFFER, bm_addr[21:0], CENTRAL, cm_addr, CENTRAL, mem_addr}),
		.wdata({boot_wdata, 64'd0, bm_wdata, cm_wdata, mem_wdata}),
		.ack  ({boot_ack, tape_ack, bm_ack, cm_ack, mem_ack}),
		.rdata(mem_rdata),

		.DDRAM_CLK       (DDRAM_CLK),
		.DDRAM_BUSY      (DDRAM_BUSY),
		.DDRAM_BURSTCNT  (DDRAM_BURSTCNT),
		.DDRAM_ADDR      (DDRAM_ADDR),
		.DDRAM_DOUT      (DDRAM_DOUT),
		.DDRAM_DOUT_READY(DDRAM_DOUT_READY),
		.DDRAM_RD        (DDRAM_RD),
		.DDRAM_DIN       (DDRAM_DIN),
		.DDRAM_BE        (DDRAM_BE),
		.DDRAM_WE        (DDRAM_WE)
	);

	// After a reset the boot file is checked and its kernel put into Buffer
	// Memory; the machine starts when that is done.
	xmp_boot #(
		.FILE  (25'h0800000),
		.BUFFER({BUFFER, 22'd0})
	) boot (
		.clk  (clk_cpu),
		.reset(reset_cpu),

		.o_req  (boot_req),
		.o_we   (boot_we),
		.o_addr (boot_addr),
		.o_wdata(boot_wdata),
		.i_ack  (boot_ack),
		.i_rdata(mem_rdata),

		.o_done      (boot_done),
		.o_missing   (boot_missing),
		.o_tape_bytes(tape_bytes),
		.o_tape      (tape_base)
	);

	///////////////////////   MACHINE   //////////////////////////////

	// Of the six consoles of the I/O Subsystem two are shown: the operator's
	// console of the MIOP (its channels 46 and 47, console 3) and the station
	// (channels 40 and 41, console 0).  What the others print is dropped.
	wire [5:0] key_valid, key_ready, char_valid, char_ready;
	wire [41:0] key, char;

	wire [31:0] exp_lba, drive_lba;
	wire [7:0] exp_din, drive_din;
	wire [8:0] drive_rd, drive_wr;
	wire exp_rd, exp_wr;
	wire cpu_held;

	wire [7:0] print_char;
	wire print_valid, print_ready;

	xmp_machine #(
		.CLOCKS_PER_MS(CPU_HZ / 1000)
	) machine (
		.clk(clk_cpu),
		.rst(reset_cpu | ~boot_done),

		.o_mem_req  (mem_req),
		.o_mem_we   (mem_we),
		.o_mem_burst(mem_burst),
		.o_mem_addr (mem_addr),
		.o_mem_wdata(mem_wdata),
		.i_mem_ack  (mem_ack),
		.i_mem_rdata(mem_rdata),

		.o_cm_req  (cm_req),
		.o_cm_we   (cm_we),
		.o_cm_addr (cm_addr),
		.o_cm_wdata(cm_wdata),
		.i_cm_ack  (cm_ack),
		.i_cm_rdata(mem_rdata),

		.o_bm_req  (bm_req),
		.o_bm_we   (bm_we),
		.o_bm_addr (bm_addr),
		.o_bm_wdata(bm_wdata),
		.i_bm_ack  (bm_ack),
		.i_bm_rdata(mem_rdata),

		.i_key_valid (key_valid),
		.i_key       (key),
		.o_key_ready (key_ready),
		.o_char_valid(char_valid),
		.o_char      (char),
		.i_char_ready(char_ready),

		.o_tape_req  (tape_req),
		.o_tape_addr (tape_addr),
		.i_tape_ack  (tape_ack),
		.i_tape_data (mem_rdata),
		.i_tape_bytes(tape_bytes),

		.o_sd_lba      (exp_lba),
		.o_sd_rd       (exp_rd),
		.o_sd_wr       (exp_wr),
		.i_sd_ack      (sd_ack[0]),
		.i_sd_buff_addr(sd_buff_addr[8:0]),
		.i_sd_buff_dout(sd_buff_dout),
		.o_sd_buff_din (exp_din),
		.i_sd_buff_wr  (sd_buff_wr),

		.o_print_valid(print_valid),
		.o_print      (print_char),
		.i_print_ready(print_ready),

		.o_drive_lba      (drive_lba),
		.o_drive_rd       (drive_rd),
		.o_drive_wr       (drive_wr),
		.i_drive_ack      ({9{sd_ack[1]}}),
		.i_drive_buff_addr(sd_buff_addr[11:0]),
		.i_drive_buff_dout(sd_buff_dout),
		.o_drive_buff_din (drive_din),
		.i_drive_buff_wr  (sd_buff_wr),

		.o_cpu_held(cpu_held),
		.o_step    (),
		.o_p       ()
	);

	// A DD-29 is 823 cylinders of 10 head groups of 18 sectors, 1,185,120 blocks;
	// the drive that asks says where in the file its blocks begin.
	localparam [31:0] DRIVE_BLOCKS = 32'd1185120;

	reg [31:0] drive_base;
	always @(*) begin
		drive_base = 32'd0;
		for (int n = 1; n < 9; n = n + 1) if (drive_rd[n] | drive_wr[n]) drive_base = n * DRIVE_BLOCKS;
	end

	// What the printer prints goes to a text file and to a screen; a character
	// is taken when both can take it.
	wire [31:0] spool_lba;
	wire [ 7:0] spool_din;
	wire spool_rd, spool_wr, spool_ready, shown_ready;
	assign print_ready = spool_ready && shown_ready;

	print_spool spool (
		.clk(clk_cpu),

		.i_char (print_char),
		.i_valid(print_valid && shown_ready),
		.o_ready(spool_ready),

		.i_mounted  (img_mounted[2]),
		.i_blocks   (img_size[40:9]),
		.o_lba      (spool_lba),
		.o_rd       (spool_rd),
		.o_wr       (spool_wr),
		.i_ack      (sd_ack[2]),
		.i_buff_addr(sd_buff_addr[8:0]),
		.i_buff_dout(sd_buff_dout),
		.o_buff_din (spool_din),
		.i_buff_wr  (sd_buff_wr && sd_ack[2])
	);

	assign sd_rd          = {spool_rd, |drive_rd, exp_rd};
	assign sd_wr          = {spool_wr, |drive_wr, exp_wr};
	assign sd_lba[0]      = exp_lba;
	assign sd_lba[1]      = drive_base + drive_lba;
	assign sd_lba[2]      = spool_lba;
	assign sd_blk_cnt[0]  = 6'd0;
	assign sd_blk_cnt[1]  = 6'd7;
	assign sd_blk_cnt[2]  = 6'd0;
	assign sd_buff_din[0] = exp_din;
	assign sd_buff_din[1] = drive_din;
	assign sd_buff_din[2] = spool_din;

	// stretch activity so it is visible
	reg [19:0] act_cnt, disk_cnt;
	always @(posedge clk_cpu) begin
		if (mem_req && !cpu_held) act_cnt <= '1;
		else if (act_cnt != 0) act_cnt <= act_cnt - 1'd1;
		if (|sd_ack) disk_cnt <= '1;
		else if (disk_cnt != 0) disk_cnt <= disk_cnt - 1'd1;
	end

	assign LED_USER = (act_cnt != 0);
	assign LED_DISK = {1'b1, disk_cnt != 0};

	///////////////////////   CONSOLES   /////////////////////////////

	localparam OPERATOR = 3;
	localparam STATION  = 0;

	assign char_ready[1] = 1'b1;
	assign char_ready[2] = 1'b1;
	assign char_ready[4] = 1'b1;
	assign char_ready[5] = 1'b1;
	assign key_valid[1]  = 1'b0;
	assign key_valid[2]  = 1'b0;
	assign key_valid[4]  = 1'b0;
	assign key_valid[5]  = 1'b0;
	assign key[13:7]     = 7'd0;
	assign key[20:14]    = 7'd0;
	assign key[41:28]    = 14'd0;

	// index 0 is the operator's console and 1 the station from here on
	wire [13:0] con_tx_data, con_rx_data, term_data;
	wire [1:0] con_tx_valid, con_tx_ready, con_rx_valid, con_rx_ready, term_valid, term_ready;

	cdc_stream #(
		.W(7)
	) operator_tx (
		.src_clk  (clk_cpu),
		.src_reset(reset_cpu),
		.src_data (char[7*OPERATOR+:7]),
		.src_valid(char_valid[OPERATOR]),
		.src_ready(char_ready[OPERATOR]),
		.dst_clk  (clk_sys),
		.dst_reset(reset),
		.dst_data (con_tx_data[6:0]),
		.dst_valid(con_tx_valid[0]),
		.dst_ready(con_tx_ready[0])
	);

	cdc_stream #(
		.W(7)
	) station_tx (
		.src_clk  (clk_cpu),
		.src_reset(reset_cpu),
		.src_data (char[7*STATION+:7]),
		.src_valid(char_valid[STATION]),
		.src_ready(char_ready[STATION]),
		.dst_clk  (clk_sys),
		.dst_reset(reset),
		.dst_data (con_tx_data[13:7]),
		.dst_valid(con_tx_valid[1]),
		.dst_ready(con_tx_ready[1])
	);

	cdc_stream #(
		.W(7)
	) operator_rx (
		.src_clk  (clk_sys),
		.src_reset(reset),
		.src_data (con_rx_data[6:0]),
		.src_valid(con_rx_valid[0]),
		.src_ready(con_rx_ready[0]),
		.dst_clk  (clk_cpu),
		.dst_reset(reset_cpu),
		.dst_data (key[7*OPERATOR+:7]),
		.dst_valid(key_valid[OPERATOR]),
		.dst_ready(key_ready[OPERATOR])
	);

	cdc_stream #(
		.W(7)
	) station_rx (
		.src_clk  (clk_sys),
		.src_reset(reset),
		.src_data (con_rx_data[13:7]),
		.src_valid(con_rx_valid[1]),
		.src_ready(con_rx_ready[1]),
		.dst_clk  (clk_cpu),
		.dst_reset(reset_cpu),
		.dst_data (key[7*STATION+:7]),
		.dst_valid(key_valid[STATION]),
		.dst_ready(key_ready[STATION])
	);

	// A key event from the HPS, in the video clock's domain: the event's bits
	// have been steady for two clocks when the change of bit 10 is seen.
	reg [ 2:0] key_tgl;
	reg [10:0] ps2_key_s = 0;
	always @(posedge clk_sys) begin
		key_tgl <= {key_tgl[1:0], ps2_key[10]};
		if (key_tgl[2] != key_tgl[1]) ps2_key_s <= {~ps2_key_s[10], ps2_key[9:0]};
	end

	// F1 shows the operator's console, F2 the station and F3 what the printer
	// prints; keys go to the station while it is shown and to the operator's
	// console otherwise
	reg [1:0] visible;
	reg       key_stb;
	always @(posedge clk_sys) begin
		key_stb <= ps2_key_s[10];
		if (reset) visible <= 2'd0;
		else if (key_stb != ps2_key_s[10] && ps2_key_s[9:8] == 2'b10) begin
			if (ps2_key_s[7:0] == 8'h05) visible <= 2'd0;
			if (ps2_key_s[7:0] == 8'h06) visible <= 2'd1;
			if (ps2_key_s[7:0] == 8'h04) visible <= 2'd2;
		end
	end

	// the printer's characters for its screen
	wire [6:0] shown_char, prt_data;
	wire shown_valid, shown_taken, prt_valid, prt_ready;

	print_screen print_screen (
		.clk  (clk_cpu),
		.reset(reset_cpu),

		.i_char (print_char),
		.i_valid(print_valid && spool_ready),
		.o_ready(shown_ready),

		.o_char (shown_char),
		.o_valid(shown_valid),
		.i_ready(shown_taken)
	);

	cdc_stream #(
		.W(7)
	) printer_tx (
		.src_clk  (clk_cpu),
		.src_reset(reset_cpu),
		.src_data (shown_char),
		.src_valid(shown_valid),
		.src_ready(shown_taken),
		.dst_clk  (clk_sys),
		.dst_reset(reset),
		.dst_data (prt_data),
		.dst_valid(prt_valid),
		.dst_ready(prt_ready)
	);

	wire no_file, started;
	cdc_bit sync_no_file (
		.clk(clk_sys),
		.d  (boot_missing),
		.q  (no_file)
	);
	cdc_bit sync_started (
		.clk(clk_sys),
		.d  (boot_done),
		.q  (started)
	);

	wire [7:0] kbd_data;
	wire kbd_valid, kbd_ready;

	// the software knows capital letters only
	term_keyboard #(
		.CAPS(1)
	) keyboard (
		.clk       (clk_sys),
		.reset     (reset),
		.ps2_key   (ps2_key_s),
		.dout      (kbd_data),
		.dout_valid(kbd_valid),
		.dout_ready(kbd_ready)
	);

	xmp_console #(
		.CLK_HZ(CLK_HZ),
		.BAUD  (115200)
	) console (
		.clk       (clk_sys),
		.reset     (reset),
		.uart_reset(uart_reset),

		.visible(visible == 2'd1),
		.no_file(no_file),
		.started(started),

		.tx_data (con_tx_data),
		.tx_valid(con_tx_valid),
		.tx_ready(con_tx_ready),
		.rx_data (con_rx_data),
		.rx_valid(con_rx_valid),
		.rx_ready(con_rx_ready),

		.term_data (term_data),
		.term_valid(term_valid),
		.term_ready(term_ready),

		.kbd_data (kbd_data),
		.kbd_valid(kbd_valid),
		.kbd_ready(kbd_ready),

		.uart_rxd  (UART_RXD),
		.uart_txd  (UART_TXD),
		.uart_break(uart_break)
	);

	///////////////////////   VIDEO   ////////////////////////////////

	wire font_8x8 = status[14];

	// 29.4 MHz pixels for the 8x16 font (31 kHz), 14.7 MHz for the 8x8 font (15 kHz)
	reg ce_pix;
	always @(posedge clk_sys) begin
		reg div;
		div    <= ~div;
		ce_pix <= ~font_8x8 | div;
	end

	wire HBlank, VBlank, HSync, VSync, video;

	xmp_terminal terminal (
		.clk  (clk_sys),
		.reset(reset),

		.ce_pix  (ce_pix),
		.font_8x8(font_8x8),
		.visible (visible),

		.rx_data ({prt_data, term_data}),
		.rx_valid({prt_valid, term_valid}),
		.rx_ready({prt_ready, term_ready}),

		.hsync (HSync),
		.vsync (VSync),
		.hblank(HBlank),
		.vblank(VBlank),
		.video (video)
	);

	assign CLK_VIDEO = clk_sys;
	assign CE_PIXEL  = ce_pix;

	assign VGA_DE = ~(HBlank | VBlank);
	assign VGA_HS = HSync;
	assign VGA_VS = VSync;

	reg [23:0] rgb;
	always @(*) begin
		case (status[4:3])
			2'd0: rgb = 24'hFFFFFF;  // white
			2'd1: rgb = 24'h33FF33;  // green
			2'd2: rgb = 24'hFFB000;  // amber
			2'd3: rgb = 24'h40FFFF;  // cyan
		endcase
	end

	assign VGA_R = video ? rgb[23:16] : 8'd0;
	assign VGA_G = video ? rgb[15:8] : 8'd0;
	assign VGA_B = video ? rgb[7:0] : 8'd0;

endmodule

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

module emu #(
	// words of memory that are zeroed when the core has been loaded: central
	// memory and Buffer Memory.  The simulation of the core clears fewer.
	parameter [24:0] CLEAR = 25'h0800000
) (
	`include "sys/emu_ports.vh"
);

	///////// Default values for ports not used in this core /////////

	assign ADC_BUS = 'Z;
	assign USER_OUT = '1;
	assign {UART_RTS, UART_DTR} = 0;
	assign {SD_SCK, SD_MOSI, SD_CS} = 'Z;
	assign {SDRAM_DQ, SDRAM_A, SDRAM_BA, SDRAM_CLK, SDRAM_CKE, SDRAM_DQML, SDRAM_DQMH, SDRAM_nWE, SDRAM_nCAS, SDRAM_nRAS, SDRAM_nCS} = 'Z;

	assign VGA_F1         = 0;
	assign VGA_DISABLE    = 0;
	assign HDMI_FREEZE    = 0;
	assign HDMI_BLACKOUT  = 0;
	assign HDMI_BOB_DEINT = 0;

	// the only sound is the bell of the terminal that is shown (further down)
	assign AUDIO_S   = 0;
	assign AUDIO_MIX = 0;

	assign LED_POWER = 0;
	assign BUTTONS   = 0;

	//////////////////////////////////////////////////////////////////

	// The boot file (tools/py/mkboot.py) holds the kernel of the I/O Subsystem and
	// its boot tape.  The load address makes the MiSTer write it straight into
	// DDR3, above central memory and Buffer Memory.  The disks are images that
	// stay on the SD card: slot 0 the disk on the Peripheral Expander, slot 1 the
	// nine DD-29 drives on the BIOP's channels 20 to 32 (octal), one after
	// another in one file.  Slot 2 is a text file that takes what the printer
	// on the Peripheral Expander prints.  Slot 3 is a tape for the drive on the
	// Peripheral Expander, a .tap file, in place of the boot tape.
	`include "build_id.v"
	localparam CONF_STR = {
		"Cray-XMP;;",
		"-;",
		"F1,IOS,Load boot file,34000000;",
		"S0,IMG,Expander disk;",
		"S1,IMG,Disk drives;",
		"S2,TXT,Printer file;",
		"S3,TAP,Tape;",
		"-;",
		"O[122:121],Aspect ratio,Original,Full Screen,[ARC1],[ARC2];",
		"O[6:5],Scale,Normal,V-Integer,Narrower HV-Integer,Wider HV-Integer;",
		"O[9:7],Scandoubler Fx,None,HQ2x,CRT 25%,CRT 50%,CRT 75%;",
		"O[4:3],Text color,White,Green,Amber,Cyan;",
		"O[14],Font,8x8 (15kHz),8x16 (31kHz);",
		"O[15],Device times,Fast,Real;",
		"-;",
		"T[0],Reset;",
		"R[0],Reset and close OSD;",
		"v,1;",  // [optional] config version 0-99.
				 // If CONF_STR options are changed in incompatible way, then change version number too,
				 // so all options will get default values on first start.
		"V,v",
		`BUILD_DATE
	};

	///////////////////////   CLOCKS   ///////////////////////////////

	// clk_sys is the video clock and runs the screens, the keyboard and the
	// serial port: 58.8 MHz, two clocks to a pixel of the 8x16 font and four to
	// one of the 8x8 font.  clk_cpu is the CPU's, with the port to DDR3: 105.263 MHz,
	// the X-MP's 9.5 ns.  clk_ios is the I/O Subsystem's, with the HPS interface,
	// which brings the disks' sectors: 80 MHz, the 12.5 ns of its own oscillator.
	// Each has a PLL of its own, and no two of the three are in step.  What passes between
	// the CPU and the I/O Subsystem goes through rtl/xmp_bridge.v inside the
	// machine; what passes between either and the video clock goes through
	// rtl/mister/cdc.v.
	localparam CLK_HZ = 58800000;
	localparam IOS_HZ = 80000000;

	wire clk_sys, clk_cpu, clk_ios;
	// The 50 MHz pin reaches three PLLs by itself, and the framework has one of
	// them.  The CPU's clock and the I/O Subsystem's take the other two; the
	// video clock, which has time to spare, gets its reference over a global
	// clock line and can sit anywhere.
	wire clk_50m_global;
	cyclonev_clkena #(
		.clock_type       ("Global Clock"),
		.ena_register_mode("always enabled")
	) ref_video (
		.inclk (CLK_50M),
		.ena   (1'b1),
		.enaout(),
		.outclk(clk_50m_global)
	);
	pll pll (
		.refclk  (clk_50m_global),
		.rst     (1'b0),
		.outclk_0(clk_sys),
		.locked  ()
	);
	pll_cpu pll_cpu (
		.refclk  (CLK_50M),
		.rst     (1'b0),
		.outclk_0(clk_cpu),
		.locked  ()
	);
	pll_ios pll_ios (
		.refclk  (CLK_50M),
		.rst     (1'b0),
		.outclk_0(clk_ios),
		.locked  ()
	);

	///////////////////////   HPS   //////////////////////////////////

	wire         forced_scandoubler;
	wire [ 21:0] gamma_bus;
	wire [  1:0] buttons;
	wire [127:0] status;
	wire [ 10:0] ps2_key;
	wire         ioctl_download;

	// the disks: 512-byte blocks, one for a sector of the expander disk and
	// eight for a sector of a DD-29
	wire [31:0] sd_lba    [4];
	wire [ 5:0] sd_blk_cnt[4];
	wire [3:0] sd_rd, sd_wr, sd_ack;
	wire [13:0] sd_buff_addr;
	wire [ 7:0] sd_buff_dout;
	wire [ 7:0] sd_buff_din  [4];
	wire        sd_buff_wr;
	wire [ 3:0] img_mounted;
	wire [63:0] img_size;
	wire        img_readonly;

	hps_io #(
		.CONF_STR(CONF_STR),
		.VDNUM   (4),
		.BLKSZ   (2)
	) hps_io (
		.clk_sys  (clk_ios),
		.HPS_BUS  (HPS_BUS),
		.EXT_BUS  (),
		.gamma_bus(gamma_bus),

		.forced_scandoubler(forced_scandoubler),

		.buttons        (buttons),
		.status         (status),
		.status_menumask(16'd0),

		.ioctl_download(ioctl_download),

		.img_mounted (img_mounted),
		.img_size    (img_size),
		.img_readonly(img_readonly),
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

	// what the menu asks is in the HPS interface's clock, the I/O Subsystem's
	reg  host_reset_ios;
	wire host_reset;
	always @(posedge clk_ios) host_reset_ios <= RESET | status[0] | buttons[1] | ioctl_download;
	cdc_bit sync_host_reset (
		.clk(clk_cpu),
		.d  (host_reset_ios),
		.q  (host_reset)
	);

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

	// the same for the I/O Subsystem's side, and in the video clock's domain
	wire reset_ios;
	cdc_bit sync_reset_ios (
		.clk(clk_ios),
		.d  (reset_cpu),
		.q  (reset_ios)
	);

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

	wire mem_req, mem_we, mem_take, mem_ack;
	wire [ 6:0] mem_len;
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
	wire [ 3:0] other_take;  // only the CPU streams; the other four wait for the acknowledge
	wire        cpu_held;  // CPU Master Clear from the I/O Subsystem

	// The CPU streams its requests and can be reset by the I/O Subsystem while
	// the port runs on; the other four hold each request until its acknowledge.
	ddr3_ports #(
		.N     (5),
		.STREAM(5'b00001)
	) ddr3 (
		.clk  (clk_cpu),
		.reset(reset_cpu),

		.req({boot_req, tape_req, bm_req, cm_req, mem_req}),
		.we({boot_we, 1'b0, bm_we, cm_we, mem_we}),
		.len({7'd1, 7'd1, 7'd1, 7'd1, mem_len}),
		.addr({boot_addr, tape_base + {4'd0, tape_addr}, BUFFER, bm_addr[21:0], CENTRAL, cm_addr, CENTRAL, mem_addr}),
		.wdata({boot_wdata, 64'd0, bm_wdata, cm_wdata, mem_wdata}),
		.user_rst({4'b0000, cpu_held}),
		.take({other_take, mem_take}),
		.ack({boot_ack, tape_ack, bm_ack, cm_ack, mem_ack}),
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
		.BUFFER({BUFFER, 22'd0}),
		.CLEAR (CLEAR)
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

	wire [31:0] exp_lba, drive_lba, reel_lba;
	wire [7:0] exp_din, drive_din, reel_din;
	wire reel_rd, reel_wr;
	wire [8:0] drive_rd, drive_wr;
	wire exp_rd, exp_wr;

	wire [7:0] print_char;
	wire print_valid, print_ready;

	xmp_machine #(
		.CLOCKS_PER_MS(IOS_HZ / 1000)
	) machine (
		.clk    (clk_cpu),
		.clk_ios(clk_ios),
		.rst    (reset_cpu | ~boot_done),
		.i_real (status[15]),

		.o_mem_req  (mem_req),
		.o_mem_we   (mem_we),
		.o_mem_len  (mem_len),
		.o_mem_addr (mem_addr),
		.o_mem_wdata(mem_wdata),
		.i_mem_take (mem_take),
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

		.i_reel_mounted  (img_mounted[3]),
		.i_reel_blocks   (img_size[63:9]),
		.i_reel_readonly (img_readonly),
		.o_reel_lba      (reel_lba),
		.o_reel_rd       (reel_rd),
		.o_reel_wr       (reel_wr),
		.i_reel_ack      (sd_ack[3]),
		.i_reel_buff_addr(sd_buff_addr[8:0]),
		.i_reel_buff_dout(sd_buff_dout),
		.o_reel_buff_din (reel_din),
		.i_reel_buff_wr  (sd_buff_wr),

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
		.clk(clk_ios),

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

	assign sd_rd          = {reel_rd, spool_rd, |drive_rd, exp_rd};
	assign sd_wr          = {reel_wr, spool_wr, |drive_wr, exp_wr};
	assign sd_lba[0]      = exp_lba;
	assign sd_lba[1]      = drive_base + drive_lba;
	assign sd_lba[2]      = spool_lba;
	assign sd_lba[3]      = reel_lba;
	assign sd_blk_cnt[0]  = 6'd0;
	assign sd_blk_cnt[1]  = 6'd7;
	assign sd_blk_cnt[2]  = 6'd0;
	assign sd_blk_cnt[3]  = 6'd0;
	assign sd_buff_din[0] = exp_din;
	assign sd_buff_din[1] = drive_din;
	assign sd_buff_din[2] = spool_din;
	assign sd_buff_din[3] = reel_din;

	// stretch activity so it is visible
	reg [19:0] act_cnt, disk_cnt;
	always @(posedge clk_cpu) begin
		if (mem_req && !cpu_held) act_cnt <= '1;
		else if (act_cnt != 0) act_cnt <= act_cnt - 1'd1;
	end
	always @(posedge clk_ios) begin
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
		.src_clk  (clk_ios),
		.src_reset(reset_ios),
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
		.src_clk  (clk_ios),
		.src_reset(reset_ios),
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
		.dst_clk  (clk_ios),
		.dst_reset(reset_ios),
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
		.dst_clk  (clk_ios),
		.dst_reset(reset_ios),
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
		.clk  (clk_ios),
		.reset(reset_ios),

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
		.src_clk  (clk_ios),
		.src_reset(reset_ios),
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
	// what the two consoles' terminals send: their keys and answers
	wire [13:0] sent_data;
	wire [1:0] sent_valid, sent_ready;

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

		.key_data (sent_data),
		.key_valid(sent_valid),
		.key_ready(sent_ready),

		.uart_rxd  (UART_RXD),
		.uart_txd  (UART_TXD),
		.uart_break(uart_break)
	);

	///////////////////////   VIDEO   ////////////////////////////////

	// The 8x8 font is the one a core starts with: its 15 kHz raster is the one
	// every screen shows, as it is or doubled, and the menu that chooses the
	// other font is drawn into the same picture.
	wire font_8x8 = ~status[14];

	// 29.4 MHz pixels for the 8x16 font (31 kHz lines), 14.7 MHz for the 8x8
	// font (15 kHz lines).  The video clock is four times the slower of the
	// two, which is what the framework's scandoubler needs.
	reg ce_pix;
	always @(posedge clk_sys) begin
		reg [1:0] div;
		div    <= div + 2'd1;
		ce_pix <= font_8x8 ? (div == 2'd0) : ~div[0];
	end

	wire HBlank, VBlank, HSync, VSync, video, half, bell;

	// the bell: a tone of 1 kHz while the terminal sounds it
	localparam [14:0] BELL_HALF = 15'(CLK_HZ / 2000 - 1);
	reg        bell_tone;
	reg [14:0] bell_div;
	always @(posedge clk_sys)
		if (!bell) begin
			bell_tone <= 1'b0;
			bell_div  <= 15'd0;
		end else if (bell_div == BELL_HALF) begin
			bell_tone <= ~bell_tone;
			bell_div  <= 15'd0;
		end else bell_div <= bell_div + 15'd1;
	assign AUDIO_L = {3'd0, bell_tone, 12'd0};
	assign AUDIO_R = {3'd0, bell_tone, 12'd0};

	xmp_terminal terminal (
		.clk  (clk_sys),
		.reset(reset),

		.ce_pix  (ce_pix),
		.font_8x8(font_8x8),
		.visible (visible),

		.rx_data ({prt_data, term_data}),
		.rx_valid({prt_valid, term_valid}),
		.rx_ready({prt_ready, term_ready}),

		.key_data (kbd_data),
		.key_valid(kbd_valid),
		.key_ready(kbd_ready),

		.tx_data (sent_data),
		.tx_valid(sent_valid),
		.tx_ready(sent_ready),

		.hsync (HSync),
		.vsync (VSync),
		.hblank(HBlank),
		.vblank(VBlank),
		.video (video),
		.half  (half),
		.bell  (bell)
	);

	reg [23:0] rgb;
	always @(*) begin
		case (status[4:3])
			2'd0: rgb = 24'hFFFFFF;  // white
			2'd1: rgb = 24'h33FF33;  // green
			2'd2: rgb = 24'hFFB000;  // amber
			2'd3: rgb = 24'h40FFFF;  // cyan
		endcase
	end

	// a character at half intensity, which is how the terminal shows a protected one
	wire [23:0] lit = !video ? 24'd0 : half ? {1'b0, rgb[23:17], 1'b0, rgb[15:9], 1'b0, rgb[7:1]} : rgb;

	// The picture goes out through the framework's video_mixer, which has the
	// scandoubler with its effects and the gamma table, and video_freak, which
	// sets the aspect ratio and the integer scales.
	//
	// Only the 15 kHz raster of the 8x8 font can be doubled.  The 31 kHz raster
	// of the 8x16 font goes out as it is: 524 lines of which 400 are shown, at
	// the line and frame rates of VGA's 640x480.  The analog output carries the
	// core's own raster either way; vga_scaler=1 in MiSTer.ini puts the
	// scaler's picture there for a screen that needs it.
	wire [2:0] fx = status[9:7];
	wire [1:0] lines = (fx > 3'd1) ? fx[1:0] - 2'd1 : 2'd0;  // CRT 25%, 50%, 75%
	wire [1:0] ar = status[122:121];

	reg doubled;
	always @(posedge clk_sys) doubled <= font_8x8 && (forced_scandoubler || (fx != 3'd0));

	assign CLK_VIDEO  = clk_sys;
	assign VGA_SL     = lines;
	assign VGA_SCALER = 0;

	wire mixer_de;

	video_freak video_freak (
		.CLK_VIDEO  (CLK_VIDEO),
		.CE_PIXEL   (CE_PIXEL),
		.VGA_VS     (VGA_VS),
		.HDMI_WIDTH (HDMI_WIDTH),
		.HDMI_HEIGHT(HDMI_HEIGHT),
		.VGA_DE     (VGA_DE),
		.VIDEO_ARX  (VIDEO_ARX),
		.VIDEO_ARY  (VIDEO_ARY),
		.VGA_DE_IN  (mixer_de),
		.ARX        ((ar == 2'd0) ? 12'd4 : {10'd0, ar - 2'd1}),
		.ARY        ((ar == 2'd0) ? 12'd3 : 12'd0),
		.CROP_SIZE  (12'd0),
		.CROP_OFF   (5'd0),
		.SCALE      ({1'b0, status[6:5]})
	);

	// the terminal's sync pulses are low; the framework wants them high
	video_mixer #(
		.LINE_LENGTH(640),
		.GAMMA      (1)
	) video_mixer (
		.CLK_VIDEO  (CLK_VIDEO),
		.CE_PIXEL   (CE_PIXEL),
		.ce_pix     (ce_pix),
		.scandoubler(doubled),
		.hq2x       (fx == 3'd1),
		.gamma_bus  (gamma_bus),
		.R          (lit[23:16]),
		.G          (lit[15:8]),
		.B          (lit[7:0]),
		.HSync      (~HSync),
		.VSync      (~VSync),
		.HBlank     (HBlank),
		.VBlank     (VBlank),
		.HDMI_FREEZE(HDMI_FREEZE),
		.freeze_sync(),
		.VGA_R      (VGA_R),
		.VGA_G      (VGA_G),
		.VGA_B      (VGA_B),
		.VGA_VS     (VGA_VS),
		.VGA_HS     (VGA_HS),
		.VGA_DE     (mixer_de)
	);

endmodule

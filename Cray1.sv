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

	// Memory images are raw big-endian 64-bit Cray words.  The load address makes
	// the MiSTer write the file straight into DDR3, where main memory lives.
	`include "build_id.v"
	localparam CONF_STR = {
		"Cray1;;",
		"-;",
		"F1,CRYIMG,Load memory image,30000000;",
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

	wire         direct_video;
	wire [  1:0] buttons;
	wire [127:0] status;
	wire [ 10:0] ps2_key;
	wire         ioctl_download;

	hps_io #(
		.CONF_STR(CONF_STR)
	) hps_io (
		.clk_sys  (clk_sys),
		.HPS_BUS  (HPS_BUS),
		.EXT_BUS  (),
		.gamma_bus(),

		.direct_video(direct_video),

		.buttons        (buttons),
		.status         (status),
		.status_menumask(16'd0),

		.ioctl_download(ioctl_download),

		.ps2_key(ps2_key)
	);

	///////////////////////   CLOCKS   ///////////////////////////////

	// clk_sys is the video clock and runs the terminal, the console queues, the
	// serial port and the HPS interface.  The machine and its memory have their
	// own, faster clock; everything between the two goes through rtl/mister/cdc.v.
	localparam CLK_HZ = 29400000;

	wire clk_sys, clk_cpu;
	pll pll (
		.refclk  (CLK_50M),
		.rst     (1'b0),
		.outclk_0(clk_sys),
		.outclk_1(clk_cpu),
		.locked  ()
	);

	// Everything except the video raster is held in reset while the menu asks for
	// it, while the menu is writing an image into memory, and for as long as a
	// serial BREAK lasts.  The BREAK is the test scripts' master clear: hold it,
	// write memory from Linux, release it to dead start.  The serial receiver has to
	// keep running through that reset, so it has its own.
	wire        uart_break;
	reg         reset;
	reg  [15:0] reset_cnt;
	wire        host_reset = RESET | status[0] | buttons[1] | ioctl_download;

	// What a dead start runs.  A reset from the menu or the board copies the monitor
	// program into memory first.  After a memory image has been loaded from the menu,
	// or after a serial BREAK (the test hook), memory is left as it is.
	reg rom_boot = 1;
	always @(posedge clk_sys) begin
		if (RESET | status[0] | buttons[1]) rom_boot <= 1;
		else if (ioctl_download | uart_break) rom_boot <= 0;
	end
	always @(posedge clk_sys) begin
		if (host_reset | uart_break) begin
			reset     <= 1;
			reset_cnt <= '1;
		end else if (reset_cnt != 0) reset_cnt <= reset_cnt - 1'd1;
		else reset <= 0;
	end

	// the same in the machine's clock domain
	wire reset_cpu, rom_boot_cpu;
	cdc_bit sync_reset (
		.clk(clk_cpu),
		.d  (reset),
		.q  (reset_cpu)
	);
	cdc_bit sync_rom_boot (
		.clk(clk_cpu),
		.d  (rom_boot),
		.q  (rom_boot_cpu)
	);

	///////////////////////   MEMORY   ///////////////////////////////

	wire mem_req, mem_we, mem_burst, mem_ack;
	wire [21:0] mem_addr;
	wire [63:0] mem_wdata, mem_rdata;

	ddr3_mem ddr3_mem (
		.clk  (clk_cpu),
		.reset(reset_cpu),

		.req  (mem_req),
		.we   (mem_we),
		.burst(mem_burst),
		.addr (mem_addr),
		.wdata(mem_wdata),
		.ack  (mem_ack),
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

	///////////////////////   CONSOLE   //////////////////////////////

	wire [7:0] con_tx_data, con_rx_data, term_data, kbd_data;
	wire con_tx_valid, con_tx_ready, con_rx_valid, con_rx_pop;
	wire [7:0] con_in_data;
	wire       con_in_stb;
	wire term_valid, term_ready, kbd_valid, kbd_ready;

	console_io #(
		.CLK_HZ(CLK_HZ),
		.BAUD  (115200)
	) console_io (
		.clk       (clk_sys),
		.reset     (reset),
		.uart_reset(host_reset),

		.tx_data (con_tx_data),
		.tx_valid(con_tx_valid),
		.tx_ready(con_tx_ready),
		.rx_data (con_rx_data),
		.rx_valid(con_rx_valid),
		.rx_pop  (con_rx_pop),
		.in_data (con_in_data),
		.in_stb  (con_in_stb),

		.term_data (term_data),
		.term_valid(term_valid),
		.term_ready(term_ready),
		.kbd_data  (kbd_data),
		.kbd_valid (kbd_valid),
		.kbd_ready (kbd_ready),

		.uart_rxd  (UART_RXD),
		.uart_txd  (UART_TXD),
		.uart_break(uart_break)
	);

	///////////////////////   MACHINE   //////////////////////////////

	// The console as the machine sees it, in the machine's clock domain.
	wire [7:0] m_tx_data, m_rx_data, rxc_data;
	wire m_tx_valid, m_tx_ready, m_rx_valid, m_rx_pop, m_rx_full;
	wire rxc_valid, rx_src_ready;

	cdc_stream tx_cdc (
		.src_clk  (clk_cpu),
		.src_reset(reset_cpu),
		.src_data (m_tx_data),
		.src_valid(m_tx_valid),
		.src_ready(m_tx_ready),
		.dst_clk  (clk_sys),
		.dst_reset(reset),
		.dst_data (con_tx_data),
		.dst_valid(con_tx_valid),
		.dst_ready(con_tx_ready)
	);

	cdc_stream rx_cdc (
		.src_clk  (clk_sys),
		.src_reset(reset),
		.src_data (con_rx_data),
		.src_valid(con_rx_valid),
		.src_ready(rx_src_ready),
		.dst_clk  (clk_cpu),
		.dst_reset(reset_cpu),
		.dst_data (rxc_data),
		.dst_valid(rxc_valid),
		.dst_ready(~m_rx_full)
	);
	assign con_rx_pop = con_rx_valid && rx_src_ready;

	con_fifo #(
		.AW(4)
	) m_rx_fifo (
		.clk  (clk_cpu),
		.reset(reset_cpu),
		.din  (rxc_data),
		.wr   (rxc_valid),
		.full (m_rx_full),
		.dout (m_rx_data),
		.valid(m_rx_valid),
		.rd   (m_rx_pop)
	);

	// A CTRL-C entering the input queue, as one pulse in the machine's domain.
	wire ctrlc = con_in_stb && (con_in_data == 8'h03);
	reg ctrlc_d, ctrlc_tgl = 1'b0;
	always @(posedge clk_sys) begin
		ctrlc_d <= ctrlc;
		if (ctrlc && !ctrlc_d) ctrlc_tgl <= ~ctrlc_tgl;
	end
	wire ctrlc_s;
	cdc_bit sync_ctrlc (
		.clk(clk_cpu),
		.d  (ctrlc_tgl),
		.q  (ctrlc_s)
	);
	reg ctrlc_s_d;
	always @(posedge clk_cpu) ctrlc_s_d <= ctrlc_s;
	wire m_ctrl_c = ctrlc_s ^ ctrlc_s_d;

	wire mem_active, test_done;

`ifdef SHELL_TEST
	// Hardware self-test in place of the CPU: console, keyboard and DDR3 checks.
	wire running, failed;

	shell_test machine (
		.clk  (clk_cpu),
		.reset(reset_cpu),

		.mem_req  (mem_req),
		.mem_we   (mem_we),
		.mem_burst(mem_burst),
		.mem_addr (mem_addr),
		.mem_wdata(mem_wdata),
		.mem_ack  (mem_ack),
		.mem_rdata(mem_rdata),

		.tx_data (m_tx_data),
		.tx_valid(m_tx_valid),
		.tx_ready(m_tx_ready),
		.rx_data (m_rx_data),
		.rx_valid(m_rx_valid),
		.rx_pop  (m_rx_pop),

		.running(running),
		.failed (failed)
	);

	assign mem_active = running;
	assign test_done  = failed;
`else
	cray_system #(
		.XMP(0)
	) machine (
		.clk     (clk_cpu),
		.reset   (reset_cpu),
		.rom_boot(rom_boot_cpu),

		.mem_req  (mem_req),
		.mem_we   (mem_we),
		.mem_burst(mem_burst),
		.mem_addr (mem_addr),
		.mem_wdata(mem_wdata),
		.mem_ack  (mem_ack),
		.mem_rdata(mem_rdata),

		.tx_data (m_tx_data),
		.tx_valid(m_tx_valid),
		.tx_ready(m_tx_ready),
		.rx_data (m_rx_data),
		.rx_valid(m_rx_valid),
		.rx_pop  (m_rx_pop),
		.ctrl_c  (m_ctrl_c),

		.mem_active(mem_active),
		.test_done (test_done),
		.test_code ()
	);
`endif

	// stretch memory activity so it is visible
	reg [19:0] act_cnt;
	always @(posedge clk_cpu) begin
		if (mem_active) act_cnt <= '1;
		else if (act_cnt != 0) act_cnt <= act_cnt - 1'd1;
	end

	assign LED_USER = (act_cnt != 0);
	assign LED_DISK = {1'b1, test_done};

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

	cray_terminal terminal (
		.clk  (clk_sys),
		.reset(reset),

		.ce_pix  (ce_pix),
		.font_8x8(font_8x8),

		.rx_data (term_data),
		.rx_valid(term_valid),
		.rx_ready(term_ready),

		.ps2_key  (ps2_key),
		.kbd_data (kbd_data),
		.kbd_valid(kbd_valid),
		.kbd_ready(kbd_ready),

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

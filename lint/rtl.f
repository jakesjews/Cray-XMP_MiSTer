// Run from the repository root after make lint-prepare.
// Match Quartus VERILOG_FILE parsing for .v sources.
+1364-2005ext+v
// Supply a default for modules without a timescale; preserve explicit ones.
--timescale 1ns/1ps

// Read the warning policy and dependency exclusions before sources.
lint/exclusions.vlt
+incdir+lint/gen
+incdir+sys
+incdir+rtl/cray/cray-1x
-y sys

// Lint-only PLL boundary.
lint/pll.sv

// Verilog/SystemVerilog sources from files.qip; keep this list in sync.
rtl/xmp_machine.v
rtl/ios/ios.v
rtl/ios/ios_core.v
rtl/ios/iop.v
rtl/ios/iop_cpu.v
rtl/ios/ios_console.v
rtl/ios/ios_expander.v
rtl/ios/ios_hsp.v
rtl/ios/ios_disks.v
rtl/ios/ios_link.v
rtl/mister/ddr3_ports.sv
rtl/mister/xmp_boot.v
rtl/mister/xmp_console.v
rtl/mister/print_spool.v
rtl/mister/print_screen.v
rtl/terminal/xmp_terminal.v
rtl/terminal/term_ampex.v
rtl/mister/con_fifo.v
rtl/mister/cdc.v
rtl/mister/uart.v
rtl/terminal/term_keyboard.v
rtl/terminal/term_video.v
rtl/cray/cray_cpu.v
rtl/cray/cray_mem_mux.v
rtl/cray/mem_fu.v
rtl/cray/vector_add.v
rtl/cray/vector_logical.v
rtl/cray/vector_pop.v
rtl/cray/vector_shift.v
rtl/cray/cray_opnd.v
rtl/cray/cray_predecode.v
rtl/cray/exchange_ctl.v
rtl/cray/fp_add.v
rtl/cray/fp_mul.v
rtl/cray/fp_recip.v
rtl/cray/v_optrack.v
rtl/cray/v_regfile.v
rtl/cray/xmp_channels.v
rtl/cray/cray-1x/a_regfile.v
rtl/cray/cray-1x/a_res_lut.v
rtl/cray/cray-1x/a_scheduler.v
rtl/cray/cray-1x/addr_add.v
rtl/cray/cray-1x/b_regfile.v
rtl/cray/cray-1x/brancher.v
rtl/cray/cray-1x/fast_addr_mult.v
rtl/cray/cray-1x/func_top.v
rtl/cray/cray-1x/i_buf.v
rtl/cray/cray-1x/imm_gen.v
rtl/cray/cray-1x/lz_sub.v
rtl/cray/cray-1x/s_const_gen.v
rtl/cray/cray-1x/s_regfile.v
rtl/cray/cray-1x/s_res_lut.v
rtl/cray/cray-1x/s_scheduler.v
rtl/cray/cray-1x/scalar_add.v
rtl/cray/cray-1x/scalar_logical.v
rtl/cray/cray-1x/scalar_pop_lz.v
rtl/cray/cray-1x/scalar_shift.v
rtl/cray/cray-1x/t_regfile_hard.v
rtl/cray/cray-1x/v_scheduler.v
CrayXMP.sv

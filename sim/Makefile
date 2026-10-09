# Simulation targets.  Run from the repository root:  make -C sim <target>
ROOT  := $(abspath ..)
BUILD := $(ROOT)/sim/build

.PHONY: core ampex spool cpu fp iop ios xmp bridge disks times tape clean

# Core-level simulation of the CRAY X-MP core
core: $(BUILD)/core/Vemu
$(BUILD)/core/Vemu: $(ROOT)/sim/core.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/sim/stubs/*) \
                    $(ROOT)/Cray-XMP.sv $(ROOT)/files.qip $(wildcard $(ROOT)/rtl/*.v $(ROOT)/rtl/*/*.v $(ROOT)/rtl/*/*.sv $(ROOT)/rtl/cray/cray-1x/*.v)
	mkdir -p $(BUILD)/core && cd $(ROOT) && python3 tools/py/qip2vc.py > sim/build/core.files && verilator -f sim/core.vc --Mdir $(BUILD)/core -o Vemu

# The Ampex terminal against the model's
ampex: $(BUILD)/ampex/Vterm_ampex_tb
$(BUILD)/ampex/Vterm_ampex_tb: $(ROOT)/sim/ampex.vc $(ROOT)/sim/harness/ampex_main.cpp $(ROOT)/sim/tb/term_ampex_tb.v $(ROOT)/rtl/terminal/term_ampex.v
	mkdir -p $(BUILD)/ampex && cd $(ROOT) && verilator -f sim/ampex.vc --Mdir $(BUILD)/ampex -o Vterm_ampex_tb

# The printer's file against what it should hold
spool: $(BUILD)/spool/Vprint_spool
$(BUILD)/spool/Vprint_spool: $(ROOT)/sim/spool.vc $(ROOT)/sim/harness/spool_main.cpp $(ROOT)/rtl/mister/print_spool.v
	mkdir -p $(BUILD)/spool && cd $(ROOT) && verilator -f sim/spool.vc --Mdir $(BUILD)/spool -o Vprint_spool

# CPU-level simulation
cpu: $(BUILD)/cpu/Vcray_cpu
$(BUILD)/cpu/Vcray_cpu: $(ROOT)/sim/cpu.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/rtl/cray/*.v $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/cray/cray-1x/*.vh)
	mkdir -p $(BUILD)/cpu && cd $(ROOT) && verilator -f sim/cpu.vc --Mdir $(BUILD)/cpu -o Vcray_cpu

# Floating point unit bench
fp: $(BUILD)/fp/Vfp_tb
$(BUILD)/fp/Vfp_tb: $(ROOT)/sim/fp.vc $(ROOT)/sim/harness/fp_main.cpp $(ROOT)/sim/tb/fp_tb.v $(wildcard $(ROOT)/rtl/cray/fp_*.v)
	mkdir -p $(BUILD)/fp && cd $(ROOT) && verilator -f sim/fp.vc --Mdir $(BUILD)/fp -o Vfp_tb

# The I/O Processor against a record of the reference model's steps
iop: $(BUILD)/iop/Viop_cpu
$(BUILD)/iop/Viop_cpu: $(ROOT)/sim/iop.vc $(ROOT)/sim/harness/iop_main.cpp $(wildcard $(ROOT)/rtl/ios/*.v)
	mkdir -p $(BUILD)/iop && cd $(ROOT) && verilator -f sim/iop.vc --Mdir $(BUILD)/iop -o Viop_cpu

# The I/O Subsystem booting its kernel
ios: $(BUILD)/ios/Vios
$(BUILD)/ios/Vios: $(ROOT)/sim/ios.vc $(wildcard $(ROOT)/sim/harness/ios_*) $(wildcard $(ROOT)/rtl/ios/*.v)
	mkdir -p $(BUILD)/ios && cd $(ROOT) && verilator -f sim/ios.vc --Mdir $(BUILD)/ios -o Vios

# The whole X-MP machine: the CPU and the I/O Subsystem
xmp: $(BUILD)/xmp/Vxmp_machine
$(BUILD)/xmp/Vxmp_machine: $(ROOT)/sim/xmp.vc $(wildcard $(ROOT)/sim/harness/ios_*) $(ROOT)/rtl/xmp_machine.v $(ROOT)/rtl/xmp_bridge.v $(wildcard $(ROOT)/rtl/ios/*.v $(ROOT)/rtl/cray/*.v $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/cray/cray-1x/*.vh)
	mkdir -p $(BUILD)/xmp && cd $(ROOT) && verilator -f sim/xmp.vc --Mdir $(BUILD)/xmp -o Vxmp_machine

# The disk drives by themselves: their sectors and their times
disks: $(BUILD)/disks/Vios_disks
$(BUILD)/disks/Vios_disks: $(ROOT)/sim/disks.vc $(ROOT)/sim/harness/disks_main.cpp $(ROOT)/sim/harness/ios_media.h $(ROOT)/rtl/ios/ios_disks.v
	mkdir -p $(BUILD)/disks && cd $(ROOT) && verilator -f sim/disks.vc --Mdir $(BUILD)/disks -o Vios_disks

# The console, the Peripheral Expander and the channel pair to the mainframe: their times
times: $(BUILD)/times/Vios_times_tb
$(BUILD)/times/Vios_times_tb: $(ROOT)/sim/times.vc $(ROOT)/sim/harness/times_main.cpp $(ROOT)/sim/tb/ios_times_tb.v $(ROOT)/rtl/ios/ios_console.v $(ROOT)/rtl/ios/ios_expander.v $(ROOT)/rtl/ios/ios_link.v
	mkdir -p $(BUILD)/times && cd $(ROOT) && verilator -f sim/times.vc --Mdir $(BUILD)/times -o Vios_times_tb

# The tape drive of the Peripheral Expander and the reel on it against what a tape does
tape: $(BUILD)/tape/Vios_tape_tb
$(BUILD)/tape/Vios_tape_tb: $(ROOT)/sim/tape.vc $(ROOT)/sim/harness/tape_main.cpp $(ROOT)/sim/harness/ios_media.h $(ROOT)/sim/tb/ios_tape_tb.v $(ROOT)/rtl/ios/ios_expander.v $(ROOT)/rtl/ios/ios_reel.v
	mkdir -p $(BUILD)/tape && cd $(ROOT) && verilator -f sim/tape.vc --Mdir $(BUILD)/tape -o Vios_tape_tb

# The bridges between the CPU's clock and the I/O Subsystem's against what they promise
bridge: $(BUILD)/bridge/Vxmp_bridge_tb
$(BUILD)/bridge/Vxmp_bridge_tb: $(ROOT)/sim/bridge.vc $(ROOT)/sim/harness/bridge_main.cpp $(ROOT)/sim/tb/xmp_bridge_tb.v $(ROOT)/rtl/xmp_bridge.v
	mkdir -p $(BUILD)/bridge && cd $(ROOT) && verilator -f sim/bridge.vc --Mdir $(BUILD)/bridge -o Vxmp_bridge_tb

clean:
	rm -rf $(BUILD)

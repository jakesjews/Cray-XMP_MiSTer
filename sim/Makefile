# Simulation targets.  Run from the repository root:  make -C sim <target>
ROOT  := $(abspath ..)
BUILD := $(ROOT)/sim/build

.PHONY: emu core ampex cpu cpu-xmp fp iop ios xmp clean

# Core-level simulation of the MiSTer wrapper
emu: $(BUILD)/emu/Vemu
$(BUILD)/emu/Vemu: $(ROOT)/sim/emu.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/sim/stubs/*) \
                   $(ROOT)/Cray1.sv $(ROOT)/files.qip $(wildcard $(ROOT)/rtl/*.v $(ROOT)/rtl/*.sv $(ROOT)/rtl/*/*.v $(ROOT)/rtl/*/*.sv $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/boot/*.mem)
	mkdir -p $(BUILD)/emu && cd $(ROOT) && python3 tools/py/qip2vc.py > sim/build/emu.files && verilator -f sim/emu.vc --Mdir $(BUILD)/emu -o Vemu

# Core-level simulation of the CRAY X-MP core
core: $(BUILD)/core/Vemu
$(BUILD)/core/Vemu: $(ROOT)/sim/core.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/sim/stubs/*) \
                    $(ROOT)/CrayXMP.sv $(ROOT)/files_xmp.qip $(wildcard $(ROOT)/rtl/*.v $(ROOT)/rtl/*/*.v $(ROOT)/rtl/*/*.sv $(ROOT)/rtl/cray/cray-1x/*.v)
	mkdir -p $(BUILD)/core && cd $(ROOT) && python3 tools/py/qip2vc.py files_xmp.qip > sim/build/core.files && verilator -f sim/core.vc --Mdir $(BUILD)/core -o Vemu

# The Ampex terminal against the model's
ampex: $(BUILD)/ampex/Vterm_ampex_tb
$(BUILD)/ampex/Vterm_ampex_tb: $(ROOT)/sim/ampex.vc $(ROOT)/sim/harness/ampex_main.cpp $(ROOT)/sim/tb/term_ampex_tb.v $(ROOT)/rtl/terminal/term_ampex.v
	mkdir -p $(BUILD)/ampex && cd $(ROOT) && verilator -f sim/ampex.vc --Mdir $(BUILD)/ampex -o Vterm_ampex_tb

# CPU-level simulation
cpu: $(BUILD)/cpu/Vcray_cpu
$(BUILD)/cpu/Vcray_cpu: $(ROOT)/sim/cpu.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/rtl/cray/*.v $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/cray/cray-1x/*.vh)
	mkdir -p $(BUILD)/cpu && cd $(ROOT) && verilator -f sim/cpu.vc --Mdir $(BUILD)/cpu -o Vcray_cpu

# The same for the CPU with the X-MP features (XMP = 1, four million words)
cpu-xmp: $(BUILD)/cpu_xmp/Vcray_cpu
$(BUILD)/cpu_xmp/Vcray_cpu: $(ROOT)/sim/cpu_xmp.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/rtl/cray/*.v $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/cray/cray-1x/*.vh)
	mkdir -p $(BUILD)/cpu_xmp && cd $(ROOT) && verilator -f sim/cpu_xmp.vc --Mdir $(BUILD)/cpu_xmp -o Vcray_cpu

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
$(BUILD)/xmp/Vxmp_machine: $(ROOT)/sim/xmp.vc $(wildcard $(ROOT)/sim/harness/ios_*) $(ROOT)/rtl/xmp_machine.v $(wildcard $(ROOT)/rtl/ios/*.v $(ROOT)/rtl/cray/*.v $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/cray/cray-1x/*.vh)
	mkdir -p $(BUILD)/xmp && cd $(ROOT) && verilator -f sim/xmp.vc --Mdir $(BUILD)/xmp -o Vxmp_machine

clean:
	rm -rf $(BUILD)

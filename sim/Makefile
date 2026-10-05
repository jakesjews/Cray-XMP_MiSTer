# Simulation targets.  Run from the repository root:  make -C sim <target>
ROOT  := $(abspath ..)
BUILD := $(ROOT)/sim/build

.PHONY: emu cpu cpu-xmp fp iop ios clean

# Core-level simulation of the MiSTer wrapper
emu: $(BUILD)/emu/Vemu
$(BUILD)/emu/Vemu: $(ROOT)/sim/emu.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/sim/stubs/*) \
                   $(ROOT)/Cray1.sv $(ROOT)/files.qip $(wildcard $(ROOT)/rtl/*.v $(ROOT)/rtl/*.sv $(ROOT)/rtl/*/*.v $(ROOT)/rtl/*/*.sv $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/boot/*.mem)
	mkdir -p $(BUILD)/emu && cd $(ROOT) && python3 tools/py/qip2vc.py > sim/build/emu.files && verilator -f sim/emu.vc --Mdir $(BUILD)/emu -o Vemu

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
$(BUILD)/ios/Vios: $(ROOT)/sim/ios.vc $(ROOT)/sim/harness/ios_main.cpp $(wildcard $(ROOT)/rtl/ios/*.v)
	mkdir -p $(BUILD)/ios && cd $(ROOT) && verilator -f sim/ios.vc --Mdir $(BUILD)/ios -o Vios

clean:
	rm -rf $(BUILD)

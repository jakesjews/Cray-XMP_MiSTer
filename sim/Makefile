# Simulation targets.  Run from the repository root:  make -C sim <target>
ROOT  := $(abspath ..)
BUILD := $(ROOT)/sim/build

.PHONY: emu cpu fp clean

# Core-level simulation of the MiSTer wrapper
emu: $(BUILD)/emu/Vemu
$(BUILD)/emu/Vemu: $(ROOT)/sim/emu.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/sim/stubs/*) \
                   $(ROOT)/Cray1.sv $(ROOT)/files.qip $(wildcard $(ROOT)/rtl/*.v $(ROOT)/rtl/*.sv $(ROOT)/rtl/*/*.v $(ROOT)/rtl/*/*.sv $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/boot/*.mem)
	mkdir -p $(BUILD)/emu && cd $(ROOT) && python3 tools/py/qip2vc.py > sim/build/emu.files && verilator -f sim/emu.vc --Mdir $(BUILD)/emu -o Vemu

# CPU-level simulation
cpu: $(BUILD)/cpu/Vcray_cpu
$(BUILD)/cpu/Vcray_cpu: $(ROOT)/sim/cpu.vc $(wildcard $(ROOT)/sim/harness/*) $(wildcard $(ROOT)/rtl/cray/*.v $(ROOT)/rtl/cray/cray-1x/*.v $(ROOT)/rtl/cray/cray-1x/*.vh $(ROOT)/rtl/cray/cray-1x/xmp/*.v)
	mkdir -p $(BUILD)/cpu && cd $(ROOT) && verilator -f sim/cpu.vc --Mdir $(BUILD)/cpu -o Vcray_cpu

# Floating point unit bench
fp: $(BUILD)/fp/Vfp_tb
$(BUILD)/fp/Vfp_tb: $(ROOT)/sim/fp.vc $(ROOT)/sim/harness/fp_main.cpp $(ROOT)/sim/tb/fp_tb.v $(wildcard $(ROOT)/rtl/cray/fp_*.v)
	mkdir -p $(BUILD)/fp && cd $(ROOT) && verilator -f sim/fp.vc --Mdir $(BUILD)/fp -o Vfp_tb

clean:
	rm -rf $(BUILD)

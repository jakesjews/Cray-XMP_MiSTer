PYTHON ?= python3
VERILATOR ?= verilator

.PHONY: help format format-check lint lint-prepare tools sim rom test-quick test

help:
	@echo "make format        Format project RTL and testbenches in place"
	@echo "make format-check  Check formatting without changing files"
	@echo "make lint          Lint the emu top, the self-test build, the X-MP setting and the I/O Processors with Verilator"
	@echo "make lint-prepare  Prepare dependencies for a direct Verilator invocation"
	@echo "make tools         Build the assembler and the reference model (tools/target/release)"
	@echo "make sim           Build the CPU, floating-point, whole-core and I/O Processor simulations (sim/build)"
	@echo "make rom           Assemble the monitor into rtl/boot/monitor.mem"
	@echo "make test-quick    Smoke and directed tests, reference vectors, 200 random programs"
	@echo "make test          test-quick, then long floating-point and random program runs"
	@echo "Append FILES='rtl/cray_system.sv sim/tb/fp_tb.v' to select formatter files"

format format-check:
	$(PYTHON) tools/py/verible.py $@ $(FILES)

lint: lint-prepare
	$(VERILATOR) --lint-only -Wall --top-module emu -f lint/rtl.f
	$(VERILATOR) --lint-only -Wall --top-module emu +define+SHELL_TEST -f lint/rtl.f
	$(VERILATOR) --lint-only -Wall --top-module cray_cpu -GXMP=1 -f lint/rtl.f
	$(VERILATOR) --lint-only -Wall --top-module ios lint/exclusions.vlt rtl/ios/iop_cpu.v rtl/ios/iop.v rtl/ios/ios_core.v rtl/ios/ios_console.v rtl/ios/ios.v

lint-prepare:
	@mkdir -p lint/gen
	@printf '`define BUILD_DATE "lint"\n' > lint/gen/build_id.v

tools:
	cargo build --release --examples --bins --manifest-path tools/Cargo.toml

sim:
	$(MAKE) -C sim cpu cpu-xmp fp emu iop ios

rom: tools
	@mkdir -p build
	tools/target/release/cray1 asm software/monitor/monitor.cal -I software/monitor -o build/monitor.img -l build/monitor.lst
	$(PYTHON) tools/py/mkrom.py build/monitor.img rtl/boot/monitor.mem

test-quick: tools sim
	$(PYTHON) tools/py/runtests.py quick

test: tools sim
	$(PYTHON) tools/py/runtests.py full

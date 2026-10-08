PYTHON ?= python3
VERILATOR ?= verilator

.PHONY: help format format-check lint lint-prepare tools sim test-quick test

help:
	@echo "make format        Format project RTL and testbenches in place"
	@echo "make format-check  Check formatting without changing files"
	@echo "make lint          Lint the core and the CPU's CRAY-1 setting with Verilator"
	@echo "make lint-prepare  Prepare dependencies for a direct Verilator invocation"
	@echo "make tools         Build the assembler and the reference models (tools/target/release)"
	@echo "make sim           Build the simulations (sim/build)"
	@echo "make test-quick    The short tests of the CPU, the I/O Processors and the core"
	@echo "make test          test-quick, then the long runs; with the COS 1.17 software, COS on the model and on the hardware description"
	@echo "Append FILES='rtl/xmp_machine.v sim/tb/fp_tb.v' to select formatter files"

format format-check:
	$(PYTHON) tools/py/verible.py $@ $(FILES)

# The core is the CPU with the X-MP features; its CRAY-1 setting is what most
# of the CPU's tests run on, so it is linted too.
lint: lint-prepare
	$(VERILATOR) --lint-only -Wall --top-module emu -f lint/rtl.f
	$(VERILATOR) --lint-only -Wall --top-module cray_cpu -GXMP=0 -f lint/rtl.f

lint-prepare:
	@mkdir -p lint/gen
	@printf '`define BUILD_DATE "lint"\n' > lint/gen/build_id.v

tools:
	cargo build --release --examples --bins --manifest-path tools/Cargo.toml

sim:
	$(MAKE) -C sim cpu fp iop ios xmp core ampex spool bridge

test-quick: tools sim
	$(PYTHON) tools/py/runtests.py quick

test: tools sim
	$(PYTHON) tools/py/runtests.py full

# The machine

Technical notes on what this core implements, where it came from and where it
differs from a real CRAY-1. The reference throughout is the CRAY-1 Hardware
Reference Manual, publication 2240004 revision C.

## Where the CPU came from

The CPU started as the X-MP generation of Chris Fenton's CRAY-1 for FPGAs, from
the cray-1x project (googlecode trunk r257, `Verilog/xmp`). Those sources are in
`rtl/cray/cray-1x/`. Upstream is no longer maintained, so they are maintained
here, formatted and linted like the rest. The first commit of this core has
them with the repairs below but still in the upstream layout.

The upstream CPU did not run programs correctly as found. What this core keeps
from it, with repairs:

- instruction issue from the current instruction parcel
- the result schedulers for the A, S and V registers and their delay tables
- the scalar integer, logical, shift and population count units
- the A, S, B and T register files and the instruction buffers

What was written for this core, in `rtl/cray/`:

- `cray_cpu`, the top, and `cray_mem_mux`, the memory request multiplexer
- `exchange_ctl`, the exchange sequence
- `cray_opnd`, which registers an instruction reads, for holding issue
- `mem_fu`, the memory sequencer for scalar, block and vector transfers
- `v_regfile` and `v_optrack`, the vector registers and per-unit operation tracking
- `vector_logical`, `vector_add`, `vector_shift`
- `fp_add`, `fp_mul`, `fp_recip`, the floating-point units

Repairs to the upstream files:

- `imm_gen`: the complemented constants of 021 and 041 used a logical not.
- `s_scheduler`: 052 and 053 deliver to S0, not Si.
- `a_regfile`, `s_regfile`: the constants read for register number 0 take
  priority over a result on its way to A0 or S0.
- `scalar_shift`, `scalar_pop_lz`: the result is chosen by the instruction
  that started the operation, not by one that issued later.
- `brancher`: the wait for a branch's second parcel no longer reads parcels
  that are not valid.
- `s_res_lut`, `v_scheduler`: delivery time of 076, and the CRAY-1 meaning of 174.
- `i_buf`: 16-word burst fills, and no fill starts during an exchange.
- `func_top`: the exchange sequence and saved P, holding issue for operands
  another unit is still producing, second parcels no longer decoded as
  instructions, exact decoding of 0020 to 0022, the T register write of 075,
  monitor-mode gating of the clock and XA instructions, the floating-point
  mode flag, field protection and the range flags, a 22-bit P, the console
  interrupt, and a new vector section.
- Lint clean-up across the files: signals and registers nothing read were
  removed and operand widths made explicit. Each module was proven equivalent
  to its form before the clean-up with `tools/py/equiv.py`.
- `xmp/intercpu_comms`: the issue signal of the fourth CPU tested the third
  CPU's instruction type. Fixed; like the rest of the X-MP code it is untested.

A parameter `XMP` selects the machine. `XMP = 0` is the CRAY-1 and is the only
setting that has been verified. `XMP = 1` keeps the X-MP additions of the
upstream source compiling (channels, shared registers, semaphores); nothing
about it has been tested. The X-MP vector population count is not built.

## What is implemented

- All CRAY-1 instructions of the manual's Appendix D.
- A, S, B, T and V registers, VL, VM, the real-time clock, P, BA, LA, XA, M and F.
- The exchange sequence with the CRAY-1 exchange package layout, dead start,
  normal and error exits.
- Memory field protection: an address is valid when the relative address plus
  16 times BA is below 16 times LA and below 2^20. A store outside the field
  does not change memory.
- Interrupt flags: normal exit, error exit, program range, operand range,
  floating-point error and console interrupt. They set only outside monitor mode.
- Vector operations with the result register also an operand behave as the
  manual describes on pages 3-14 to 3-16.
- 1,048,576 words of memory, held in the MiSTer's DDR3.

## Differences from a real CRAY-1

- **Timing.** One clock period is one 29.4 MHz FPGA clock. Functional unit
  times in clock periods follow the upstream tables, but memory references
  take longer than on the real machine and vary. Programs get the same results
  as on a CRAY-1 but not in the same number of clock periods.
- **Chaining is looser than on the real machine.** An operation may start on
  a register that a functional unit is still filling as soon as the first
  element is in, and at any time after that, not only in the one chain slot
  clock. A register being filled by a vector load is never chained, because
  memory does not deliver at a steady rate. Results are the same.
- **Floating-point multiply rounding.** The manual's figure of the CRAY-1
  multiply pyramid does not fix every bit that is dropped. The multiply
  follows the arithmetic the X-MP manual spells out instead. Some products may
  differ in the last bit from a real CRAY-1; this has not been compared with one.
- **Reciprocal.** The reciprocal approximation follows the algorithm of the
  cray-sim project's model. Its results match that project's test values.
- **Interrupts are precise.** The exchange happens right after the instruction
  that raised the flag. The manual allows a few more parcels to issue.
- **No I/O channels.** 0010 to 0012 do nothing. 033 reads zero. The I/O
  interrupt flag never sets.
- **No memory errors.** The memory error flag and the error fields of the
  exchange package are always zero.
- **Real-time clock interrupt.** The flag exists in F but nothing sets it; the
  manual does not name a source.
- **A fetch outside the field in monitor mode** is not checked, since the
  program range flag cannot set there.

## Additions that are not CRAY-1

A real CRAY-1 has no console on the CPU; a maintenance control unit loads
memory and operators work through front-end computers. This core adds:

- **An I/O page** in the top 16 words of memory. Word addresses, octal:
  - `3777760` console status. Read: bit 0 an input character waits, bit 1
    output ready, bit 2 console interrupt enabled, bit 3 requested. Write:
    bit 0 enables the console interrupt and clears a request.
  - `3777761` console data. Read takes the next input character. Write prints
    the low 8 bits, waiting while the output queue is full.
  - `3777762` test exit, used by the test programs.
  - `3777763` a free-running clock counter.
- **The console interrupt** is requested when CTRL-C arrives from the keyboard
  or the serial port while it is enabled.
- **Dead start.** A reset copies the monitor from a ROM into memory from word
  0 and exchanges to the package at word 0. After a memory image has been
  loaded from the menu, the image is started instead.

## Memory

`rtl/mister/ddr3_mem.sv` maps Cray word n to the 8 bytes at HPS address
`0x30000000 + 8n`, most significant byte first, so a memory image file is
simply the words in order. Instruction buffers fill with 16-word bursts. A
vector load stepping by 1 to 7 words also reads whole lines in bursts when
three or more of its elements lie in a line, except in the I/O page, and
picks its elements out as they arrive. A block or vector store reads the next
word from its register while the one before is on its way to memory.

With these and chaining, the monitor's SAXPY demonstration runs its vector
loop about 9 times faster than its scalar loop on a DE10-Nano: 7,413 clock
periods against 67,742 for 1024 elements.

Measured on a DE10-Nano at 29.4 MHz: a store takes 2 clocks, a single read 8
clocks typically and 22 at worst, a 16-word burst 23 typically and 32 at worst.

The CPU's memory port is a request held until acknowledged, one acknowledge
pulse per word. Nothing in the CPU depends on how long memory takes, and the
tests run every program with several memory timings to hold it to that.

## How it was verified

- An instruction-level reference model was written from the manual, separately
  from the RTL (`tools/crates/model`). Tests compare end states.
- 42 smoke test runs and 5 directed tests agree with the model in five run modes.
- 10,000 random programs of up to 250 instructions agree with the model in five
  run modes each, with no failures.
- The floating-point units match the reference arithmetic on 200,000 random
  cases per operation, streamed and with gaps, and on 79 cases from cray-sim.
- On a real MiSTer, 547 programs were run and their memory compared with the
  model, with no failures. The monitor's demonstrations print the same output
  there as in simulation, and its memory test of every word above the monitor
  reports no errors.

The model and the RTL share one reading of the manual for anything no test
vector from a real machine covers.

## Resources

Quartus 17.0 for the DE10-Nano: about 19,000 ALMs (45%), 121 memory blocks,
36 DSP blocks. Timing is met; the 29.4 MHz clock path is good for about 50 MHz.

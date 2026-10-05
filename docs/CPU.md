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
- Floating-point range errors as on manual page 3-21. For the add unit that
  is an incoming exponent of 60000 octal or more; a carry that takes in-range
  operands to 60000 is delivered without the error.
- Vector operations with the result register also an operand behave as the
  manual describes on pages 3-14 to 3-16.
- 1,048,576 words of memory, held in the MiSTer's DDR3.

## Differences from a real CRAY-1

- **Timing.** One clock period is one cycle of the machine's own FPGA clock,
  81.67 MHz against the real machine's 80 MHz. Functional unit times in clock
  periods follow the upstream tables, but memory references take longer than
  on the real machine and vary. Programs get the same results as on a CRAY-1
  but not in the same number of clock periods.
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

## Compared with Cray-on-FPGA

Zorislav Shoyat's Cray-on-FPGA is another rework of the same cray-1x sources,
for a Xilinx board. Every behavioural change it makes was checked against this
core. It turned up no fault here at the CRAY-1 setting beyond one in the
simulator's single-step mode, where an exit with a result still in flight was
taken before its flag was set; that is fixed. Several of its changes differ
from the manual (exit flags set in monitor mode, a vector length of 64 for
`VL 1`, a result register that is also an operand read element by element),
and it still has upstream faults repaired here, so no code was taken from it.
One idea was: its memory instructions issue at once and transfer in the
background. For vector loads and stores that is what the real machine does
(manual page 4-70), and this core now does it too. Its other additions are not
CRAY-1 behaviour (eight instruction buffers, a two-clock address multiply) or
belong to the X-MP.

What it and the review showed about the X-MP paths, none of which the CRAY-1
build uses, and all of which wait for the X-MP work:

- 174ij1 and 174ij2, the vector population counts, start a unit that is not
  built. With `XMP = 1` the result register would stay reserved for good.
- The inter-CPU module registers each CPU's monitor mode and never checks it.
- Reading the interrupting channel with 033 clears every pending channel
  interrupt, not only the one reported.
- The floating-point status bit, bidirectional memory mode (0025, 0026) and
  002700 are carried through the exchange package or not decoded, and do nothing.
- The X-MP add unit also reports an out-of-range result; this one does not.

## Clocks

The machine (CPU, dead start, I/O page) and its memory port run on their own
PLL output. The terminal, the console queues, the serial port and the HPS
interface stay on the 29.4 MHz video clock. The two meet only in
`rtl/mister/cdc.v`: a two-flip-flop synchroniser for reset and the dead start
choice, a handshake that carries one console character at a time in each
direction, and a toggle for CTRL-C. `Cray1.sdc` tells the timing analyser the
two clocks are unrelated, and the whole-core simulation runs them at
unrelated rates (`--cpu-ratio`).

The machine clock is set in `rtl/pll/pll_0002.v` (`output_clock_frequency1`).
With the PLL's 735 MHz oscillator the exact choices are 735 divided by a whole
number: 49, 52.5, 56.5, 61.25, 66.8, 73.5 MHz.

What limits the clock is the path every result takes in one clock period: off
the result bus, through the register file's bypass, through operand selection
and into the first stage of a functional unit, and the instruction issue loop
beside it. Told the real target, the fitter closes those paths in 12.2 ns,
which is what allows 81.67 MHz. Only 0.03 ns is to spare there, so 73.5 MHz is
the setting to fall back to if a later change no longer fits. Two changes were
needed to get from 79.6 to 81.67 MHz: a memory transfer under way goes by
flags latched at its start instead of choosing between the live and the
latched instruction, and 077 writes its V register element in the clock after
it issues, which nothing can observe.

Clock periods are as fast as the real machine's, so work between registers
runs at its speed. Memory does not: a scalar load takes about 25 clock periods
here against 11 on a CRAY-1, and vector transfers move about one word every
two clock periods, not one per clock period.

## Memory

`rtl/mister/ddr3_mem.sv` maps Cray word n to the 8 bytes at HPS address
`0x30000000 + 8n`, most significant byte first, so a memory image file is
simply the words in order. Instruction buffers fill with 16-word bursts. A
vector load stepping by 1 to 7 words also reads whole lines in bursts when
three or more of its elements lie in a line, except in the I/O page, and
picks its elements out as they arrive. A block or vector store reads the next
word from its register while the one before is on its way to memory.

A vector load or store lets its instruction issue three clocks after it starts
and goes on in the background while other instructions issue; its V register
stays reserved and other memory instructions wait for it. A transfer whose
first or last address is outside the field stays the current instruction
instead, so the range error interrupt is taken right behind it. Scalar
references and block transfers hold issue until they are done.

With these and chaining, the monitor's SAXPY demonstration runs its vector
loop about 10 times faster than its scalar loop on a DE10-Nano: 7,417 clock
periods against 77,404 for 1024 elements at 81.67 MHz, which is 91
microseconds against 948. The 64 by 64 matrix product takes 10.2 ms.

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

Quartus 17.0 for the DE10-Nano: about 19,800 ALMs (47%), 121 memory blocks,
37 DSP blocks. Timing is met with the machine at 81.67 MHz and the video side
at 29.4 MHz.

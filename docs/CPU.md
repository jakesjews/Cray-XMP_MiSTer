# The machine

Technical notes on what this core implements, where it came from and where it
differs from a real CRAY-1. The machine is the CRAY-1 as sold in 1982, with its
two instruction set options. The reference is the CRAY-1 Hardware Reference
Manual: publication 2240004 revision C, whose page numbers are used below,
and revision F of May 1982 (HR-0004) where the two differ. Pages of revision F
are marked "rev F".

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
- `i_buf`: a buffer that is being filled again no longer answers for the
  block it held before. A jump taken after a fetch ahead could otherwise run
  parcels of the wrong block.
- Lint clean-up across the files: signals and registers nothing read were
  removed and operand widths made explicit. Each module was proven equivalent
  to its form before the clean-up with `tools/py/equiv.py`.

A parameter `XMP` selects the machine. `XMP = 0` is the CRAY-1 and is what the
MiSTer core is built with. `XMP = 1` adds what a one-processor CRAY X-MP has
that the operating system COS needs; see "The X-MP setting" below. The
upstream source's own X-MP code (channels and the registers shared by four
CPUs) did not work and has been removed.

## What is implemented

- All CRAY-1 instructions of the manual's Appendix D.
- The vector population instructions option (rev F pages 4-25 and 4-70):
  026ij1 `Ai QSj`, the parity of the one bits of (Sj); 174ij1 `Vi PVj`, the
  population counts of the elements of Vj; 174ij2 `Vi QVj`, their parities.
  The vector unit takes 6 clock periods and runs one operation at a time
  together with the reciprocal unit.
- The programmable clock option (rev F pages 4-10 and 6-23): 0014j4 `PCI Sj`
  enters the interrupt interval, 0014j5 `CCI` clears the interrupt request,
  0014j6 `ECI` enables it and 0014j7 `DCI` disables it, all in monitor mode
  only. The countdown runs all the time, one count per clock period; a
  request comes every interval + 1 clock periods.
- A, S, B, T and V registers, VL, VM, the real-time clock, P, BA, LA, XA, M and F.
- The exchange sequence with the CRAY-1 exchange package layout, dead start,
  normal and error exits.
- Memory field protection: an address is valid when the relative address plus
  16 times BA is below 16 times LA and below 2^20. A store outside the field
  does not change memory.
- Interrupt flags: normal exit, error exit, program range, operand range,
  floating-point error, the programmable clock interrupt (bit 31) and the MCU
  interrupt (bit 32). They set only outside monitor mode. A clock or MCU
  request made in monitor mode waits and is taken when a user program runs.
  The two bits are named as in revision F; revision C called them console
  interrupt and real-time clock interrupt.
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
- **Floating-point multiply.** The CRAY-1 had two multiply units. Machines
  up to about 1980 had a pyramid that was not commutative (revisions C and E
  of the manual). Change packet E-01 of May 1980 documents a symmetric unit,
  in the same words as the later CRAY-1 S and X-MP manuals. This core has the
  symmetric unit. About one product in five differs in its last bit from
  what the original unit would give; which machines had which is not known.
- **The multiply and the reciprocal match Cray's own simulation of them.**
  Cray's floating-point diagnostic contains a simulation of each unit (the
  listing found is the 1997 edition for the J90; the code goes back to 1980).
  The core's units give the same bits as a transcription of it on millions of
  operands, for 064 to 067 and 070. The complement step of 067 follows it
  too; with it the statistics W. Kahan published from real machines in 1990
  come out, which they do not with the rule the cray-sim project guessed.
  `research/notes/fp-multiply.md` has the evidence.
- **Half-precision products** keep 29 bits, as that simulation and the
  CRAY-1 S and X-MP manuals have it. The 1980 change packet says 30.
- **Interrupts are precise.** The exchange happens right after the instruction
  that raised the flag. The manual allows a few more parcels to issue.
- **No I/O channels on the CRAY-1 setting.** 0010 to 0012 do nothing. 033
  reads zero. The I/O interrupt flag never sets. The X-MP setting has them.
- **No memory errors.** The memory error flag and the error fields of the
  exchange package are always zero.
- **Encodings the 1982 manual leaves undefined.** 0014jk with k = 1, 2 or 3
  is a pass. 026ijk with k = 2 to 7 is the population count. 174ijk with
  k = 3 to 7 is the reciprocal. Dead start clears the programmable clock's
  enable and request; on the real machine they are undefined then.
- **A fetch outside the field in monitor mode** is not checked, since the
  program range flag cannot set there.
- **A store into an instruction** that is already in an instruction buffer
  does not change what runs, as on the real machine (manual page 3-33): the
  buffer keeps the old parcels until it is filled again or an exchange voids
  it. Which blocks are in the four buffers at a given moment follows this
  core's fetch sequence, not necessarily the real machine's. The reference
  model has no buffers and runs the new parcel at once, so the two can differ
  on a program that modifies code it is about to run.

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
  or the serial port while it is enabled. The console stands in for the
  maintenance control unit, so the request raises the MCU interrupt flag.
  It stays until the status word is written.
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

## The X-MP setting

The only operating system that survives for these machines, COS 1.17, is a
build for the X-MP. `XMP = 1` gives the CPU what that build was found to need
beyond a CRAY-1 (the study is in `research/cos-cray1/`, the specification in
`research/notes/machine-spec.md`). The reference is the CRAY X-MP Series
Model 14 mainframe reference manual, CSM-0111000. It is simulated and tested
against the reference model. No MiSTer build uses it yet: COS also needs the
I/O Subsystem with its I/O processors, which does not exist here.

What changes with `XMP = 1`:

- **Memory** has four million words. The I/O page is its top 16 words, word
  `17777760` octal on.
- **The exchange package** has the X-MP layout: a 24-bit P, an instruction
  base and limit and a data base and limit of 19 bits each in units of 32
  words, the mode bits of words 1 and 2, the deadlock flag, the program state
  bit and the cluster number. The processor number, the memory error fields
  and the VNU, ESVL and EAM bits are stored as zero.
- **Fields.** Instructions are fetched through the instruction pair, operands
  through the data pair. Only the low 22 bits of an operand address count. A
  load from outside the data field delivers zero and a store is dropped; the
  operand range flag also needs its mode bit, which 0023 sets and 0024 clears.
  A block or vector transfer goes on after such a reference.
- **Branches** take 24 bits and raise no flag themselves.
- **The cluster number** is set from the package or by 0014j3 in monitor mode.
  Clusters 1 to 3 each have eight SB registers (026ij7, 027ij7), eight ST
  registers (072ij3, 073ij3) and 32 semaphores (0034, 0036, 0037, 072i02,
  073i02). In cluster 0 stores do nothing and loads give zero.
- **Test and set** (0034) of a set semaphore does not issue. Outside monitor
  mode that is the deadlock interrupt at once, there being one CPU: the
  package stored has the flag, the waiting bit and P at the instruction. In
  monitor mode the instruction waits for good.
- **The status register** (073i01) as the manual has it: ones in the low
  half, the cluster number only in monitor mode.
- **Modes.** 0025 and 0026 switch the bidirectional memory bit, which is only
  carried. 0027 waits for memory references to finish. The floating-point
  error status bit sets on any floating-point error and is cleared by 0021
  and 0022.
- **No recursion.** A vector register used as operand and result of one
  instruction is read element by element before it is written.
- **Channels.** Four pairs of 6 Mbyte channels, 10 to 17 octal, the even
  ones input and the odd ones output (`rtl/cray/xmp_channels.v`). In monitor
  mode 0011 enters a limit address, 0010 a current address and starts the
  channel, 0012 clears its flags and stops it; 0012j1 also raises the Master
  Clear line of an output channel or forgets a held Ready of an input
  channel. 033 reads the lowest numbered channel that asks for an interrupt,
  a current address, or the error flag, which is always zero. Only the low
  four bits of the channel number count, and 0 to 7 name no channel. A
  channel moves 16-bit parcels, four to a word, to or from absolute
  addresses. An input channel stops at its limit or at the device's
  Disconnect and holds a Ready that finds it stopped. A channel that asks
  sets the I/O interrupt flag outside monitor mode. No device is connected
  yet; the simulation cables each output channel to the input of its pair.
- 0021 to 0027 and 073i01 wait for results still on their way, so that a
  floating-point error is counted under the modes its instruction saw. This
  holds for 0021 and 0022 on the CRAY-1 setting as well.

Not there: the X-MP's 24-bit constant `Ah exp` (01hijkm with the high bit of
i), `Ai VL` (023i01), the second vector logical unit, gather and scatter, the
interrupt monitor mode, the X-MP's rule for VL, the 100 Mbyte channels and
channel parity.

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
beside it. Told the real target, the fitter closes those paths in about 12.2 ns,
which is what allows 81.67 MHz. There is next to nothing to spare: from one
build to the next the worst path has come out between 0.27 ns inside the
clock period and 0.06 ns over it. The build in `releases` is 0.06 ns over on
two paths from the S register bypass into the multiply unit; it passes the
whole hardware regression. Closing that again is left until the feature work
is done, and 73.5 MHz is the setting to fall back to. Two changes were
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

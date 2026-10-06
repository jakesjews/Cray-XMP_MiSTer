# The machine

Technical notes on what this core implements, where it came from and where it
differs from the real thing.

The machine is a CRAY X-MP with one processor and its I/O Subsystem, as far as
the operating system COS 1.17 needs one: that is the only operating system
that survives for these machines, and it is a build for the X-MP. The CPU is
the CRAY-1 as sold in 1982, with its two instruction set options, plus the
X-MP features COS was found to use. The I/O Subsystem is three I/O Processors
with the devices COS and the subsystem's own software work with.

The references are the CRAY-1 Hardware Reference Manual, publication 2240004
revision C, whose page numbers are used below, and revision F of May 1982
(HR-0004) where the two differ (pages marked "rev F"); the CRAY X-MP Series
Model 14 mainframe reference manual, CSM-0111000; and the I/O Subsystem
hardware reference manual, HR-0030.

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

A parameter `XMP` selects the CPU. `XMP = 1` is what the core is built with:
it adds what a one-processor CRAY X-MP has that COS needs; see "The X-MP
setting" below. `XMP = 0` is the CRAY-1 of 1982. No core is built from it any
more, but the CPU was developed and verified as a CRAY-1 first, and most of
its tests still run on that setting. The upstream source's own X-MP code
(channels and the registers shared by four CPUs) did not work and has been
removed.

## What the CPU implements

At the CRAY-1 setting:

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
- 1,048,576 words of memory.

## Differences from a real CRAY-1

- **Timing.** One clock period is one cycle of the machine's own FPGA clock,
  73.5 MHz against the CRAY-1's 80 MHz and the X-MP's 105. Functional unit times in clock
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
Model 14 mainframe reference manual, CSM-0111000. It is tested against the
reference model, which with a model of the I/O Subsystem dead starts COS 1.17
and runs batch jobs.

What changes with `XMP = 1`:

- **Memory** has four million words.
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
  sets the I/O interrupt flag outside monitor mode. The first pair, 10 and
  11, leads to the MIOP of the I/O Subsystem; the other three lead nowhere.
- 0021 to 0027 and 073i01 wait for results still on their way, so that a
  floating-point error is counted under the modes its instruction saw. This
  holds for 0021 and 0022 on the CRAY-1 setting as well.

Not there: the X-MP's 24-bit constant `Ah exp` (01hijkm with the high bit of
i), `Ai VL` (023i01), the second vector logical unit, gather and scatter, the
interrupt monitor mode, the X-MP's rule for VL, the 100 Mbyte channels and
channel parity.

## The I/O Subsystem

COS does no input or output itself. It talks to the I/O Subsystem, a cabinet
of up to four 16-bit I/O Processors with their own software (the kernel and
its overlays), which also is the operator's way into the machine. This core
has three of them, which is what the stock software of COS 1.17 expects:

- **MIOP**, the master. It dead starts the others and the mainframe, and has
  the operator's console and the station.
- **BIOP**, which has the disk drives and the 100 Mbyte channel that moves
  their data to and from central memory.
- **XIOP**, which on a real machine has the block multiplexer channels to tape
  units. Here it is a processor with a console and nothing else; the software
  wants it to answer.

What they are made of (`rtl/ios/`):

- The processor (`iop_cpu.v`): accumulator, carry, B register, 512 operand
  registers, a 16-entry exit stack, 65,536 parcels of Local Memory. An
  instruction takes 3 to 5 clocks.
- On every processor: a real-time clock that asks for an interrupt every
  millisecond, a channel to Buffer Memory, and a channel pair to each other
  processor, over which one can master clear and dead start another.
- Buffer Memory, shared by the three, in DDR3.
- On the MIOP: the Peripheral Expander with a tape drive (the boot tape, which
  holds the kernel's overlays), a disk (the COS binary, parameter files and
  jobs) and a printer; the channel pair to the mainframe's channels 10 and
  11, with the lines that master clear the CPU; four consoles, of which the
  operator's and the station are shown.
- On the BIOP: nine DD-29 disk drives and the channel pair into central
  memory.

The consoles are Ampex Dialogue 80 terminals as far as the software uses
them (`rtl/terminal/term_ampex.v`).

Not there, because the software runs without them: the concentrator for a
front-end computer (the kernel says so once, some seconds after COS has been
started: `Concentrator ordinal 3  VAX interface select error. Command
aborted.`), the error log channel, the block multiplexer channels, the clock
on the expander (the operator types date and time), and writing to tape.

Device times are not those of the real devices. The disks answer as fast as
the SD card does; the software was found to work with all devices from
instant to several times slower than real ones.

### Starting

The core does what an operator with a boot tape did, up to the point where
the kernel runs. The boot file ([tools/py/mkboot.py](../tools/py/mkboot.py))
holds the kernel and the tape. After every reset `rtl/mister/xmp_boot.v`
checks the file and copies the kernel to the start of Buffer Memory, and the
MIOP loads its Local Memory from there. The kernel tests memory, loads its
overlays from the tape, starts the other two processors and asks for the date.
The CPU is held by Master Clear until `START` is typed; then the kernel loads
COS from the expander disk through the channel pair and lets the CPU go.

### What is printed

The printer on the Peripheral Expander is where the output of batch jobs goes.
The core shows what is printed on a screen of its own and writes it to a text
file on the SD card (`rtl/mister/print_spool.v`). The file has a fixed length
and is all line feeds when new; printing fills it from the top and goes on,
in a later session, behind what is there.

## Clocks

The machine and the HPS interface, which brings the disks' blocks, run on one
PLL output. The screens, the keyboard and the serial port run on the 29.4 MHz
video clock. The two meet only in `rtl/mister/cdc.v`: a two-flip-flop
synchroniser for levels and a handshake that carries one console character at
a time in each direction. `CrayXMP.sdc` tells the timing analyser the two
clocks are unrelated, and the core-level simulation runs both.

The machine clock is set in `rtl/pll/pll_0002.v` (`output_clock_frequency1`)
and named in `CrayXMP.sv` (`CPU_HZ`), from which the I/O Processors' clocks
count their milliseconds. With the PLL's 735 MHz oscillator the exact choices
are 735 divided by a whole number: 73.5 and 81.67 MHz are the two of interest.

The CPU alone was closed at 81.67 MHz. With the I/O Subsystem beside it the
first fit missed that by 0.99 ns: in the I/O Processors, from Local Memory
through the adder into the operand registers, and in the CPU's paths into the
T register file and the memory unit, which have less room in a fuller FPGA.
That build ran COS on a DE10-Nano all the same. The clock is 73.5 MHz until
those paths are worked on.

What limits the CPU is the path every result takes in one clock period: off
the result bus, through the register file's bypass, through operand selection
and into the first stage of a functional unit, and the instruction issue loop
beside it.

Memory is slower than the real machine's: a scalar load takes about 25 clock
periods against 11 on a CRAY-1, and vector transfers move about one word
every two clock periods, not one per clock period.

## Memory

All large memories are in the MiSTer's DDR3, in 64-bit words from HPS address
`0x30000000`, most significant byte first:

- central memory, four million words, from word 0
- Buffer Memory, four million words, from word `0o20000000`
- the boot file, from word `0o40000000`

`rtl/mister/ddr3_ports.sv` shares the DDR3 port between the CPU, the BIOP's
channel into central memory, the Buffer Memory channels, the tape drive and
the copy of the kernel; they take turns. The Local Memories of the I/O
Processors are block memory in the FPGA.

In the CPU, instruction buffers fill with 16-word bursts. A vector load
stepping by 1 to 7 words also reads whole lines in bursts when three or more
of its elements lie in a line, and picks its elements out as they arrive. A
block or vector store reads the next word from its register while the one
before is on its way to memory.

A vector load or store lets its instruction issue three clocks after it starts
and goes on in the background while other instructions issue; its V register
stays reserved and other memory instructions wait for it. A transfer whose
first or last address is outside the field stays the current instruction
instead, so the range error interrupt is taken right behind it. Scalar
references and block transfers hold issue until they are done.

The CPU's memory port is a request held until acknowledged, one acknowledge
pulse per word. Nothing in the CPU depends on how long memory takes, and the
tests run every program with several memory timings to hold it to that.

## How it was verified

The CPU:

- An instruction-level reference model was written from the manuals, separately
  from the RTL (`tools/crates/model`). Tests compare end states.
- The smoke and directed tests agree with the model in five run modes, at both
  settings of the CPU.
- 10,000 random programs of up to 250 instructions agree with the model in five
  run modes each, and 2,000 more at the X-MP setting.
- The floating-point units match the reference arithmetic on 200,000 random
  cases per operation, streamed and with gaps, and on 79 cases from cray-sim.
- On a real MiSTer, with the CRAY-1 build this core began as, 547 programs
  were run and their memory compared with the model, with no failures.

The model and the RTL share one reading of the manual for anything no test
vector from a real machine covers.

The I/O Subsystem and the core:

- A model of the whole system (`tools/crates/ios`) runs the subsystem's own
  software and COS 1.17: dead start, station, start-up, a batch job whose log
  equals the one that comes with the software.
- The I/O Processor follows the model step by step through the kernel's boot,
  205 million steps on the three processors, and through 4,400 random
  programs: same interrupts, same channel functions, same registers.
- The three processors and their devices boot the kernel in simulation, and a
  self-checking program covers what the kernel's start does not use.
- The whole machine, and the core around it with stand-ins for the MiSTer
  framework, load and start COS in simulation; in the longest run COS reads
  the nine drives and asks the operator its start-up questions.
- The terminal and the printer's file are checked against what they should
  hold on random input.
- Faults were put into the channels, the processors, the devices, the link to
  the mainframe, the terminal and the printer's file, one at a time, to see
  that the tests notice. They found what the tests missed, and the tests were
  extended until every fault was caught, bar three in the link to the
  mainframe: two concern the I/O Master Clear line, which the software only
  ever raises together with the CPU's, and one a parcel that arrives before
  the MIOP listens, which loading and starting COS does not bring about.
- On a DE10-Nano the first build booted the kernel, loaded and started COS,
  logged the station on and went through COS's start-up with the nine drives,
  typed on the keyboard and on the serial port.

## Resources

Quartus 17.0 for the DE10-Nano, first fit: 26,703 ALMs (64%), 504 of 553
memory blocks, 38 DSP blocks. The Local Memories of the three I/O Processors
take 384 of the memory blocks.

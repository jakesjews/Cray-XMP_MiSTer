# The machine

Technical notes on what this core implements, where it came from and where it
differs from the real thing.

The machine is a CRAY X-MP with one processor and its I/O Subsystem, as far as
the operating system COS 1.17 needs one: that is the only operating system
that survives for these machines, and it is a build for the X-MP. The CPU has
the instruction set the X-MP shares with the CRAY-1 of 1982 and the X-MP
features COS was found to use. The I/O Subsystem is three I/O Processors
with the devices COS and the subsystem's own software work with.

The references are the CRAY X-MP Series Model 14 mainframe reference manual,
CSM-0111000; for the instructions the X-MP shares with the CRAY-1, the CRAY-1
Hardware Reference Manual, publication 2240004 revision C, whose page numbers
are used below, and revision F of May 1982 (HR-0004) where the two differ
(pages marked "rev F"); and the I/O Subsystem hardware reference manual,
HR-0030.

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
- `cray_predecode`, everything the issue logic has to know about an
  instruction, decoded a clock before the instruction is the current one
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
- `s_res_lut`, `v_scheduler`: delivery time of 076, and 174 as the reciprocal.
- `i_buf`: 16-word burst fills, and no fill starts during an exchange.
- `i_buf`: buffers of 32 words, as on the X-MP, fetched in two halves.
- `func_top`: the exchange sequence and saved P, holding issue for operands
  another unit is still producing, second parcels no longer decoded as
  instructions, exact decoding of 0020 to 0022, the T register write of 075,
  monitor-mode gating of the clock and XA instructions, the floating-point
  mode flag, field protection and the range flags, the console interrupt,
  and a new vector section.
- `i_buf`: a buffer that is being filled again no longer answers for the
  block it held before. A jump taken after a fetch ahead could otherwise run
  parcels of the wrong block.
- Lint clean-up across the files: signals and registers nothing read were
  removed and operand widths made explicit. Each module was proven equivalent
  to its form before the clean-up with `tools/py/equiv.py`.

The CPU was first brought up and verified as the CRAY-1 of 1982, which is
what the upstream sources set out to be, with the X-MP's features behind a
parameter. The core has only ever been released as the X-MP, and that
parameter and the CRAY-1 it selected are gone; the history of this repository
has them. The upstream source's own X-MP code (channels and the registers
shared by four CPUs) did not work and was removed earlier.

## What the CPU implements

The instructions and registers the X-MP has in common with the CRAY-1, and
what a one-processor X-MP has beyond them that COS 1.17 needs. That was found
by running COS on the cray-sim simulator with one feature after another taken
out; the specification is [spec/machine-spec.md](spec/machine-spec.md). The
CPU is tested against the reference model, which with a model of the I/O
Subsystem dead starts COS 1.17 and runs batch jobs.

- All instructions of Appendix D of the CRAY-1 manual.
- The vector population instructions (rev F pages 4-25 and 4-70): 026ij1
  `Ai QSj`, the parity of the one bits of (Sj); 174ij1 `Vi PVj`, the
  population counts of the elements of Vj; 174ij2 `Vi QVj`, their parities.
  The vector unit takes the 5 clock periods of the X-MP's manual and runs one
  operation at a time together with the reciprocal unit.
- The programmable clock (rev F pages 4-10 and 6-23): 0014j4 `PCI Sj` enters
  the interrupt interval, 0014j5 `CCI` clears the interrupt request, 0014j6
  `ECI` enables it and 0014j7 `DCI` disables it, all in monitor mode only.
  The countdown runs all the time, one count per clock period; a request
  comes every interval + 1 clock periods.
- A, S, B, T and V registers, VL, VM, the real-time clock, XA and a 24-bit P.
- **VL** as the X-MP has it: 0020 enters the low six bits of (Ak) and sets
  the seventh when they are zero, so the register holds 1 to 100 octal, and
  023i01 `Ai VL` reads it. The field of an exchange package is loaded as it
  stands.
- **The 24-bit constant** 01hijkm `Ah exp`, told from the branches 010 to 017
  by the high bit of i. The assembler uses it for a constant that neither
  22 bits nor their complement hold.
- **Memory** has four million words.
- **The exchange package** has the X-MP layout: a 24-bit P, an instruction
  base and limit and a data base and limit of 19 bits each in units of 32
  words, the mode bits of words 1 and 2, the flags, the program state bit and
  the cluster number. The processor number, the memory error fields and the
  VNU, ESVL and EAM bits are stored as zero. Dead start, normal and error
  exits use it.
- **Fields.** Instructions are fetched through the instruction pair, operands
  through the data pair. An address is valid when it plus 32 times the base
  is below 32 times the limit and below four million. Only the low 22 bits of
  an operand address count. A load from outside the data field delivers zero
  and a store is dropped; the operand range flag also needs its mode bit,
  which 0023 sets and 0024 clears. A block or vector transfer goes on after
  such a reference.
- **Branches** take 24 bits and raise no flag themselves. A fetch outside the
  instruction field is the program range error.
- **Interrupt flags**: normal exit, error exit, I/O interrupt, program range,
  operand range, floating-point error, the MCU interrupt (bit 32), the
  programmable clock interrupt (bit 31) and deadlock. They set only outside
  monitor mode. A clock or MCU request made in monitor mode waits and is
  taken when a user program runs.
- **Interrupt monitor mode.** With its bit set, the error exit, program
  range, operand range, floating-point error and deadlock flags set in
  monitor mode too and interrupt the program; the normal exit, clock, MCU
  and I/O flags still do not. A flag that cannot set in the mode a package
  runs in stays as the package brought it.
- **Floating-point range errors** as on manual page 3-21. For the add unit
  that is an incoming exponent of 60000 octal or more; a carry that takes
  in-range operands to 60000 is delivered without the error.
- **A vector register used as operand and result** of one instruction is read
  element by element before it is written: the operation sees what the
  register held before. The CRAY-1's recursive use of such a register is not
  the X-MP's.
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
- **Modes.** 0021 and 0022 switch the floating-point interrupt mode. 0025 and
  0026 switch the bidirectional memory bit, which is only carried. 0027 waits
  for memory references to finish. The floating-point error status bit sets
  on any floating-point error and is cleared by 0021 and 0022. 0021 to 0027
  and 073i01 wait for results still on their way, so that a floating-point
  error is counted under the modes its instruction saw.
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

Not there: the second vector logical unit, gather and scatter, the 100 Mbyte
channels of the CPU and channel parity.

## Differences from a real X-MP

- **Timing.** One clock period is one cycle of the CPU's own clock: 105 MHz,
  9.52 ns against the X-MP's 9.5. An instruction that needs the result of
  another in an A or S register issues in the clock period that result
  arrives, as on the real machine, and the functional units take the clock
  periods of the manual. After a vector instruction has issued, its operand
  registers are free in (VL) + 3 clock periods, its unit in (VL) + 4 and its
  result register in (VL) + 5 + the unit time, and the mask of a 175 is ready
  in (VL) + 4, as the manual has them. So are the branches while their address
  is in an instruction buffer: 5 clock periods taken, 2 not taken, 7 for 005,
  2 more across two buffers, and a branch on A0 or S0 waits until its
  register has not been busy for three clock periods
  (`tests/rtl_only/timing.cal` measures all of these). What is not the real
  machine's: memory references take longer and vary, and with them everything
  that waits for memory, such as a branch to an address that is in no buffer.
  Programs get the same results but not always in the same number of clock
  periods.
- **Chaining.** As on the X-MP (CSM-0111000 page 4-12), a register that is
  reserved as a result and not as an operand does not hold the issue of an
  instruction that reads it, a vector store included. The operation runs as
  the data becomes available, element by element, also behind a vector load,
  whose pauses then show in every operation of the chain. The manual gives no
  clock periods for this but one rule: nothing is lost if the instruction
  issues before or at the time element 0 arrives at the register. The core
  keeps that rule exactly. An element is at the unit that takes it 4 clock
  periods after it arrived at its register, as it is 4 clock periods after
  issue (`tests/rtl_only/timing.cal` has the cases). With it the manual's
  divide of two 64-element vectors takes 3 * 64 + 39 clock periods, where
  the manual says 38: that number is in the CRAY-1 S manual too, and is what
  the CRAY-1's chaining gives.
- **The multiply and the reciprocal match Cray's own simulation of them.**
  Cray's floating-point diagnostic contains a simulation of each unit (the
  listing found is the 1997 edition for the J90; the code goes back to 1980).
  The core's units give the same bits as a transcription of it on millions of
  operands, for 064 to 067 and 070. The complement step of 067 follows it
  too; with it the statistics W. Kahan published from real machines in 1990
  come out, which they do not with the rule the cray-sim project guessed.
  [spec/fp-multiply.md](spec/fp-multiply.md) has the evidence.
- **Half-precision products** keep 29 bits, as that simulation and the
  CRAY-1 S and X-MP manuals have it. The 1980 change packet to the CRAY-1
  manual says 30.
- **Interrupts are precise.** The exchange happens right after the instruction
  that raised the flag. The manual allows a few more parcels to issue.
- **No memory errors.** The memory error flag and the error fields of the
  exchange package are always zero.
- **Encodings the manuals leave undefined.** 0014jk with k = 1 or 2 is a
  pass. 023ijk other than 023i01 is `Ai Sj`. 026ijk with k = 2 to 6 is the
  population count. 174ijk with k = 3 to 7 is the reciprocal. Dead start clears the programmable clock's enable and
  request; on the real machine they are undefined then.
- **A fetch outside the field in monitor mode** is not checked without
  interrupt monitor mode, since the program range flag cannot set there.
- **A store into an instruction** that is already in an instruction buffer
  does not change what runs, as on the real machine (manual page 3-33): the
  buffer keeps the old parcels until it is filled again or an exchange voids
  it. Which blocks are in the four buffers at a given moment follows this
  core's fetch sequence, not the real machine's.
  The reference model has no buffers and runs the new parcel at once, so the
  two can differ on a program that modifies code it is about to run.

## Compared with Cray-on-FPGA

Zorislav Shoyat's Cray-on-FPGA is another rework of the same cray-1x sources,
for a Xilinx board. Every behavioural change it makes was checked against this
core while it was still a CRAY-1. It turned up no fault here beyond one in the
simulator's single-step mode, where an exit with a result still in flight was
taken before its flag was set; that is fixed. Several of its changes differ
from the manual (exit flags set in monitor mode, a vector length of 64 for
`VL 1`), and it still has upstream faults repaired here, so no code was taken
from it. One idea was: its memory instructions issue at once and transfer in
the background. For vector loads and stores that is what the real machine does
(manual page 4-70), and this core now does it too.

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
  registers, a 16-entry exit stack, 65,536 parcels of Local Memory. Its clock
  is the real one's 80 MHz, but an instruction takes 3 to 5 clocks where the
  real processor issues up to one parcel a clock.
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

The kernel takes a date only with a year from 80 to 99 and asks again
otherwise (tried on the system model: 79, 00 and 26 are refused, 80 and 99
taken). That is one reason the MiSTer's clock is not passed on to the
machine; the other is that the kernel's driver for the expander's clock is
switched off in this software, as it was on the machine it came from.

Device times are not those of the real devices. The disks answer as fast as
the SD card does; the software was found to work with all devices from
instant to several times slower than real ones.

### The system on the card

The package for the SD card ([tools/py/mkcos.py](../tools/py/mkcos.py)) is
COS installed on one drive, where the machine the software came from had
nine. The I/O Subsystem has all nine drives, and the nine-drive system of
the cray-sim project runs on the core as it is; the package only makes COS
use the first. What is changed is in two lists of parameters on the expander
disk, the ones COS is installed and started with:

- The master device on channel 20 is the only drive. It is no longer
  reserved for requests by name: with nothing but reserved devices COS
  halts during the install.
- The striped group is off. The parameters make drives 24 to 26 a striped
  group but mark the three drives themselves as not available. COS then has
  the group in its tables with no members and one drive's 18 sectors to a
  track, while the I/O Subsystem's software spreads a track of the group
  over the three drives, 54 sectors to a head. A request that COS lets run
  past sector 17 comes back with other sectors than the ones it addressed at
  the next head, and a dataset that lands on the group ends in `BLOCK NUMBER
  ERROR`. The cray-sim simulator gives the same error with the same disks,
  with the same blocks handed to COS.
- The device in Buffer Memory is off. Buffer Memory does not keep its
  contents, and with the device COS asks at every start whether to write its
  label anew.

The drive is made on the system model: COS installs itself on an empty
drive, and a job saves the programs of [software/cos-tools](../software/cos-tools/README.md)
on it. Nothing of COS itself is patched.

The software has no list of users, so an `ACCOUNT` statement is taken as it
comes. Who owns a saved dataset still matters: an interactive session is
the user `SYSTEM`, a job is the user its `ACCOUNT` statement names, and what
a job saves without naming one belongs to `SYSTEM` after the next start,
where that job no longer finds it. So the jobs here name `SYSTEM`.

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

The printer is a plotter as well: in its graphics mode the parcels it is given
are dots, 1,056 to a row. The banner page of a job has a picture drawn that
way. Here a character stands for eight dots, `X` if any is set, so a row of
dots is as wide as a line of text and the file stays text.

## Clocks

Three clocks, and none is in step with another:

- **The CPU's, 105 MHz**: the X-MP's 9.5 ns clock period (9.52 ns here). The
  port to DDR3 runs on it.
- **The I/O Subsystem's, 80 MHz**: the 12.5 ns of the I/O Processors' own
  oscillator (HR-0030). The HPS interface, which brings the disks' blocks,
  and the printer's file run on it too. It comes from a second PLL
  (`rtl/pll_ios.v`), because 80 and 105 MHz do not both divide out of one.
- **The video clock, 58.8 MHz**, for the screens, the keyboard and the serial
  port. A pixel of the 8x16 font is two of its periods (29.4 MHz, 31 kHz
  lines) and one of the 8x8 font four (14.7 MHz, 15 kHz lines), which is
  what the MiSTer framework's scandoubler needs. The picture goes out
  through the framework's `video_mixer` (scandoubler with HQ2x, scanlines,
  gamma) and `video_freak` (aspect ratio, integer scaling).

The CPU and the I/O Subsystem are two cabinets with an oscillator each on the
real machine, and they meet here as they do there, through signals that do
not assume how the clocks stand to each other (`rtl/xmp_bridge.v`, used in
`rtl/xmp_machine.v`): the Ready, Resume and Disconnect pulses of the channel
pair, each carried as a level that turns over; the parcels, which stand on
their lines from Ready to Resume; the two Master Clear lines; and the I/O
Subsystem's three memory ports, each a request that is handed over, carried
out in the CPU's clock and handed back. Either of them meets the video clock
only in `rtl/mister/cdc.v`: a two-flip-flop synchroniser for levels and a
handshake that carries one console character at a time in each direction.

`Cray-XMP.sdc` tells the timing analyser that the three are unrelated; the
framework's own constraints know the core's first PLL only. The core-level
simulation runs all three. The benches of the machine without the MiSTer give
the CPU and the I/O Subsystem one clock, which the bridges take as well.

The CPU's clock is set in `rtl/pll/pll_0002.v` (`output_clock_frequency1`).
The I/O Subsystem's is set in `rtl/pll_ios.v` and named in `Cray-XMP.sv`
(`IOS_HZ`), from which the I/O Processors' clocks count their milliseconds.

The first build with the I/O Subsystem missed 81.67 MHz by 0.99 ns. What
took the CPU from there to 105 MHz:

- The decision to issue an instruction starts from flip-flops. The
  instruction is decoded while it is still the next instruction parcel
  (`cray_predecode`, kept in a register beside CIP), and the A and S
  schedulers keep the registers that have a result on its way, and the
  register at the head of their pipelines, in registers of their own.
- Results are gathered a clock early. Each of the A and S register files has
  one register that takes whatever result is due in the next clock, and the
  units whose results are due soonest keep result registers of their own. A
  result then goes from a flip-flop through one choice into the register
  file and its bypass. The floating-point units take their operands into
  registers before anything else, within their 6, 7 and 14 clock periods.
- An instruction issues in the clock its operand arrives without the
  decision becoming longer: beside the registers that have a result on its
  way, each scheduler keeps the same list without the result that arrives
  next, and that list is what an instruction's operands are held against.
  The operand itself comes through the bypass.
- Nothing wide waits for the decision to issue. 075, 025 and the return jump
  write their T or B register in the clock after they issue, 003 and 0014j0
  load the vector mask and the real-time clock then, and the units that are
  started by an instruction take in its operands every clock while idle. A
  074 right behind a 075 waits one clock, and so does what reads the vector
  mask right behind a 003; the X-MP manual (HR-0032) lists both waits for
  the real machine.
- A test and set looks at its semaphore a clock before it decides, the limit
  check of the fetch pointer is kept in a register beside P and the target
  of a branch in one of its own, and the memory unit forms its first address
  in a clock of its own.
- The port to DDR3 picks the next of its five users with one bit a user.
  Indexing the users with a number had made Quartus build a multiplier into
  the way of every memory request.
- The I/O Subsystem is off the CPU's clock. An I/O Processor also stores a
  result in Local Memory a clock after forming it, so that the way from
  Local Memory through its adder does not lead back into a memory.

Memory is slower than the real machine's: a scalar load takes about 25 clock
periods against 11 on a CRAY-1, and vector transfers move about one word
every two clock periods, not one per clock period.

COS keeps its own time of day by counting the CPU's clock periods as an
X-MP's, and the station's clock is the I/O Subsystem's, in real milliseconds.
With the CPU at 105 MHz the two agree: in runs on a DE10-Nano the times in a
job's log were within the few seconds that could be told of the station's.
(At 73.5 MHz COS's clock had been about a quarter slow.)

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

DDR3 keeps its contents when a core is loaded, so central memory and Buffer
Memory are filled with zeros the first time after the core has been loaded,
as after switching the machine on (`rtl/mister/xmp_boot.v`). COS takes its
system log up from what it finds in memory, and what a run on other disks
left there makes it stop during start-up with `CRAY HALT`; its operations
manual (SM-0043, changing systems) has a field engineer clear memory for
that case. A reset from the menu leaves the memories as they are, as a
restart of the real machine does.

An instruction buffer holds 32 words, as on the X-MP, and fills with two
16-word bursts: first the half with the parcel that is wanted, which runs as
soon as it is in, then the other half. A vector load
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
- The smoke and directed tests agree with the model in five run modes.
- 12,000 random programs of up to 250 instructions agree with the model in five
  run modes each.
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
  extended until every fault was caught. The last were in the link to the
  mainframe, in what loading and starting COS does not bring about: a parcel
  that arrives before the MIOP listens, and I/O Master Clear without the
  CPU's. The self-checking program now starts the CPU on a few instructions
  of its own for those.
- On a DE10-Nano: the kernel boots and lists the files of its disk; COS is
  loaded and started; the station logs on; start-up reads the nine drives and
  completes; a batch job runs and its printout, banner picture and log, is in
  the printer's file and on the printer's screen; a reset with COS running
  brings the kernel back with the CPU held, and COS starts again; keys typed
  as fast as the serial port carries them all arrive. The first session on
  the hardware found two things no simulation had: a key lost when keys
  queued up, and the printer's graphics mode.
- The bridges between the two clocks have a bench of their own: pulses,
  levels and memory requests under clocks of random periods and the
  machine's own, requests that are taken back, and resets in the middle.
  Faults put into the bridges are found by it or by the tests of the whole
  machine. Three faults in how the machine is wired around them are not
  noticed by any test yet: the CPU let go for a few clocks right after a
  reset, a Disconnect that does not reach the CPU, and a reset that does not
  reach the I/O Subsystem's side of the bridges.
- The build with the CPU at 105 MHz and the I/O Subsystem at 80 MHz was run
  on a DE10-Nano from fresh disks and an empty Buffer Memory: kernel, COS
  loaded and started, start-up to its end, and the batch job, whose printout
  is the one the earlier builds gave but for its times. In an interactive
  session at the station it fetched the dataset lister and the text editor
  from the expander disk and ran them; with the striped group switched off
  the first program fetched runs, which it did not before.
- The build with the framework's video modules was run the same way: start-up
  on fresh disks without clearing the memories by hand, the batch job, the
  job that installs the tools, and after a reset a second start on the same
  disks, where the installed commands were still there. Screenshots of the
  HDMI picture show both fonts, the four aspect ratios (the two custom ones
  at 16:10 and 1:1), the three integer scales, and the 8x8 font doubled with
  scanlines. With HQ2x a line comes out 1,024 pixels wide where 1,280 are
  expected, with the text whole; that has not been looked into. With direct
  video a RetroTINK 4K reports the 8x16 font's picture as 1872x524p
  (1280x384) at 58.80 MHz, 31.41 kHz and 59.94 Hz.
- The package for the SD card was run on a DE10-Nano as it comes out of its
  zip file, with the core under its present name: COS starts from the one
  drive with a single question; the three example jobs assemble, compile
  and interpret their programs and print what the system model prints; in
  an interactive session a program is written in the editor, assembled,
  linked and run, and a FORTRAN program is written, saved and compiled by a
  job submitted from the session, whose printout comes out on the station's
  printer; after a reset and a second start a new session gets the saved
  texts back. The same session runs on the system model, where the package
  is made.

## Resources

Quartus 17.0 for the DE10-Nano: 28,290 ALMs (68%), 528 of 553 memory blocks,
37 DSP blocks, 4 of 6 PLLs. The Local Memories of the three I/O Processors
take 384 of the memory blocks, the line buffers of the framework's
scandoubler 26. Timing is met with the CPU at 105 MHz, with 0.54 ns to spare,
the I/O Subsystem at 80 MHz and the video side at 58.8 MHz.

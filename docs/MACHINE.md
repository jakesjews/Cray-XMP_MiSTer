# The machine

Technical notes on what this core implements, where it came from and where it
differs from the real thing.

The machine is a CRAY X-MP with one processor and its I/O Subsystem, as far as
the operating system COS 1.17 needs one: that is the only operating system
that survives for these machines, and it is a build for the X-MP. The CPU has
the instruction set the X-MP shares with the CRAY-1 of 1982 and what the X-MP
added to it. The I/O Subsystem is three I/O Processors with the devices COS
and the subsystem's own software work with.

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
- the rules by which the A, S and V schedulers let an instruction issue, and
  the tables of where a result comes from
- the scalar integer, logical, shift and population count units
- the B and T register files and the instruction buffers

What was written for this core, in `rtl/cray/`:

- `cray_cpu`, the top, and `cray_mem_mux`, the memory request multiplexer
- `exchange_ctl`, the exchange sequence
- `cray_opnd`, which registers an instruction reads, for holding issue
- `cray_predecode`, everything the issue logic has to know about an
  instruction, decoded a clock before the instruction is the current one
- `mem_fu`, the memory sequencer for scalar, block and vector transfers
- `res_lanes`, the results on their way to the A and S registers, a lane for
  every unit, and `res_regfile`, the A and S registers with a way in for each
  lane; they took the place of the upstream result pipelines and register
  files, which delivered one A result and one S result a clock period, as
  the CRAY-1 does
- `v_regfile` and `v_optrack`, the vector registers and per-unit operation tracking
- `vector_logical`, `vector_add`, `vector_shift`, `vector_pop`
- `fp_add`, `fp_mul`, `fp_recip`, the floating-point units
- `xmp_channels`, the 6 Mbyte channels

Repairs to the upstream files:

- `imm_gen`: the complemented constants of 021 and 041 used a logical not.
- `s_scheduler`: 052 and 053 deliver to S0, not Si.
- The A and S register files: the constants read for register number 0 take
  priority over a result on its way to A0 or S0 (now in `res_regfile`).
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
- Lint clean-up across the files: signals, registers and ports nothing read
  were removed and operand widths made explicit. Each module was proven
  equivalent to its form before the clean-up with `tools/py/equiv.py`.

The CPU was first brought up and verified as the CRAY-1 of 1982, which is
what the upstream sources set out to be, with the X-MP's features behind a
parameter. The core has only ever been released as the X-MP, and that
parameter and the CRAY-1 it selected are gone; the history of this repository
has them. The upstream source's own X-MP code (channels and the registers
shared by four CPUs) did not work and was removed earlier.

## What the CPU implements

The instructions and registers the X-MP has in common with the CRAY-1, and
what a one-processor X-MP has beyond them. What COS 1.17 needs of that was
found by running COS on the cray-sim simulator with one feature after another
taken out; the specification is [spec/machine-spec.md](spec/machine-spec.md).
The CPU is tested against the reference model, which with a model of the I/O
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
  the cluster number. VNU, vector not used, is cleared by the first 076, 077
  or 140 to 177 a program issues, and ESVL says whether the second vector
  logical unit may be used. The processor number, the memory error fields
  and the EAM bit are stored as zero. Dead start, normal and error exits use
  it.
- **The second vector logical unit** (CSM-0111000 page 4-18), which not every
  X-MP had. With ESVL set, a 140 to 145 goes to it when it is free and to the
  full unit otherwise; the merges and 175 have the full unit only. Its unit
  time is 4 clock periods, and it is busy when the floating-point multiply
  unit is and the other way round. Whether COS sets the bit is its matter.
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
- **Gather, scatter and compress index** (CSM-0111000 pages 5-87 to 5-92),
  which the X-MP got in 1985 and COS 1.17 does not use: 176i1k `Vi ,A0,Vk`
  reads the words at (A0) + (Vk element), 1771jk `,A0,Vk Vj` writes them, and
  175ijk with k from 4 to 7, `Vi,VM Vj,Z` and so on, makes the mask and puts
  the numbers of the elements that pass into Vi. The low 24 bits of an
  element of Vk are a signed number.
- **The multiply and the reciprocal** give the bits of Cray's own simulation
  of them. Cray's floating-point diagnostic contains a simulation of each
  unit (the listing found is the 1997 edition for the J90; the code goes back
  to 1980). The core's units give the same bits as a transcription of it on
  millions of operands, for 064 to 067 and 070. The complement step of 067
  follows it too; with it the statistics W. Kahan published from real
  machines in 1990 come out, which they do not with the rule the cray-sim
  project guessed. [spec/fp-multiply.md](spec/fp-multiply.md) has the
  evidence. Half-precision products keep 29 bits, as that simulation and the
  CRAY-1 S and X-MP manuals have it; the 1980 change packet to the CRAY-1
  manual says 30.
- **Instruction buffers.** Four, of 32 words each. A store into an
  instruction that is already in a buffer does not change what runs (manual
  page 3-33): the buffer keeps the old parcels until it is filled again or an
  exchange voids it.

### The clock periods instructions take

The core takes the clock periods of the manual (CSM-0111000 section 5),
apart from memory and the other things under
[Differences](#differences-from-a-real-x-mp). `tests/rtl_only/timing.cal`
measures cases of each kind below with the real-time clock, and
`timing_esvl.cal` the second vector logical unit:

- **Scalar instructions.** An instruction that needs the result of another in
  an A or S register issues in the clock period that result arrives, and the
  functional units take the clock periods of the manual. Results of
  different units that are due in the same clock period all arrive in it:
  only units that can never deliver together share a way into the registers
  (two that write as many clock periods after their instruction issued would
  have had to issue in the same one).
- **Memory instructions.** A scalar reference (10h to 13h) issues in 2 clock
  periods and a block transfer (034 to 037) or a vector transfer (176, 177)
  in 1, and the instructions behind them go on while memory is at work
  (pages 2-6, 5-40, 5-64, 5-92). A scalar reference waits a clock period
  longer than other instructions for the register with its address ("Ah
  reserved or busy previous CP"). The instructions that use the B or the T
  registers wait for a block transfer of them.
- **Branches** whose address is in an instruction buffer: 5 clock periods
  taken, 2 not taken, 7 for 005, 2 more across two buffers. A branch on A0 or
  S0 waits until its register has not been busy for three clock periods.
- **Vector instructions.** After one has issued, its operand registers are
  free in (VL) + 3 clock periods, its unit in (VL) + 4 and its result
  register in (VL) + 5 + the unit time. The mask of a 175 is ready in
  (VL) + 4, for 073 a clock period later, and the Vi of a compress index in
  (VL) + 10. 076 has its element in Si in 4, and the element of a 077 is in
  its register in 1.
- **Chaining** (page 4-12). A register that is reserved as a result and not
  as an operand does not hold the issue of an instruction that reads it, a
  vector store included. The operation runs as the data becomes available,
  element by element, also behind a vector load, whose pauses then show in
  every operation of the chain. A store, gather or scatter sees an element a
  clock period after it has arrived. An instruction that issues before or at
  the time element 0 arrives at the register loses nothing, which is the one
  rule the manual gives.

## Compared with Cray-on-FPGA

Zorislav Shoyat's Cray-on-FPGA is another rework of the same cray-1x sources,
for a Xilinx board. Every behavioural change it makes was checked against this
core while it was still a CRAY-1. It turned up no fault here beyond one in the
simulator's single-step mode, where an exit with a result still in flight was
taken before its flag was set; that is fixed. Several of its changes differ
from the manual (exit flags set in monitor mode, a vector length of 64 for
`VL 1`), and it still has upstream faults repaired here, so no code was taken
from it. One idea was: its memory instructions issue at once and transfer in
the background. That is what the real machine does (CSM-0111000 pages 2-6,
5-40, 5-64 and 5-92), and this core now does it too, for scalar references
and block transfers as for vector loads and stores.

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
  is the real one's 80 MHz, and an instruction takes the clock periods in
  which the manual has its result delivered (HR-0030 section 6): one for a
  PASS, a constant or a function that sends, two to five for the other
  register instructions, seven to thirteen with an operand in Local Memory,
  five for a branch inside the instruction stack and nine for one that
  leaves it. The reference model counts the same, and the processor is
  checked against it clock period for clock period.
- On every processor: a real-time clock that asks for an interrupt every
  millisecond, a channel to Buffer Memory, and a channel pair to each other
  processor, over which one can master clear and dead start another.
- Buffer Memory, shared by the three, in DDR3.
- On the MIOP: the Peripheral Expander with a tape drive (the boot tape, which
  holds the kernel's overlays, or a tape file from the menu), a disk (the
  COS binary, parameter files and jobs) and a printer; the channel pair to
  the mainframe's channels 10 and 11, with the lines that master clear the
  CPU; four consoles, of which the operator's and the station are shown.
- On the BIOP: nine DD-29 disk drives and the channel pair into central
  memory.

The consoles are Ampex Dialogue 80 terminals; see [The consoles](#the-consoles).

The operator types the date and the time. The kernel takes a date only with
a year from 80 to 99 and asks again otherwise (tried on the system model: 79,
00 and 26 are refused, 80 and 99 taken). That is one reason the MiSTer's
clock is not passed on to the machine; the other is that the kernel's driver
for the expander's clock is switched off in this software, as it was on the
machine it came from.

### The consoles

The operator's console and the station are Ampex Dialogue 80 terminals
(`rtl/terminal/`), which is what the I/O Subsystem's software drives them
as. No manual of the terminal could be found, but its firmware could: the
program ROMs and the character generator are on bitsavers. What the terminal
does was read out of them, and the core's terminal does the same, code by
code and oddity by oddity:

- **The screen** is 24 lines of 80 characters under a status line, which
  shows the attributes and modes that are on, a locked keyboard, the page,
  and an error (`MODE ERR`, `CURS ERR`, `PROT ERR`) until CTRL+CLEAR takes it
  away. There are two pages of display memory (`ESC N`, `ESC F`, the PAGE
  key, and auto flip, `ESC v`).
- **A character** has a protect bit, which shows as half intensity, and four
  attributes: reverse, blank, flash and underline (`ESC j` to `ESC q`). The
  attributes are only stored while write attribute mode is on (`ESC A`), the
  protect bit while write protect is (`ESC )`).
- **The functions:** cursor address and report, tab stops, erasing to the end
  of the line or page with spaces or nulls, clearing, inserting and deleting
  characters and lines, the eleven line drawing characters (`ESC G A` to
  `ESC G K`), fields (`ESC f`), protect mode, in which the cursor only rests
  on unprotected characters, block and conversation mode, sending a line or
  the page to the computer, program mode, which puts control characters on
  the screen as pictures instead of obeying them, and locking the keyboard.
- **The keyboard** is a PC's. Its letters, digits and signs send what the
  terminal's did, with CTRL for the control characters; the arrow keys and
  Home send the terminal's codes for them (08, 0C, 0B, 0A and 1E hex). The
  terminal's own keys, which act on the screen and are never sent, are on the
  function keys:

  | Key | Alone | With Shift | With Ctrl | With Ctrl and Shift |
  |---|---|---|---|---|
  | F4 | SEND LINE | the protected fields too | SEND PAGE | the protected fields too |
  | F5 | CLEAR | with nulls | CTRL+CLEAR | |
  | F6 | LINE INS | LINE DEL | | |
  | F7 | CHAR INS | CHAR DEL | | |
  | F8 | LINE ERASE | with nulls | PAGE ERASE | with nulls |
  | F9 | TAB SET | TAB CLEAR | | all tab stops |
  | F10 | conversation mode | block mode | | |
  | F11 | protect mode off | on | write protect off | on |

  Insert and Shift+Insert are CHAR INS and LINE INS as well, Shift+Delete
  and Ctrl+Delete CHAR DEL and LINE DEL, Shift+Tab BACK TAB and Page Down the
  PAGE key. F1 to F3 choose the screen and F12 is the MiSTer's menu.
- **The bell** is a tone of 1 kHz on the audio outputs.

Where the firmware can go round a loop for good (`ESC X` in protect mode
with auto flip is one way there), the terminal takes no more characters or
keys until the machine is reset, as the real one would until it was switched
off.

The third screen, what the printer prints, is a plain one: 24 lines that
take characters and new lines.

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

### Tapes

The tape drive on the Peripheral Expander reads, writes, writes file marks,
spaces over records in both directions and rewinds (`rtl/ios/ios_expander.v`).
After a reset the boot tape is on it, as an operator puts it on for a dead
start. It has no write ring: the kernel answers `@MT0: NO WRITE RING` to an
attempt to write on it.

A tape file chosen in the menu takes its place until the next reset
(`rtl/ios/ios_reel.v`). It is a file in `.tap` form: a record is its length
in four bytes, low byte first, its bytes and the length again, and a length
of zero alone is a file mark. The file keeps its length. Its whole blocks of
512 bytes are the tape, 16 Mbytes of them at most, and what is on the tape
ends at the first length whose fourth byte is not zero: the drive writes
four bytes of all ones behind what it writes. A blank tape is therefore a
file of bytes of all ones; [tools/py/mktape.py](../tools/py/mktape.py) makes
one, and lists what a tape holds. A file that is read-only on the SD card is
a tape without a write ring.

The kernel's commands for the drive are `FDUMP` and `FLOAD`, which copy files
of the expander disk to a tape and back, `DDUMP` and `DLOAD` for whole
directories, and `COPY`; the station has `SUBMIT,@MT0:n` and `SAVE,@MT0:n,dsn`
for a job or a dataset in a tape file. `HELP` on either console lists them.

### Device times

With the menu's "Device times: Fast" a device is done as soon as the board
has moved its data. With "Real" these take the times of the real ones:

- **The DD-29 drives** (HR-0077): a seek 15 ms and up to 80 ms across all
  cylinders, a revolution 16.6 ms, and a sector is done when it has next
  passed under the heads.
- **The consoles**: a character to a display takes ten bits at 9,600 baud.
- **The tape drive**, Data General's 9-track drive of 800 bytes an inch
  (SG-0051 page 1-2; its manual, 015-000040, has the times): 75 inches a
  second, so 60,000 bytes a second, 5 ms to start and to stop, and a rewind
  at 200 inches a second. A record takes 8 ms, which are a start and what is
  left of the gap, and its bytes, read, written or spaced over; the boot
  tape's 152 records take 12 seconds.
- **The printer**, a Gould 5000: 1,200 lines a minute, so a new line takes
  50 ms, and a row of dots in graphics mode 3 ms.
- **The channel pair to the CPU**: 6 Mbytes a second at most, a parcel every
  27 clock periods of the I/O Subsystem (HR-0030 page 1-3).

The disk on the Peripheral Expander stays as fast as the SD card: the times
of its 80 Mbyte drive are in none of the manuals at hand. Buffer Memory and
the 100 Mbyte channel move a word as fast as DDR3 gives or takes it.

## Differences from a real X-MP

What is known to differ, for the whole machine: mostly when things happen,
and what is not there.

### The CPU

- **Central memory is slower and not steady.** It is in the MiSTer's DDR3.
  The X-MP has a word in its A or S register 17 clock periods after a load
  issues, moves a word a clock period in a vector transfer, and takes 19
  clock periods for a branch to an address that is in no instruction buffer
  (CSM-0111000 section 5). Here all of these take longer, and how long
  varies. Measured on a DE10-Nano under COS, in clock periods for each
  element of a loop over 4,096 words: a loop with nothing in it but its
  count 13, with a scalar load 23.6, with a scalar store 14; a vector load
  of 64 words 2.8 a word and a vector store 3.1. A vector add of registers
  takes 72 for its 64 elements, as the manual's times give.
- **One memory port.** The X-MP has three, two for loads and one for stores,
  so that two block or vector loads and a store can be under way together,
  and a memory bank takes a new reference every 8 clock periods (pages 2-5,
  2-22, 5-64). Here one block or vector transfer is under way at a time,
  the next memory instruction of any kind holds issue until it is done, and
  scalar references are made one after the other in the order they issued,
  eight of them waiting at most. A loop that loads two vectors, adds them
  and stores the sum takes 8.9 clock periods an element on a DE10-Nano, the
  three transfers one after the other; the same work with scalar
  instructions takes 64.
- **The instruction behind a block or vector transfer** waits two clock
  periods more unless a look at the first address and the step shows that
  the transfer stays inside the field: a step of less than 2, 16 or 1,024
  words upwards from an address 64, 1,024 or 65,536 words below the field's
  end (a block transfer takes the second of the three). Behind a transfer
  whose first or last address is outside the field, and behind every gather
  and scatter, it waits to the end, so that the range error interrupts right
  behind the transfer.
- **Interrupts are precise.** The exchange happens right after the instruction
  that raised the flag, and the instruction behind it does nothing, whatever
  it is. The X-MP lets the instructions in NIP and CIP issue first (page
  2-12). The mode instructions 0021 to 0024 wait for every result and
  memory reference on its way; the X-MP issues them at once (page 5-16).
- **No memory errors.** The memory error flag never sets and the error fields
  of the exchange package are zero. The two memory error modes and the
  bidirectional memory mode are carried and change nothing.
- **Not there:** the 100 Mbyte channels of the CPU, and channel parity (the
  error flag of a channel is always zero).
- **Which blocks the four instruction buffers hold** at a given moment
  follows this core's fetch sequence, not the real machine's. It shows in
  times, and in a program that stores into code it is about to run.

### The I/O Subsystem

- **An I/O Processor does one instruction after the other.** Each takes the
  clock periods in which the manual has its result delivered; the real
  processor can issue an instruction while those before it are still under
  way. There is no instruction stack: a relative branch of up to 11 octal
  parcels forward or 13 back counts as inside it, any other branch as
  leaving it. Local Memory never makes a reference wait.
- **Devices answer as fast as they can** unless the menu's "Device times:
  Real" is chosen ([Device times](#device-times)). Even then the disk on the
  Peripheral Expander has no times of its own, and Buffer Memory and the
  100 Mbyte channel take what DDR3 takes.
- **Nothing fails.** The Local Memory error flag never sets and the disk
  drives report no faults.
- **The consoles' look is not the Dialogue 80's.** The letters are the fonts
  of the MiSTer's VT52 core in cells of 8 by 16 (or 8 by 8) dots; the
  terminal had 6 by 8 dots in a cell of 7 by 10, with dots shifted by half a
  dot. The line drawing characters and the pictures of the control characters
  are drawn after its character generator. How the terminal showed underline,
  flash and a blanked character in reverse is in no ROM; here the underline
  is the cell's lowest line but one, a flashing character goes out for nine
  frames in eighteen, and a blanked one is an empty cell.
- **Not on the consoles:** the terminal's printer port (`ESC P`, `ESC J` and
  the PRINT key do nothing), its twenty programmable keys, half duplex and
  the margin bell, which were switches, the third and fourth page of
  display memory, BREAK, and the self test with its messages. A character is
  worked off before the next is taken; the terminal took characters into a
  queue of 256 and worked them off behind, and said `COMM ERR` when the queue
  ran over.
- **Not there**, because the software runs without them: the concentrator for
  a front-end computer (the kernel says so once, some seconds after COS has
  been started: `Concentrator ordinal 3  VAX interface select error. Command
  aborted.`), the error log channel, the block multiplexer channels and with
  them the tape units of the XIOP, and the clock on the expander.

### Where the manuals give no answer

Here the core had to choose, and the real machine may do otherwise:

- **The time of a chained element.** The manual has the rule that nothing is
  lost if an instruction issues by the time element 0 arrives, and no clock
  periods. Here an element is at the unit that takes it 4 clock periods after
  it arrived at its register, as it is 4 clock periods after issue: the time
  that makes the rule exact. The manual's divide of two 64-element vectors
  then takes 3 * 64 + 39 clock periods, where the manual says 38; that number
  is in the CRAY-1 S manual too, and is what the CRAY-1's chaining gives.
- **Encodings the manuals leave undefined.** 0014jk with k = 1 or 2 is a
  pass. 023ijk other than 023i01 is `Ai Sj`. 026ijk with k = 2 to 6 is the
  population count. 174ijk with k = 3 to 7 is the reciprocal. 176 with a j
  other than 1 and 177 with an i other than 1 are the load and the store by
  (Ak).
- **Dead start** clears the programmable clock's enable and request; on the
  real machine they are undefined then.
- **A fetch outside the field in monitor mode** is not checked without
  interrupt monitor mode, since the program range flag cannot set there. The
  manual speaks of programs outside monitor mode only.
- **Which vector logical unit** a 140 to 145 takes that had to wait for a
  register: here the second one whenever it is enabled and free. The
  manual's sentence on the case (page 4-18) can be read more than one way.
- **The gap before a disk sector's data.** With the DD-29's times a sector
  asked for while it passes counts as caught, so the sectors of a track
  follow each other 0.92 ms apart and one that has just passed takes a
  revolution. HR-0077 has no time for the gap.
- **The I/O Processor's interrupt and functions.** HR-0030 gives no time for
  the interrupt sequence; it takes the 9 clock periods of a branch that
  leaves the instruction stack. A function that sends takes 1 clock period
  and one that reads 5, after a training workbook (T0201D).
- **The consoles' line speed.** HR-0030 gives none. 9,600 baud is what the
  channels of the later Models C and D are set to by Master Clear
  (CSM-1009-000 page 4-40).
- **The devices on the Peripheral Expander.** HR-0030 describes the channel
  and names the devices; their registers are those the software is known to
  work with. The tape drive's status bits and its answers to what it cannot
  do (illegal, with the error bit, for writing without a write ring; the end
  of the tape where a record would not fit) follow Data General's
  controller, and so do its times; the gap between records is the 0.6 inch
  of the tape format, and a rewind is Done when the tape is at the load
  point. The printer's 1,200 lines a minute and 100 dots an inch are from an
  advertisement of its maker; six lines to the inch are assumed, and a new
  page counts as a line.

## Clocks

Three clocks, and none is in step with another:

- **The CPU's, 105.263 MHz**: the X-MP's 9.5 ns clock period. The port to
  DDR3 runs on it.
- **The I/O Subsystem's, 80 MHz**: the 12.5 ns of the I/O Processors' own
  oscillator (HR-0030). The HPS interface, which brings the disks' blocks,
  and the printer's file run on it too.
- **The video clock, 58.8 MHz**, for the screens, the keyboard and the serial
  port. A pixel of the 8x16 font is two of its periods (29.4 MHz, 31 kHz
  lines) and one of the 8x8 font four (14.7 MHz, 15 kHz lines), which is
  what the MiSTer framework's scandoubler needs. The picture goes out
  through the framework's `video_mixer` (scandoubler with HQ2x, scanlines,
  gamma) and `video_freak` (aspect ratio, integer scaling). The PLL of this
  clock has whole-number dividers; only the CPU's has a fractional one, and
  nothing of the picture runs on that.

  The 8x8 font is the one the core starts with. Its raster is 262 lines at
  15.71 kHz and 59.94 Hz, which a 15 kHz screen takes as it is and the
  scandoubler doubles for others; the menu is drawn into the picture, so a
  core that started with the other font could not be set right on a 15 kHz
  screen. The 8x16 font's raster is 524 lines at 31.41 kHz, of which 400 are
  shown: the line and frame rates of VGA's 640x480. The analog output
  carries the core's own raster in both cases; `vga_scaler=1` in `MiSTer.ini`
  puts the scaler's picture there instead.

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

Each clock has a PLL of its own, because no two of the three divide out of
one oscillator. The CPU's is set in `rtl/pll_cpu.v`; 9.5 ns is 2000/19 MHz,
which takes a PLL with a fractional multiplier. The I/O Subsystem's is set in
`rtl/pll_ios.v` and named in `Cray-XMP.sv` (`IOS_HZ`), from which the I/O
Processors' clocks count their milliseconds. The video clock is the
framework's usual `rtl/pll.v`.

The board's 50 MHz pin reaches three of the chip's six PLLs by itself, and
the framework's PLL for HDMI is in one of them. The CPU's and the I/O
Subsystem's take the other two. The video PLL, whose clock has time to
spare, gets its reference over a global clock line (`cyclonev_clkena` in
`Cray-XMP.sv`) and sits in the other half of the chip.

The first build with the I/O Subsystem missed 81.67 MHz by 0.99 ns. What
took the CPU from there to the X-MP's clock:

- The decision to issue an instruction starts from flip-flops. The
  instruction is decoded while it is still the next instruction parcel
  (`cray_predecode`, kept in a register beside CIP), and the A and S
  schedulers keep the registers that have a result on its way in registers
  of their own (`res_lanes`).
- A result has a short way into its register. Every unit has a lane, and
  units that can never deliver in the same clock share a way into the
  registers, so that six ways lead into the S registers and five into the A
  registers. The units that take longer have their result a clock before it
  is due and write it then. The floating-point units take their operands
  into registers before anything else, within their 6, 7 and 14 clock
  periods.
- An instruction issues in the clock its operand arrives without the
  decision becoming longer: beside the registers that have a result on its
  way, each scheduler keeps the same list without the result that arrives
  next, and that list is what an instruction's operands are held against.
  The operand itself is read off the lane, and whether a read port takes it
  from there or from the register is worked out the clock before and kept in
  a register for every port (`res_regfile`); the decision to issue only
  picks between what was prepared for either answer. Whether A0 or S0 is
  zero, for the branches, is worked out when the register is written.
- A memory unit that has things ready. An address is checked against the
  number of words the field has above its base, which is kept in a register,
  so the check is one comparison in the clock after a scalar reference
  issues; and a vector transfer reads how far its register is filled from a
  register of its own.
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

COS keeps its own time of day by counting the CPU's clock periods as an
X-MP's, and the station's clock is the I/O Subsystem's, in real milliseconds.
With the CPU at the X-MP's clock the two agree: in runs on a DE10-Nano the times in a
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
the copy of the kernel; they take turns. Requests go to the DDR3 in the
order they are taken, several of them on their way at once, and the words
of reads come back in that order: the CPU presents a request a clock, the
others hold each request until its acknowledge. The Local Memories of the
I/O Processors are block memory in the FPGA.

DDR3 keeps its contents when a core is loaded, so central memory and Buffer
Memory are filled with zeros the first time after the core has been loaded,
as after switching the machine on (`rtl/mister/xmp_boot.v`). COS takes its
system log up from what it finds in memory, and what a run on other disks
left there makes it stop during start-up with `CRAY HALT`; its operations
manual (SM-0043, changing systems) has a field engineer clear memory for
that case. A reset from the menu leaves the memories as they are, as a
restart of the real machine does.

An instruction buffer holds 32 words, as on the X-MP, and fills with two
16-word reads: first the half with the parcel that is wanted, which runs as
soon as it is in, then the other half. A block or vector load issues its
reads without waiting for their words, up to eight of them ahead; stepping
by 1 to 7 words it reads the elements that lie in a 16-word line as one
request, from the first of them to the last, and picks them out as the
words go by. A block or vector store reads its register a word a clock and
hands the words to memory as it takes them. Scalar references are presented
to memory one behind the other, in the order they issued.

No memory instruction waits for memory to issue. A scalar reference issues
as soon as both its parcels are there; its address is formed and checked
against the field in the clock after, before the next instruction can
issue, and it then waits its turn in a queue of eight. The register of a
load stays reserved until the word has come; the word of a store is taken
from its register when the instruction issues. A block or vector transfer
issues in the clock it starts and goes on in the background; its B, T or V
registers stay reserved, and other memory instructions wait for it. The
instructions behind it wait until it is known to stay inside the field: not
at all when its first address and its step show that at a glance, two clocks
when the last address has to be worked out, and to the end of a transfer
whose first or last address is outside the field, so that the range error
interrupt is taken right behind it.

No instruction buffer starts to fill while a scalar reference is waiting or
under way, so a word a program has stored is in memory before a fetch
behind the store reads it (the X-MP's manual has the same order, and the
same exception: a fetch that began before the store issued, page 2-6). A
fetch does not wait for a block or vector transfer; it takes its turn
between two of the transfer's words. An exchange waits for everything.

The CPU's memory port is a request held until acknowledged, one acknowledge
pulse per word. Nothing in the CPU depends on how long memory takes, and the
tests run every program with several memory timings to hold it to that.

## How it was verified

The CPU:

- An instruction-level reference model was written from the manuals, separately
  from the RTL (`tools/crates/model`). Tests compare end states.
- The smoke and directed tests agree with the model in five run modes.
- 12,000 random programs of up to 250 instructions, and 2,000 more that are
  mostly chained vector instructions, agree with the model in five run modes
  each.
- The floating-point units match the reference arithmetic on 200,000 random
  cases per operation, streamed and with gaps, and on 79 cases from cray-sim.
- On a real MiSTer, with the CRAY-1 build this core began as, 547 programs
  were run and their memory compared with the model, with no failures.

The model and the RTL share one reading of the manual for anything no test
vector from a real machine covers. The model has no instruction buffers and
runs a parcel that was just stored at once, so the two can differ on a
program that modifies code it is about to run.

The I/O Subsystem and the core:

- A model of the whole system (`tools/crates/ios`) runs the subsystem's own
  software and COS 1.17: dead start, station, start-up, a batch job whose log
  equals the one that comes with the software.
- The I/O Processor follows the model step by step through the kernel's boot,
  318 million steps on the three processors, and through 4,400 random
  programs: same interrupts, same channel functions, same registers, same
  clock periods.
- The three processors and their devices boot the kernel in simulation, and a
  self-checking program covers what the kernel's start does not use.
- The whole machine, and the core around it with stand-ins for the MiSTer
  framework, load and start COS in simulation; in the longest run COS reads
  the nine drives and asks the operator its start-up questions.
- The model's terminal against the firmware of a real Dialogue 80, run on a
  model of the terminal's hardware: 4,700 random streams of 200 to 3,000
  characters and keys, with every cell of every page, the status line, the
  cursor, the modes and what the terminal sends compared along the way. Of
  26 faults put into the model all were found.
- The core's terminal against the model's on 5,600 such streams, and of 109
  faults put into it all were found. The printer's file is checked against
  what it should hold on random input.
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
  (1280x384) at 58.80 MHz, 31.41 kHz and 59.94 Hz. The analog output has not
  been tried on a screen.
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
- The times of the real devices have a bench: the clocks from a function to
  its Done flag for a character to a console, for reads, writes, spacing,
  file marks and a rewind of the tape and for the printer's lines, and the
  clocks from one parcel of the channel pair to the next. With those times
  the machine boots its kernel from the tape and loads and starts COS in
  simulation: `START COMPLETE` comes after 16.6 seconds of machine time.
- The tape drive with its reel runs random commands on 300 tapes, tape files
  with and without a write ring and boot tapes, against a tape kept in the
  bench: the status, the address and the count after each command, what a
  read stored, and the file at the end. Faults put into the drive and the
  reel are found, but for one in a path nothing reaches (a block that was
  written to and is still at hand when a read wants another: every write ends
  by putting its block back).
- With the COS 1.17 software the kernel dumps a file of its disk to a blank
  tape with `FDUMP`, deletes the file and loads it back with `FLOAD`, on the
  system model and on the machine in simulation, and the tapes are the same;
  the core as a whole, with a tape chosen in the menu, writes that tape too.
  On the system model the station then submits the job in the tape's second
  file with `SUBMIT,@MT0:1`, and COS runs it.
- The build with the 9.5 ns clock, the device times and the tape was run on a
  DE10-Nano from the package as it comes out of its zip file, started by
  hand as a user starts it: the core from the Computer menu, the three
  images and the boot file chosen in the menu. COS starts and the three
  example jobs print what they should. With a tape file chosen in the menu
  the kernel dumps a file to it, deletes the file and loads it back, and the
  tape file on the card holds the records and file marks of the dump; after
  a reset the kernel boots from the boot tape again. With "Device times:
  Real" the kernel's boot takes 26.8 seconds where it takes 13.1, COS starts
  and the FORTRAN job runs and prints. A session at the station with the
  text editor works from the keyboard, Backspace included.
- The build with a way into the registers for every unit, memory references
  in the background and the whole terminal was run on a DE10-Nano from the
  package: COS starts and the three example jobs print what they should. The
  FORTRAN job takes 23.3 seconds of CPU time by COS's own count, where the
  build before took 29.1. The status line shows `BLK` after Shift+F10 and
  `PRT` after Shift+F11, F10 and F11 take them away again, and text typed
  after Ctrl+Shift+F11 is dim. A job that assembles a program with the
  macros of `SYSTXT` and a C program compiled on another computer and put on
  the expander disk run and print.
- The core as a whole starts COS in simulation with "Device times: Real" as
  well: the station has logged on after 17.1 seconds of machine time, where
  it takes 3.1 with the fast ones.
- The whole test suite passes on Linux as on macOS (Ubuntu with Verilator
  5.032, macOS with 5.052).
- The build that starts with the 8x8 font was run on a DE10-Nano: its
  picture is 640 by 200 after the core is loaded and 640 by 400 once the
  menu's Font is set to 8x16; COS starts and the FORTRAN job prints. The
  loops whose clock periods are under
  [Differences](#differences-from-a-real-x-mp) were assembled, linked and
  run there as jobs, and timed by the CPU time COS charges them.

## Resources

Quartus 17.0 for the DE10-Nano: 36,182 ALMs (86%), 544 of 553 memory blocks,
38 DSP blocks, 5 of 6 PLLs. Of the memory blocks the Local Memories of the
three I/O Processors take 384, the framework's scaler, scandoubler and menu
83, the screens with their fonts 30, the disk drives' buffers 20 and the
CPU's vector registers 16.

Timing is met with the CPU at its 9.5 ns, with 0.14 ns to spare, the I/O
Subsystem at 80 MHz with 0.37 ns, the framework's HDMI clock with 0.33 ns
and the video side at 58.8 MHz. With the device this full, whether the
design fits at all and whether every clock is met hang on where the fitter
starts (`SEED` in `Cray-XMP.qsf`). For these sources the seeds 1, 2 and 4
each missed one clock, the CPU's or the HDMI clock, by 0.06 to 0.23 ns, and
6, the one set, met them all. For the sources before, of the seeds 1 to 4
only the fourth met every clock and the third did not fit. A change to the
sources is a new draw: try other seeds before looking for a cause.
Against 105 MHz the exact period costs 0.03 ns, and the timing analysis
allows a clock from a PLL with a fractional multiplier 0.09 ns more
uncertainty.

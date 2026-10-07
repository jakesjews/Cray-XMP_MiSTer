# I/O Subsystem devices, channels and boot sequence: specification for the FPGA core

> A working specification, written while the core was built and kept as it was. Manuals are
> named by their file names on [Bitsavers](http://www.bitsavers.org/pdf/cray/). `cray-sim/` is the
> [cray-sim](https://github.com/andrastantos/cray-sim) project, `system/` its ready-to-run
> COS 1.17; `cos-cray1/` and directories of experiments are working material that is not in
> this repository.

Written 2026-10-05. Scope: everything around the I/O Processor (IOP) that the COS 1.17 image
needs to boot through the real IOS software (kernel 4.2.2, serial 302): channel devices inside
and outside an IOP, the links to the mainframe, the peripherals, and the boot sequence. The IOP
processor and its instruction set are specified elsewhere. The mainframe instructions 0010 to
0012 and 033 are defined in `machine-spec.md` sections 1.6 and 2.10; this note defines the
channel model behind them.

## 0. Sources, citation keys and how facts were checked

| Key | Source |
|---|---|
| HW | `HR-0030-...IO_Subsystem_Model_B_Hardware_Reference_Manual-May_1986.OCR.pdf` (rev B). PDF page = 12+N for section 1, 20+N for section 2, 40+N for section 5, 130+N for section 7, 198+N for section 8, 212+N for appendix C. |
| DSK | `HR-0077-...Disk_Systems_Hardware_Reference_Manual-July_1985.OCR.pdf`. PDF page = 10+N for section 1, 14+N for section 2. |
| XMP | `CSM-0111000-B-CRAY_XMP_1_System_Programmer_Reference_Manual-August_1986.OCR.pdf`. PDF page = 26+N for section 2, 50+N for section 3, 216+N for appendix B. |
| SWI | `SM-0046-G-IOS_Software_Internal_Reference_Manual-September_1988.OCR.pdf`. PDF page = 24+N for section 1, 32+N for section 2. |
| OPS | `SG-0051D-01_IO_Subsystem_[IOS]_Operations_Guide_Oct84.pdf`. PDF page = 22+N for section 2. |
| MC | `CSM-1009-000_IO_Subsystem_Models_C_and_D_System_Programmer_Reference_Apr89.pdf` (later IOS model; used only for bypass mode). |
| TRIA | `TR-IA-IOS_Architecture_Software_Training_Manual-July_1987.OCR.pdf`. |
| sim | `cray-sim/simulator/sim_lib/` (non-commercial licence: behaviour is described here in my own words, nothing is copied). A fact marked **sim only** has no manual behind it. |
| cfg | `cray-sim/cos_117.cfg`. |
| log | A function log made for this note: a private copy of the instrumented simulator from `cos-cray1/` with one extra hook that records every IOP channel function (IOP, P, channel, function, A in, value out, busy and done after) and every interrupt taken. It booted `cos_117.cfg` to the station, ran two batch jobs and an interactive session (5.75 million function records). Files: `ios-devices-experiments/` (see section 14.3). A fact marked **log** was observed in that run. |
| art | [The Cray Files](https://www.modularcircuits.com/blog/articles/the-cray-files/) (Andras Tantos). |

`HR-0808_CRAY-1S_HWref_Nov81.pdf` (key S81) is the 1981 description of
the same IOS. HW says it is obsolete and replaces it together with DSK, so only two passages
were used: the system start overview (PDF page 47) and appendix H (PDF page 454). The
simulator's disk model cites its Part 3 page 7-24. Not read: the packet layouts in SM-0045 and
SM-0042 beyond what `cos-cray1/README.md` reports.

Facts that rest on the simulator alone, collected (each is also marked where it appears):
the register protocols of the expander tape, disk, printer and clock (10.2 to 10.5); all image
and tape file formats (8.6, 10.2, 10.3); the value cylinder << 5 after a seek (8.3, supported
by the manual's programming example); the Ampex control sequences (9.3, partly confirmed by
the log); CPU Master Clear implemented as reset plus an MCU interrupt on release (7); I/O
Master Clear resetting all mainframe channels (7); what a missing channel returns (2); the
stub block multiplexer channel (11.3).

Page citations are written `[HW 5-9/49]`: printed page 5-9, PDF page 49. Channel numbers and
function codes are octal, as in the manuals. Other numbers are decimal unless marked `0x` or
given a subscript.

Bit numbering in this note: bit 0 is the least significant bit (2^0), as in HW.

## 1. The minimum system

### 1.1 Processors, memories and channels that `cos_117.cfg` uses

| Unit | Needed | Notes |
|---|---|---|
| Mainframe | 1 CPU, 4,194,304 words | Size is a constant in the COS image (`cos-cray1/README.md`). The high-speed channel was seen to touch addresses up to 0x3FFFE0 (log). |
| Buffer Memory | 4,194,304 words of 64 bits | cfg sets `BufferMemorySize 0x400000`. The kernel prints `MOS SIZE 100K` (100000 octal = 32,768 units of 128 words = 4 MW) (log). Addresses used in the run: 0 to 0x1CE1F (kernel, overlays, messages, kernel storage) and 0x280000 to 0x316800 (log). COS has a Buffer Memory resident device `BMR-0-20` in its deadstart parameters. Manuals: 1, 4 or 8 MW [HW 1-8/20]; "at least one-half million words" [SWI 1-3/27]. |
| IOP 0, MIOP | yes | 65,536 parcels of Local Memory. Starts first. |
| IOP 1, BIOP | yes | Disks and the 100 Mbyte channel. |
| IOP 2, DIOP | no | cfg marks it absent; the kernel never touches it (log: no function ever issued by or for IOP 2; MIOP channels 10 and 11 only get the initial clear). |
| IOP 3, XIOP | **yes with the stock software** | See 1.3. |

Channel assignments (cfg; they match the typical assignment in [HW C-1/213 to C-5/217]):

| IOP | Channel | Mnemonic | Device |
|---|---|---|---|
| all | 0 | IOR | interrupt request |
| all | 1 | PFR | program fetch request |
| all | 2 | PXS | program exit stack |
| all | 3 | LME | Local Memory error |
| all | 4 | RTC | real-time clock |
| all | 5 | MOS | Buffer Memory |
| all | 6, 7 | AIA, AOA | first other IOP, in ascending IOP number |
| all | 10, 11 | AIB, AOB | second other IOP |
| all | 12, 13 | AIC, AOC | third other IOP |
| 0 | 16 | ERA | error log |
| 0 | 17 | EXB | Peripheral Expander |
| 0 | 20, 21 | CIA, COA | from and to mainframe channels 11 and 10 |
| 0 | 24, 25 | CIB, COB | front-end concentrator (not needed for the station; see 11.2) |
| 0 | 40/41, 42/43, 44/45, 46/47 | TIA/TOA to TID/TOD | four consoles; 46/47 is the kernel console, 40/41 the station |
| 1 | 14, 15 | HIA, HOA | 100 Mbyte channel to central memory |
| 1 | 20, 21, 22, 24, 25, 26, 30, 31, 32 | DKA... | nine DD-29 drives |
| 1 | 42, 43 | TIB, TOB | BIOP console (one line feed is written to it; never read) (log) |
| 3 | 20 to 23 | BMA to BMD | block multiplexer channels |
| 3 | 42, 43 | TIB, TOB | XIOP console (one line feed) (log) |

The accumulator channel pairing, from the simulator's wiring and [HW C-1/213 to C-4/216]:

| On IOP | 6/7 | 10/11 | 12/13 |
|---|---|---|---|
| 0 | IOP 1 | IOP 2 | IOP 3 |
| 1 | IOP 0 | IOP 2 | IOP 3 |
| 2 | IOP 0 | IOP 1 | IOP 3 |
| 3 | IOP 0 | IOP 1 | IOP 2 |

The kernel prints its own view at start (log, kernel console): channels 6/7 `IOP1 A->A`, 12/13
`IOP3 A->A`, 16 `ERROR LOG`, 17 `EXPANDER`, 20/21 `CRAY 50MB`, 24/25 `CONC ( 3)`, 40 to 47
`AMPEX 80` with a `*` on 47; resources `MOS SIZE 100K`, `DISK BUFFERS 6`, `SMODS 40`,
`OVL SPACE 30000`, `DAL COUNT 40`.

### 1.2 What each IOP does at start, in one line each (log)

- MIOP: clears its exit stack, sends functions 6 and 0 to every channel 3 to 47, tests memory,
  reads the overlay file from the expander tape into Buffer Memory, deadstarts IOP 1 and IOP 3,
  prints its banner and configuration, reads the clock, then waits for commands.
- BIOP: same clear, loads its tables from Buffer Memory, writes one word to central memory
  address 0x80 through HOA, runs the buffer echo test and the status register test on each of
  its nine disk channels, reserves and releases each drive.
- XIOP: same clear, exchanges start messages with IOP 0 and IOP 1, runs a register test on
  channel 20 (BMA).

### 1.3 Can IOP 3 be left out?

Not without patching software. Three tests were run for this note (log), each with IOP 3 held
in reset:

1. Stock software. The MIOP prints `MOS TEST COMPLETE`, deadstarts IOP 1 and IOP 3, issues
   `AOC : 0` and waits for IOP 3 for ever. No banner, no commands.
2. Configuration overlay `AMAP` patched so that IOP 3 is not configured (5 parcels, below).
   The IOS then starts normally with two IOPs: banner, clock, `START COS_117 DEADSTART`,
   `MFINIT: COMPLETE`, linkage complete, `STATION`, `LOGON`, and COS's first message
   `ENTER CONFIGURATION CHANGES OR 'GO' TO CONTINUE`.
3. Same, then `REPLY,0,GO`: the MIOP stops with `IOP-0 HALT 020` / `IOP STOP IN CDEM` and
   enters the dump dialogue. CDEM is the demon that passes mainframe packets to the other
   IOPs. At this point COS sends its one tape packet (type `D`; it is the 23rd packet of a
   normal start, just before the first disk request), and there is no IOP to give it to.

The AMAP patch of test 2. AMAP is the first overlay of the overlay file; its data starts at
byte 0x14 of tape file 0. Parcel numbers count from the start of the file:

| Parcel | Byte offset | Old | New | Meaning (inferred) |
|---|---|---|---|---|
| 0x12 | 0x24 | 0x009B | 0 | pointer to the IOP 3 section (the IOP 2 pointer at 0x11 is already 0) |
| 0x25, 0x26 | 0x4A, 0x4C | 0x801B | 0 | MIOP channels 12 and 13: link to IOP 3 |
| 0x53, 0x54 | 0xA6, 0xA8 | 0x801B | 0 | BIOP channels 12 and 13: link to IOP 3 |

Zeroing only the link entries changes the kernel's configuration display but not the
deadstart: the kernel deadstarts IOP 2 and IOP 3 according to the section pointers (kernel
0x4A0C to 0x4A13).

So the choices are:

1. Build IOP 3 as a third copy of the IOP with a stub block multiplexer channel on channel 20
   (section 11.3). This is what the simulator does and is known to boot and run jobs.
2. Two IOPs plus the AMAP patch plus one more patch that is **not yet found**: either COS must
   not send the `D` packet, or CDEM must answer it instead of halting (Q1).

COS itself does not need the tapes: the deadstart parameter file configures `MT0` to `MT3` as
`NAVAIL` (`cray-sim/target/cos_117/disk_content/station/deadstart.param`).

## 2. The generic channel interface, as a device sees it

An IOP has up to 40 channels, numbered 0 to 47 octal [HW 5-1/41]. Each has:

- A 4-bit function code and a function strobe, valid for one clock period, with the 16-bit
  accumulator as output data [HW 5-3/43]. Instructions 140 to 157 take the channel from the d
  field (`iod : f`), 160 to 177 from the B register (`IOB : f`) [HW 7-1/131].
- A 16-bit value returned to the accumulator. In the simulator only functions 10 to 13 (bit 3
  set and bit 2 clear in the function code) load A and the carry; the carry gets bit 16 of the
  returned value, that is 0 (sim, `cray_iop.cpp`).
- A Busy flag and a Done flag, sampled into the carry by instructions 040 (`C = 1, iod = DN`),
  041 (`= BZ`), 042 and 043 (the same through B) [HW 5-3/43, 7-2/132].
- An Interrupt Enable flag per channel. Function 6 clears it and function 7 sets it on every
  channel except the Peripheral Expander, where 6 and 7 mean something else [HW 7-1/131].
  Function 0 clears Busy and Done and idles the channel [HW 7-1/131].
- An interrupt request: Done and Interrupt Enable both set [HW 7-2/132].

Rules that hold for all channels:

- An interrupt is taken when a request is present, the System Interrupt Enable flag is set,
  and an instruction completes. The hardware clears System Interrupt Enable, pushes P on the
  exit stack (E = E + 1) and continues at the address held in exit stack location 0
  [HW 5-19/59].
- Priority: channel 0 first, then 1, then 2 to 47 in descending priority, so the lowest
  numbered requesting channel wins [HW 5-1/41].
- `IOR : 10` returns the number of the highest priority requesting channel in the low 9 bits
  of A (6 bits significant), or 0 when none is left. The value changes when the channel's
  Interrupt Enable or its Done flag is cleared [HW 5-9/49]. The kernel's interrupt handler
  loops on `IOR : 10` until it reads 0 (log: 229,550 reads on the MIOP during boot).
- Allow one clock period between function 0 and a Busy/Done test, and between functions 6 or
  7 and `IOR : 10` [HW 5-9/49 footnotes]. A real-speed implementation must update flags in
  the clock period after the strobe.
- Master Clear is sent to every interface when the IOP is deadstarted, and never again
  [HW 5-4/44].
- A function sent to a channel with no interface must do nothing. At start the kernel sends
  functions 6 and then 0 to every channel from 3 to 47 whether or not it exists (log). In the
  simulator a missing channel reads as not busy and not done (sim only).
- The kernel keeps its own software state for channels and waits for Done interrupts. It does
  not test Busy before `AOx : 14` (kernel code at 0x06B7 to 0x06BE in
  `cray-sim/target/cos_117/disasm/iop_kern.asm`).

State after Master Clear in the simulator (sim only; the kernel sets everything it uses, so
only "Done clear, Busy clear" matters): Interrupt Enable is set on MOS, AIx, AOx, CIA, COA,
HIA, HOA, DKx, TIx, TOx and EXB, and clear on PFR, PXS, RTC and ERA.

## 3. Channels inside an IOP

### 3.1 Channel 0, IOR (interrupt request)

| Function | Effect |
|---|---|
| `IOR : 10` | A = number of the highest priority channel with Done and Interrupt Enable set, else 0 [HW 5-9/49]. |

Done is always 1 and Busy always 0, so 040 to 043 on channel 0 set or clear the carry
[HW 5-9/49]. The kernel uses this (kernel disassembly: `C = 1, IOB 0 = BZ` to clear the carry).
No other function is issued (log).

### 3.2 Channel 1, PFR (program fetch request)

A 9-bit register holds the operand register number of the instruction that set the Program
Fetch Request flag. The flag is the channel's Done flag; there is no Busy flag [HW 5-9/49].

| Function | Effect |
|---|---|
| `PFR : 0` | Clear the flag (Done). |
| `PFR : 6`, `: 7` | Clear, set Interrupt Enable. The flag is not changed. |
| `PFR : 10` | A = register number in the low 9 bits; Done is cleared [HW 5-10/50]. The simulator does not clear Done here (sim). |

The IOS never issues a function on channel 1 in the whole run, not even the initial clear
(log). Which instructions set the flag belongs to the processor specification.

### 3.3 Channel 2, PXS (program exit stack)

Sixteen 16-bit locations and a 4-bit pointer E. Done is the Exit Stack Boundary flag; no Busy
flag [HW 5-10/50].

| Function | Effect |
|---|---|
| `PXS : 0` | Clear the boundary flag. |
| `PXS : 6`, `: 7` | Clear, set Interrupt Enable. |
| `PXS : 10` | A = E in the low 4 bits, upper bits 0. |
| `PXS : 11` | A = stack location E. |
| `PXS : 14` | E = low 4 bits of A. |
| `PXS : 15` | Stack location E = A. |

- Master Clear clears location 0. On the deadstart interrupt P is set to 0, execution starts
  at 0, and E is set to E + 1 [HW 5-11/51].
- The boundary flag sets when the stack fills or empties [HW 1-6/18]. In the simulator it sets
  when a push leaves E at 14 or more, and when a pop finds E equal to 0 (sim only).
- Real hardware needs 4 clock periods for a channel access to the stack and 5 before a
  changed value is used; the kernel does three circular shifts of 17 after such a function
  [HW 5-11/51]. A design in which `PXS` functions take effect at once is safe.
- This is the busiest channel: 930,000 functions on the MIOP during boot (log). Functions
  used: 7 (once), 10, 11, 14, 15. Functions 0 and 6 are never used (log). The first thing the
  kernel does is write 0 to locations 1 to 15 (log).

### 3.4 Channel 3, LME (Local Memory error)

| Function | Effect |
|---|---|
| `LME : 0` | Clear the parity error flag. |
| `LME : 6`, `: 7` | Clear, set Interrupt Enable. |
| `LME : 10` | A bits 0-1 bank, bits 2-3 section, bit 4 which byte [HW 5-12/52]. |

Minimum: Done always 0, Busy 0, all functions accepted, `: 10` returns 0. The kernel issues
`LME : 6`, `LME : 0` and `LME : 7` once and never reads it (log).

### 3.5 Channel 4, RTC (real-time clock)

A 17-bit counter that advances every clock period (12.5 ns). On reaching 234177 octal (79,999)
it sets Done, clears to 0 and goes on, giving an interrupt every millisecond. No Busy flag.
`RTC : 10` returns the upper 16 bits of the counter (0 to 39,999) [HW 5-12/52].

| Function | Effect |
|---|---|
| `RTC : 0` | Clear Done. |
| `RTC : 6`, `: 7` | Clear, set Interrupt Enable. |
| `RTC : 10` | A = counter / 2. |

- Every IOP enables it and takes one interrupt per millisecond; the handler issues `RTC : 0`
  (log: 93,854 on each IOP for 94 seconds). 100 interrupts make the kernel's tenth-of-a-second
  tick [SWI 2-49/81].
- Only the BIOP reads the count (`RTC : 10`, 9,108 times, during its disk channel tests)
  (log).
- The simulator returns a free-running count / 2 truncated to 16 bits instead of a count that
  restarts each millisecond (sim). Follow the manual.
- On an FPGA whose IOP does not run at 80 MHz the requirement is one Done per millisecond of
  real time. cfg: `TimerLimit 79999` at a nominal 80 MHz.

### 3.6 Channel 5, MOS (Buffer Memory)

Buffer Memory is addressed in 64-bit words with a 23-bit address; there is no parcel
addressing [HW 8-3/201]. One word holds four parcels: parcel 0 in bits 63 to 48, parcel 3 in
bits 15 to 0, moved in the order 0, 1, 2, 3 [HW 8-1/199, 8-2/200]. Each IOP has its own port
[HW 8-2/200]. The channel moves a block in one direction at a time between Local Memory and
Buffer Memory [HW 5-13/53].

Registers: Local Memory address (16 bits, low 2 bits forced to 0), Buffer Memory address (24
bits, entered in two parts), block length in words (14 bits). All three are cleared to 0 at
the end of a block copy; an omitted entry therefore uses 0 [HW 5-13/53].

| Function | Effect | Busy/Done |
|---|---|---|
| `MOS : 0` | Idle the channel. Must be issued after an error before another transfer [HW 5-15/55]. | both clear |
| `MOS : 1` | Local Memory address = A with bits 0-1 forced to 0. | no change |
| `MOS : 2` | Buffer Memory address bits 23-9 = A bits 14-0 [HW 5-14/54]. | no change |
| `MOS : 3` | Buffer Memory address bits 8-0 = A bits 8-0. | no change |
| `MOS : 4` | Block length = A bits 13-0; read Buffer Memory into Local Memory. Length 0 means 16,384 words (65,536 parcels). | Busy set, Done clear; at the end Busy clear, Done set |
| `MOS : 5` | Same, Local Memory to Buffer Memory [HW 5-15/55]. | same |
| `MOS : 6`, `: 7` | Clear, set Interrupt Enable. | no change |
| `MOS : 14` | A bits 1 and 2 to a diagnostic control register (disable check bits, disable refresh) [HW 5-15/55]. | - |

- A multiple-bit error leaves Busy set together with Done [HW 5-14/54]. The FPGA has no need
  to produce errors.
- The Local Memory address wraps from the last word to 0 during a transfer (sim).
- Order used by the kernel: `MOS : 0`, `: 3`, `: 2`, `: 1`, then `: 4` or `: 5`. It then polls
  Done; `MOS : 7` is never issued and `MOS : 6` once (log). [SWI 2-11/43] confirms that the
  Buffer Memory and 100 Mbyte channels are polled, not interrupt driven.
- Lengths seen: 8 words (inter-IOP messages), 17, 128, 512, 320 and 192 (log).
- Deadstart: if an IOP is master cleared with the deadstart signal, the three registers are
  cleared, a `MOS : 4` starts when Master Clear drops, Interrupt Enable is set, and the Done
  interrupt starts the processor at P = 0 [HW 5-15/55]. That is a 65,536-parcel copy of
  Buffer Memory words 0 to 16,383 to Local Memory 0 to 65,535. The first interrupt an IOP
  ever takes is from channel 5 (log).
- Dead dump: the same with `MOS : 5` and Interrupt Enable clear [HW 5-16/56]. Not used in a
  normal boot.
- "The first instruction executed after a deadstart cannot be an I/O instruction" (S81
  appendix H, PDF 454). The kernel starts with a jump.
- Later IOS models add `MOS : 10` (read errors), `MOS : 15` and `MOS : 16` (bypass mode:
  Buffer Memory to central memory directly) [TRIA 3-18/84; MC 5-1/139]. The kernel contains a
  code path that issues `MOS : 16` (0x083D) but **never takes it** on this configuration:
  no `MOS : 16`, `: 10`, `: 14` or `: 7` appears in 5.75 million functions (log). The Model B
  has no bypass [SWI 1-3/27]. Do not build it.

Timing constraint: after deadstarting another IOP the MIOP waits a fixed 40,000 passes of a
three-instruction loop and then restores the word it patched in Buffer Memory (kernel 0x493D to
0x4944; real hardware finishes the copy in about 2 ms [HW 5-17/57]). The copy must finish
inside that wait, or the source IOP must be held until it does (the simulator holds it: sim).

## 4. Inter-IOP accumulator channels (AIA/AOA, AIB/AOB, AIC/AOC)

One 16-bit data register per direction per pair of IOPs. The sender's output channel and the
receiver's input channel are two views of the same register [HW 5-16/56].

Input channel (6, 10, 12). No Busy flag.

| Function | Effect |
|---|---|
| `AIA : 0` | Clear Done. |
| `AIA : 6`, `: 7` | Clear, set Interrupt Enable. |
| `AIA : 10` | A = the word; the register is emptied; Done clears; the sender's output channel gets Done (and an interrupt if enabled) [HW 5-16/56]. |

Done on the input channel is set when the other IOP writes a word with `AOA : 14` ("this
function generates an interrupt on the corresponding input channel in the receiving
processor") [HW 5-18/58].

Output channel (7, 11, 13).

| Function | Effect |
|---|---|
| `AOA : 0` | Clear Busy and Done. |
| `AOA : 1` | Control register = A bits 3-0: bit 0 Master Clear, bit 1 Deadstart, bit 2 Dead dump / Master Clear Buffer Memory, bit 3 Short transfer [HW 5-17/57]. |
| `AOA : 6`, `: 7` | Clear, set Interrupt Enable. |
| `AOA : 14` | Set Busy, send A. When the receiver takes the word Busy clears and Done sets [HW 5-18/58]. |

Deadstart of another IOP [HW 5-17/57]:

- Set Master Clear and Deadstart together for at least 200 ns, then clear both. The target
  copies 65,536 parcels from Buffer Memory address 0 to its Local Memory address 0 (about
  2 ms) and is then interrupted, which starts it at P = 0.
- With bit 3 also set the copy is 4,096 parcels (about 100 microseconds).
- Master Clear with Dead dump copies Local Memory to Buffer Memory and does not interrupt.
- Dead dump set while Master Clear is clear may master clear Buffer Memory [HW 5-18/58]. Not
  used in a boot.

What the software does (log):

1. Before each deadstart the MIOP writes the target's IOP number into the kernel image in
   Buffer Memory (the word holding Local Memory parcel 0x0CF0), so the same image tells each
   IOP who it is; afterwards it puts the old value back (kernel 0x4904 to 0x4944).
2. `AOA : 1` with A = 3, eight passes of a delay loop, `AOA : 1` with A = 0. Then the fixed
   wait of section 3.6. Done for IOP 1 on channel 7 and for IOP 3 on channel 13. Bits 2 and 3
   are never set.
3. Start handshake: the MIOP issues `AOA : 0` and waits. The started IOP's first action on
   the link is `AIA : 10` (repeated a few times); a read "resumes the channel" and so sets
   Done on the MIOP's output channel even though no word was sent [HW 5-16/56]. The MIOP then
   issues `AIA : 10` twice (discarding), `AIA : 7`, `AOA : 7`, takes the Done interrupt on the
   output channel and clears it with `AOA : 0`. With IOP 3 configured but held in reset the MIOP
   never gets past the wait after `AOC : 0` (section 1.3).
4. Messages afterwards are one word each: bits 15-12 a function code, bits 11-0 an index into
   the sender's message area in Buffer Memory; each message area unit is eight 64-bit words
   [SWI 2-52/84, 2-53/85]. Almost all traffic is code 10 octal (0x8nnn) from the MIOP, and the
   receiver later sends the same word back as the completion notice (log: 6,888 each way).
   Code 0 words carry commands such as the once-a-minute `M$SYNCH` [SWI 2-50/82]. A word of
   the form 177nnn octal means a fatal error in the sender [SWI 2-54/86].
5. Per message the sender issues `AOA : 14`; on the Done interrupt it issues `AOA : 0`. The
   receiver, on its input interrupt, issues `AIA : 10`.

Simulator differences (sim): the output channel's Busy reads 1 when no word is waiting, the
opposite of [HW 5-18/58]; the kernel does not test it. `AIA : 10` with no word waiting returns
the last word (0 at start) and still sets Done on the sender's output channel. Follow the manual.

## 5. The low-speed channel pair between the MIOP and the mainframe

### 5.1 Wiring

| IOP side | Mainframe side | Direction |
|---|---|---|
| MIOP channel 20, CIA | channel 11 octal (output) | mainframe to IOS |
| MIOP channel 21, COA | channel 10 octal (input) | IOS to mainframe |

cfg gives the mainframe numbers in decimal (9 and 8). Mainframe channels 10 to 17 are four
pairs, even for input and odd for output [XMP 2-17/43]. Sixteen data bits, four parity bits
(odd parity per 4-bit group) and Ready, Resume, Disconnect; the output channel of a pair also
has Master Clear [XMP 2-16/42, B-1/217].

A 64-bit word is four parcels; parcel 0 (bits 63 to 48) goes first [XMP 2-16/42]. In Local
Memory the same four parcels sit at ascending addresses.

### 5.2 The 16-bit handshake

Mainframe output channel 11 to CIA [XMP B-4/220]:

1. The CPU sets CL, then CA; setting CA activates the channel.
2. The channel reads the word at CA, advances CA, and sends parcel 0 with Ready.
3. The receiver answers each parcel with Resume; the next parcel follows with Ready.
4. After the fourth Resume: if CA is not equal to CL go to 2. Otherwise send Disconnect, set
   the channel's interrupt request and deactivate it.

CIA to mainframe input channel 10 [XMP B-2/218]:

1. The CPU sets CL and CA; the channel is active.
2. The sender puts a parcel on the lines with Ready (the first one may arrive before step 1;
   the Ready is then held until the channel is activated, with no error and no interrupt
   [XMP 2-18/44]).
3. The channel answers Resume. After four parcels it stores the word at CA and advances CA.
4. The sender ends with Disconnect, after the Resume for its last parcel. The channel sets
   its interrupt request and deactivates. A partly assembled word is stored with the missing
   low-order parcels as zero [XMP 2-18/44]. A Disconnect is ignored if CA = CL or the channel
   is not active [XMP B-2/218].
5. If CA reaches CL first, the channel sets the interrupt and deactivates (table B-1 step 10b
   to 13). The simulator does not do this: its input channel ends only on Disconnect (sim).
   With this software every transfer is exactly as long as the buffer, so both agree.

### 5.3 Mainframe side registers and flags

Per channel: CA, CL (22 bits are enough for 4 MW), Active, Interrupt request, Error.

| Instruction | Effect |
|---|---|
| `0010jk` `CA,Aj Ak` | CA = (Ak); the channel becomes active [XMP 2-15/41]. In the simulator it is active only if CA differs from CL, and the Error flag is cleared (sim). |
| `0011jk` `CL,Aj Ak` | CL = (Ak). |
| `0012j0` `CI,Aj` | Clear the Interrupt and Error flags; on an output channel clear the device Master Clear [XMP 2-15/41]. |
| `0012j1` `MC,Aj` | Clear the flags; output channel: set device Master Clear; input channel: clear a held Ready. The simulator also sets CA = CL = 0 and deactivates the channel (sim). |
| `033i00` `Ai CI` | Ai = lowest numbered channel requesting an interrupt [XMP 2-17/43]; 0 if none (sim). |
| `033ij0` `Ai CA,Aj` | Ai = CA of channel (Aj). |
| `033ij1` `Ai CE,Aj` | Ai = Error flag of channel (Aj). |

Interrupt causes [XMP 2-16/42]: on an output channel CA = CL, at the Resume for the last
parcel; on an input channel a Disconnect while active; a channel error. The I/O Interrupt (IOI)
flag is set for the CPU when a request is present, the CPU is not in monitor mode and not
waiting for an exchange (`machine-spec.md` 2.10). The request stays until `0012` clears it.

Errors: an input parity error is latched and reported in the Error flag at Disconnect or
CA = CL; an output channel flags a Resume while inactive [XMP 2-18/44, 2-19/45]. None of these
occur in a correct FPGA link; `CE` can read 0.

### 5.4 IOP side: CIA (channel 20), input from the mainframe

Registers: Local Memory address (16 bits, low 2 bits forced to 0, advanced by 4 per word),
parcel count (16 bits, positive), four parity error flags, Ready Waiting flag [HW 7-15/145 to
7-17/147].

| Function | Effect | Busy/Done |
|---|---|---|
| `CIA : 0` | Clear the channel, abort any transfer. | both clear |
| `CIA : 1` | Local Memory address = A; start input. Each 4 parcels received are stored. Ends when the parcel count reaches 0 or a Disconnect arrives. | Busy set, Done clear; at the end Busy clear, Done set |
| `CIA : 2` | Parcel count = A. | no change |
| `CIA : 3` | Clear the parity error flags. | no change |
| `CIA : 4` | Clear Ready Waiting (discard a Ready that arrived while the channel was idle). | no change |
| `CIA : 6`, `: 7` | Clear, set Interrupt Enable. Interrupt when Done sets. | no change |
| `CIA : 10` | A = Local Memory address: one more than the last parcel stored. | no change |
| `CIA : 11` | A = status: bits 0-3 parity error flags, bit 15 Ready Waiting [HW 7-17/147]. | no change |

- Ready Waiting sets when a Ready arrives while the channel is idle. If it is not discarded
  and the channel is then started, the waiting parcel is taken as the first parcel
  [HW 7-16/146]. So a packet that COS sends before the MIOP has re-armed is not lost.
- Normal state: Busy and not Done, open for a 6-word packet at any time [SWI 2-55/87].
- Sequence used for every packet (log): `CIA : 2` A = 30 octal (24 parcels), `CIA : 1`
  A = buffer address, (`CIA : 7` after the first). On the Done interrupt: `CIA : 0`,
  `CIA : 11`, `CIA : 10`. The handler "validates and saves" both values [SWI 2-55/87].
  `CIA : 10` must return the start address + 24. `CIA : 11` returned 0x8000 on almost every
  packet in the simulator, because its Ready Waiting flag is only cleared by `CIA : 4`, and
  the software carried on (log). Returning 0 is the documented normal value. Do not return a
  parity bit.
- During mainframe initialisation the MIOP also reads single words: `CIA : 2` A = 4,
  `CIA : 1` (log).
- The simulator converts the count with A >> 2 words, so a count that is not a multiple of 4
  rounds down (sim). The software only uses 4 and 24.

### 5.5 IOP side: COA (channel 21), output to the mainframe

Registers: Local Memory address, parcel count, Sequence Error flag, external control register
[HW 7-18/148 to 7-21/151].

| Function | Effect | Busy/Done |
|---|---|---|
| `COA : 0` | Clear the channel, abort any transfer. | both clear |
| `COA : 1` | Local Memory address = A; start output, 4 parcels per memory reference, until the parcel count is reached. A Disconnect is then sent unless Hold Disconnect is set. | Busy set, Done clear; at the end Busy clear, Done set |
| `COA : 2` | Parcel count = A. | no change |
| `COA : 3` | Clear the Sequence Error flag (a Resume arrived while idle). | no change |
| `COA : 4` | External control signals from A; held until the next `COA : 4`. | no change |
| `COA : 6`, `: 7` | Clear, set Interrupt Enable. Interrupt on Done or on a sequence error. | no change |
| `COA : 10` | A = Local Memory address: one more than the last parcel sent. | no change |
| `COA : 11` | A = status: bits 0-3 a 4-bit maintenance channel, bit 15 Sequence Error [HW 7-21/151]. | no change |

`COA : 4` bits [HW 7-20/150]:

| Bit | Signal |
|---|---|
| 8 | Write Disconnect: send a Disconnect with no data |
| 9 | Hold Disconnect: do not send the automatic Disconnect at the end of a transfer |
| 11 | Dead Dump |
| 12 | RTC Interrupt (for systems with no programmable clock) |
| 13 | Error Channel Resume |
| 14 | I/O Master Clear |
| 15 | Cray Mainframe Master Clear |

- The transfer only moves while the mainframe's input channel is active; until then the
  first Ready is held and COA stays Busy (sim; [XMP 2-18/44]).
- Sequence used for every packet (log): `COA : 2` A = 30 octal, `COA : 1` A = address. On the
  Done interrupt: `COA : 0`, `COA : 11` (expects 0), `COA : 10` (expects start + 24).
- Bits 8, 9, 11, 12 and 13 are never set in the run (log); the simulator stops if 11, 12 or
  13 is set (sim).

### 5.6 How a 6-word packet travels

IOS to COS:

1. COS has set CL = buffer + 6 and CA = buffer on channel 10.
2. MIOP: `COA : 2` 24, `COA : 1` address. Twenty-four parcels cross; the mainframe stores six
   words; CA = CL.
3. COA sends Disconnect: channel 10 raises its interrupt and goes inactive. COA sets Done:
   MIOP interrupt on channel 21.
4. COS, on the I/O interrupt: `033` finds channel 10, `0012` clears it, the packet is
   processed and the channel re-armed with `0011`/`0010`.

COS to IOS:

1. MIOP has CIA armed: count 24, Busy.
2. COS sets CL and CA on channel 11. Six words go out as 24 parcels. At CA = CL channel 11
   raises its interrupt and sends Disconnect.
3. CIA's count reaches 0: Done, MIOP interrupt on channel 20.

Every packet is 6 words. The first two parcels are the destination and source identifiers
(for example 0x0541 0x4331 for a disk request, `A` and `C1`) [SWI 2-54/86]. Packet layouts are
in SM-0045 (`cos-cray1/README.md` gives the pages) and are handled entirely by the IOS and COS
software; the hardware does not interpret them. Counts for a boot plus two jobs: 4,497 packets
to COS and 4,505 from it (`cos-cray1/README.md`).

## 6. The 100 Mbyte channel between the BIOP and central memory (HIA 14, HOA 15)

One input channel (central memory to Local Memory) and one output channel (Local Memory to
central memory). 64 data bits and 8 check bits in parallel, 16-word bursts, a separate 12-bit
path that carries the address in three steps [HW 7-21/151]. **There are no CPU instructions for
it**: the IOP starts every transfer and the mainframe side is passive logic with its own
buffers and a port into memory [XMP 2-15/41]. In the FPGA it is a DMA port into central
memory that needs no handshake with the CPU. The wire-level signals (Transmit Address, Address
Ready, Data Ready, Last Word, Transmit Data: [HW 7-22/152 to 7-29/159]) do not have to be
reproduced.

Registers per direction: central memory address (22 bits, entered in two parts), Local Memory
address (16 bits, low 2 bits forced to 0), block length in 64-bit words (14 bits)
[HW 7-30/160, 7-36/166].

| Function | Effect | Busy/Done |
|---|---|---|
| `HIA : 0`, `HOA : 0` | Clear Busy and Done. Needed to clear an error. | both clear |
| `: 1` | Local Memory address = A, bits 0-1 forced to 0. | no change |
| `: 2` | Central memory address bits 23-9 = A bits 14-0 (unused bits must be 0). | no change |
| `: 3` | Central memory address bits 8-0 = A bits 8-0. | no change |
| `HIA : 4` | Block length = A bits 13-0 (0 means 16,384 words); read central memory into Local Memory. | Busy set, Done clear; at the end Busy clear, Done set |
| `HOA : 5` | Same, write Local Memory to central memory. | same |
| `: 6`, `: 7` | Clear, set Interrupt Enable. | no change |
| `: 14` | Diagnostic mode from A bits 2-0 (maintenance only) [HW 7-32/162]. | - |

- Functions 0 to 4 (or 5) issued while the channel is active are a function error; an
  unrecoverable error ends with Busy and Done both set [HW 7-32/162, 7-34/164]. Not needed.
- Word packing is the same as everywhere else: the word's bits 63-48 are the parcel at the
  lower Local Memory address.
- Addresses are absolute central memory addresses. The channel is not subject to the CPU's
  base and limit registers.
- The Local Memory address wraps within Local Memory (sim).
- Sequence used (log): `: 0`, `: 3`, `: 2`, `: 1`, then `HIA : 4` or `HOA : 5` with the
  length, then the kernel polls Done. `: 6` once at start; `: 7` and `: 14` never. The
  simulator stops if `: 14` is issued (sim).
- Lengths seen (log): 512, 320 and 192 words (disk sectors go to memory as 320 + 192 from one
  2048-parcel buffer), 6 and 2 (packets and status the IOS reads or writes in COS tables), 1.
  Boot plus jobs: 10,481 output and 1,969 input transfers.
- When an IOP other than the BIOP needs central memory it asks the BIOP through a message; the
  BIOP copies through a 512-word Buffer Memory buffer [SWI 2-30/62]. So every word that enters
  or leaves central memory passes through the BIOP's Local Memory.
- Mainframe side on real hardware: two 16-word buffers per direction and a memory port that
  has lower priority than the CPU ports [XMP 2-13/39, 2-14/40]. The X-MP manual says the I/O
  Interrupt flag is also set when a 100 Mbyte channel to the SSD completes
  (`machine-spec.md` 2.2); the IOS channel raises nothing in the CPU, and the simulator's
  channel has no connection to the CPU at all (sim).

## 7. How the IOS master clears, loads and deadstarts the mainframe

Manual sequence [XMP 3-21/71]: Master Clear on, I/O Clear on, I/O Clear off, load memory
through the IOS, Master Clear off. Master Clear halts the CPU and forces control latches. The
deadstart forces XA to 0 and an interrupt, so the CPU exchanges with the package at address 0
[XMP 3-14/64]. The exchange package for CPU 0 must be at address 0.

The control lines are COA function 4, bits 15 (CPU Master Clear) and 14 (I/O Master Clear)
(section 5.5). Observed sequence for `START` (log; MIOP unless noted):

| Step | Function | Meaning |
|---|---|---|
| 1 | `COA : 0`, `: 6`, `: 3` | idle the output channel |
| 2 | `COA : 4` A = 0xC000 | CPU Master Clear and I/O Master Clear on |
| 3 | `COA : 4` A = 0x8000 | I/O Master Clear off, CPU Master Clear held |
| 4 | `CIA : 0`, `: 4`, `: 3` | idle the input channel, drop a waiting Ready, clear errors |
| 5 | expander disk reads, `MOS : 5`, messages to the BIOP; BIOP: `MOS : 4`, `HOA : 5` | load memory |
| 6 | `COA : 4` A = 0 | CPU Master Clear off: the CPU starts |
| 7 | `COA : 2` 30 octal, `COA : 1` | send the first 6-word packet |

Effects in the simulator (sim): CPU Master Clear going on puts every CPU in reset, clears
monitor mode and all interrupt flags. Going off, it raises the MCU interrupt in the start-up
CPU, and the first interrupt taken while in reset causes the exchange. I/O Master Clear going
off resets every mainframe channel: CA = CL = 0, inactive, interrupt and error clear. The
simulator does not force XA to 0 on master clear; the manual does. Force it.

"Raising the MCU interrupt" is therefore not a separate function: it is the release of bit 15.
The MCU flag is F bit 32 in the manual's numbering ("set when the MIOP sends this signal",
`machine-spec.md` 2.10). COS takes 2 MCU exchanges in a session (`cos-cray1/README.md`).

The `START` command runs the sequence twice (log; `cos-cray1/results/iolog_boot.txt`):

1. **MFINIT, the IOS's own mainframe check.** Master clear as above; 184 words are written at
   address 0 (a test program and its exchange package); release. The program sends one word
   on channel 11 (the MIOP receives it with `CIA : 2` A = 4, `CIA : 1`). The IOS reads back
   1,536 words from 0x200. Console: `MFINIT: COMPLETE`. This check uses the shared registers
   and cluster numbers (it is where a plain CRAY-1 S fails: `cos-cray1/README.md`).
2. **The real load.** Master clear again. 279,032 words of COS go to addresses 0 to 0x441F7
   in 1,090 transfers (the 8 header words of the binary are skipped: the first sector gives
   320 + 184 words). Then one word at address 0x0C, the 69-word parameter file at 0x441F8, and
   one word at 0x0F. Release.
3. **First packet.** The MIOP immediately sends 6 words with no destination or source:
   word 0 = month, word 1 = day, word 2 = year (0x7C2 = 1986 in the run), then the time. COS's
   first instructions read CA of channel 10, set CL = 0x8E0 and CA = 0x8DA on it and wait for
   its interrupt (art, `the-hunt-for-the-red-bootcode.md`). The packet may arrive before COS
   has activated the channel, so the held-Ready rule of section 5.2 is required.
4. **Linkage.** Console: `CPU <-> MIOP CHANNEL INIT`. The MIOP clears both channels, reads one
   word (`CIA : 2` A = 4), then exchanges the `I` and `J` initialisation packets
   [SWI 2-55/87]. Console: `CPU <-> MIOP LINKAGE COMPLETE`, `START COMPLETE`.

Earlier, when the BIOP kernel starts, it writes one word to central memory address 0x80
(`HOA : 5` with length 1) (log). The IOS has not master cleared the mainframe at that point,
so the CPU must already be idle: hold it in reset from power-on until the first release of
bit 15.

Two things the manual says that the simulator does not do, and that this software does not
need: I/O Clear "clears the input CA register of the MCU channel and activates the MCU input
channel" [XMP 3-21/71]; and the Dead Dump and RTC Interrupt lines. Leave channel 10 inactive
after I/O Clear (open question Q4).

## 8. DD-29 disk drives on a DCU-4 (BIOP channels 20 to 32)

### 8.1 Geometry and capacity

| Item | Value | Source |
|---|---|---|
| Sector | 512 words of 64 bits = 2,048 parcels = 4,096 bytes | [DSK 1-4/14, 2-1/15] |
| Sectors per track | 18 (0 to 21 octal) | [DSK 1-4/14] |
| Head groups (tracks per cylinder) | 10 (0 to 11 octal) | [DSK 2-1/15] |
| Cylinders | 823 (0 to 1466 octal) | [DSK 1-4/14] |
| Capacity | 148,140 sectors = 606,781,440 bytes | |
| Rotation | 16.6 ms; head group switch 6 microseconds; seek 15 to 80 ms | [DSK 2-1/15] |

One channel per drive; each DCU-4 serves four drives through one DMA port [DSK 1-2/12]. Data
moves between the disk and Local Memory only in whole sectors [DSK 2-1/15], in bursts of four
parcels, through two 256-parcel buffers A and B used alternately [DSK 2-12/26].

### 8.2 Registers

- Local Memory Address register: 16 bits, low 2 bits forced to 0, advanced by 4 for each
  burst. Read with `DKA : 10`, written with `DKA : 14`. "The address must be updated at the
  beginning of each sector transferred" [DSK 2-4/18].
- Status Response register: written by `DKA : 1` status requests and by `DKA : 5`, read with
  `DKA : 11` [DSK 2-4/18].
- In the drive: cylinder, head group, offset, difference, interlock and fault registers.

### 8.3 Functions

| Function | A | Effect | Busy/Done |
|---|---|---|---|
| `DKA : 0` | - | Clear Busy and Done. Not interlocked with a disk sequence in progress [DSK 2-5/19]. | both clear |
| `DKA : 1` | mode | Select mode or request status; see below. | Busy set, Done clear; a few microseconds later Busy clear, Done set |
| `DKA : 2` | sector | Read one sector to Local Memory at the address register. Waits for the sector to come round. | Busy set, Done clear; Done set and Busy clear "as the last word of disk data is entered in Local Memory" [DSK 2-13/27] |
| `DKA : 3` | sector | Write one sector from Local Memory. Done when buffer A's last data has gone to the disk; the drive finishes from buffer B [DSK 2-15/29]. | same |
| `DKA : 4` | head | Reserve the unit and select head group A bits 3-0. May be issued at any time; takes effect after the function in progress, so a read can cross to the next track of the cylinder without losing a revolution [DSK 2-17/31, 2-18/32]. | no change |
| `DKA : 5` | cylinder | Seek to cylinder A bits 9-0. On arrival the first sector ID read is put in the Status Response register. | Busy set, Done clear; at the end Busy clear, Done set. On a drive fault Done sets and Busy stays [DSK 2-18/32, 2-19/33] |
| `DKA : 6`, `: 7` | - | Clear, set Interrupt Enable. Interrupt whenever Done is set [DSK 2-19/33]. | no change |
| `DKA : 10` | - | A = Local Memory Address register [DSK 2-20/34]. | no change |
| `DKA : 11` | - | A = Status Response register. | no change |
| `DKA : 14` | address | Local Memory Address register = A. | no change |
| `DKA : 15` | value | Status Response register = A (diagnostic); `DKA : 11` must read it back. | no change |

`DKA : 1` parameters (A, 6 octal digits) [DSK 2-5/19 to 2-11/25]:

| A | Meaning | Status Response afterwards |
|---|---|---|
| 000xxx | Release the unit | - |
| 001xxx | Reserve the unit; selects head group 0. Needed before read, write or seek. | - |
| 002xxx | Clear fault flags | - |
| 003xxx | Return to cylinder 0 (up to 625 ms) | - |
| 004xxx | Select cylinder margin (offset from low 5 bits, direction bit 5) | - |
| 005xxx | Read sector number | sector now under the heads (from a counter) |
| 006xxx | Read error flags | fault flags, table 2-3 [DSK 2-8/22]; 0 = no error |
| 007000 | Read cylinder register | cylinder in the low 10 bits |
| 007001 | Read head register | head in bits 3-0, bit 5 = reserved to this IOP, bit 6 = 600 Mbyte unit: 140 to 151 octal for a DD-29; 0 if not reserved |
| 007002 | Read margin/difference | ones complement of cylinders still to cross; 1777 octal when on cylinder |
| 007003 | Read interlock register | 8 flags, 0 = no fault [DSK 2-12/26] |

Sector ID word, 24 bits: cylinder, head group, sector and four parity bits [DSK 2-3/17]. After
`DKA : 5` the software reads `DKA : 11`, shifts right 5 and compares with the cylinder
[DSK 2-22/36]. The simulator returns cylinder << 5 with zeros below (sim). Return that.

Special modes are selected by sector numbers of 40 octal and above (format 40-61, read
correction code 100-121, read early 200-221, read late 400-421) [DSK 2-14/28]; bit 6 on a
write gives an all-zero correction code [DSK 2-17/31]. None is used in the run (log). The
simulator treats any write with bits 7-5 set as "format": it writes zeros (sim).

Buffer echo mode: after a Master Clear and before the first `DKA : 1`, a `DKA : 3` copies
Local Memory into buffers A and B and a `DKA : 2` copies them back, with no disk involved
[DSK 2-17/31]. **The BIOP does this on every disk channel at start** (log):

1. `DKA : 14` A = X, `DKA : 3`, `DKA : 10` must return X + 01000 octal (512 parcels).
2. `DKA : 14` A = X + 01000, `DKA : 2`, `DKA : 10` must return X + 02000.

So in echo mode a write takes 512 parcels and a read returns the same 512 parcels, and the
address register advances by 512 each time (sim agrees). The unpatched overlay does this
500 times per drive, each time with a new 512-parcel pattern built from `RTC : 10` values
(`INDD29` parcels 0x6C to 0xE6); the simulator's patch makes it once.

Then it writes 500 different values with `DKA : 15` and reads each back with `DKA : 11` (log).

### 8.4 Sequences the software uses (log)

Start, per drive: `DKA : 6`, `DKA : 0`; the echo test; the status register test; `DKA : 0`,
`DKA : 1` A = 001000 (reserve), `DKA : 0`, `DKA : 7`, `DKA : 6`, `DKA : 1` A = 0 (release),
`DKA : 0`, `DKA : 7`.

Read of consecutive sectors (the driver reads ahead, 4 sectors at a time):

1. `DKA : 4` A = head. `DKA : 5` A = cylinder. Interrupt. `DKA : 0`, `DKA : 11` (check
   cylinder << 5).
2. `DKA : 14` A = buffer, `DKA : 2` A = sector. If the next sector is on the next track,
   `DKA : 4` A = next head is issued at once, while the read is busy.
3. Interrupt. `DKA : 0`. Next sector from step 2 with a new buffer address.
4. When the drive goes idle: `DKA : 1` A = 0 (release); the clock demon deselects inactive
   disks [SWI 2-50/82].

Write is the same with `DKA : 3`. Only `DKA : 1` parameters 000xxx and 001xxx occur; no status
request (005 to 007) is ever issued when nothing goes wrong (log). Head values 0 to 9, sectors
0 to 21 octal.

The buffers are 2,048 parcels at addresses that are multiples of 0x800 (log).

### 8.5 Errors the software expects

A fault is signalled by Done with Busy still set [DSK 2-13/27, 2-16/30]. The driver then
reads the error flags (006xxx), the interlock register (007003) and so on, and retries
[SWI 3-13/105 on]. A drive that is not there shows as `DISK NOT READY CH nn` on the BIOP console and
is not fatal [OPS 2-5/27]. For the FPGA: never set Busy with Done; return 0 for error flags
and interlocks; return 0 from 007001 until the unit is reserved.

The simulator's tape patch in the overlay `INDD29` (section 13) changes the pass count of
the buffer echo test from 500 to 1; it is not a wait for the drive.

### 8.6 Image file format (sim only)

`ImageFileName`, one file per drive, no header. 606,781,440 bytes = 823 x 10 x 18 x 4,096.

- Sector offset = 4,096 x ((cylinder x 10 + head) x 18 + sector).
- Within a sector the 2,048 parcels are in Local Memory order, each stored high byte first.
  A 64-bit word is therefore 8 bytes, most significant first. Text in the image reads
  directly (`MD-1-20A` at the start of `biop_dk20.img`).
- The simulator creates a missing file as zeros. A zero-filled drive is "unlabelled"; COS
  needs an `INSTALL` start to format it (art, `a-brave-new-world.md`).

## 9. Consoles (TIx and TOx)

### 9.1 Channels

Keyboard channel (40, 42, 44, 46): a 7-bit register assembled from a serial line.

| Function | Effect |
|---|---|
| `TIA : 0` | Clear Done. |
| `TIA : 6`, `: 7` | Clear, set Interrupt Enable; Busy and Done unchanged. |
| `TIA : 10` | A = the character in bits 6-0, upper bits 0; Done clears [HW 7-5/135]. |

Busy is set while a character is being received and Done when it is complete [HW 7-5/135].
The simulator's Busy is always 0 (sim).

Display channel (41, 43, 45, 47): a 7-bit register sent serially.

| Function | Effect |
|---|---|
| `TOA : 0` | Clear Busy and Done. |
| `TOA : 6`, `: 7` | Clear, set Interrupt Enable. |
| `TOA : 14` | Send A bits 6-0. Busy set, Done clear; when the character has gone, Busy clear, Done set [HW 7-6/136]. |

Driver pattern (log): per output character `TOA : 14`, then on the Done interrupt `TOA : 6`,
`TOA : 0`, and `TOA : 7` again before the next. Per key `TIA : 6`, `TIA : 10`, `TIA : 0`,
`TIA : 7`. Characters are 7-bit ASCII; bit 7 was never set on output (log). The line speed is
not given in the manuals read (Q8); any speed works as long as Done follows.

**An output channel whose Done never comes stops the kernel**: with no terminal attached the
simulator never completes the first character and the MIOP hangs at its first message (log).
Always complete output, attached or not.

### 9.2 Which terminals the software expects

- The terminals are Ampex Dialogue 80 displays: the IOS configuration type is `AMPEX`,
  "AMPEX dialogue 80 display" (`SM-0043G-...pdf` PDF page 90; device type
  `CF$AMPEX` in `SM-0007-...pdf` PDF page 93, which also lists TEC 455
  and TEC 1440). "Two to four Ampex display/consoles attached to the MIOP and one to four
  attached to any of the other IOPs" [OPS 1-2/14]. The kernel prints `AMPEX 80` for channels
  40 to 47 (log).
- **Kernel console**: MIOP channels 46/47 in this configuration (marked `*` in the kernel's
  configuration display; the `START` and `STATION` commands were typed there) (log). It is a
  plain scrolling terminal: only printable characters, LF, CR and one NUL were sent in the
  whole run; lines begin or end with LF then CR (log).
- **Station**: MIOP channels 40/41. Nothing is written to it until `STATION` is typed on the
  kernel console; then it is a full-screen display driven with cursor addressing (log).
- 42/43 and 44/45 on the MIOP are configured consoles that stayed silent. The BIOP and the
  XIOP each write a single LF to their channel 43 and never read channel 42 (log).
- Input lines end with CR (0x0D). The simulator translates the host's backspace (0x08 or
  0x7F) to 0x7F before giving it to the IOS, and drops a 0x7F that the IOS sends (cfg
  `MapBackspace`, `BackspaceChar 127`; sim). Control-D on the kernel console asks for a
  system dump [OPS 2-8/30].

### 9.3 Control sequences

Sent by the station in the run (boot, status displays, an interactive session) (log):

| Sequence | Count | Meaning (sim) |
|---|---|---|
| `ESC = r c` | 657 | cursor to row r - 0x20, column c - 0x20 (0-based) |
| 0x0C | 610 | cursor right one column |
| LF 0x0A, CR 0x0D | 24 each | line feed, carriage return |
| `ESC R` | 20 | delete the cursor line; lines below move up; the last line is cleared |
| BEL 0x07 | 4 | bell |
| `ESC *` | 3 | clear the screen (the standalone emulator also homes the cursor) |
| `ESC T` | 1 | erase from the cursor to the end of the line |

Also handled by the simulator's terminal code but not seen in the run (sim: `iop_console.cpp`,
and `system/sw/ampex80term/ampex80term.cpp`, whose author
lists them as his reading of the Dialogue 80):

| Sequence | Meaning |
|---|---|
| `ESC j`, `ESC k` | reverse video on, off |
| `ESC n`, `ESC o` | blink on, off |
| `ESC G A` to `ESC G K` | line drawing: A top left, B top right, C bottom left, D bottom right corner; E top, F right, G left, H bottom tee; I horizontal, J vertical line; K cross |
| `ESC space` | identify: the emulator answers `50` CR |
| `ESC ?` | report cursor: answers row + 0x20, column + 0x20, CR |
| 0x08, 0x09 | backspace, tab |
| 0x0B | cursor up |
| 0x1A | clear unprotected characters (whole screen in the emulator) |
| 0x1E | home |
| 0x1F | new line |
| 0x00-0x06, 0x0E-0x19, 0x1C, 0x1D | ignored |

The article text calls the terminals "modified Wyse-50" and the emulator `wy50_con`
(art, `the-matrix.md`); the code and the manuals say Ampex Dialogue 80, and the sequences are
the same family (Lear Siegler ADM-3A style). The emulator's default window is
80 columns by 25 rows.

The front-end protocol that the station carries to COS (SM-0042) runs between the IOS station
software and COS in `B` packets; no hardware is involved beyond sections 5 and 9.

## 10. Peripheral Expander (MIOP channel 17, EXB) and its devices

### 10.1 The channel

The expander is a bus for up to 16 controllers of the Data General kind: each device has
three registers A, B and C, a Busy and a Done flag, an interrupt request, and takes the
control pulses Start, Clear, Pulse and I/O reset (art, `the-matrix.md`; [HW 7-7/137]). Devices
can move data to and from Local Memory by themselves, one parcel per reference
[HW 2-1/21]. All devices share channel 17 and are selected by a 6-bit device address
[HW 7-7/137].

| Function | Effect |
|---|---|
| `EXB : 0` | Idle: clear channel Busy and Done, clear the channel Interrupt Enable, clear DMA enable [HW 7-8/138]. |
| `EXB : 1`, `: 2`, `: 3` | DIA, DIB, DIC: fetch register A, B or C of the selected device onto the data bus; read it afterwards with `EXB : 10` [HW 7-8/138]. |
| `EXB : 4` | Fetch the selected device's Busy and Done and find the highest priority interrupting device; read the result with `EXB : 11` [HW 7-8/138]. |
| `EXB : 5` | Device address = A bits 5-0. Not delayed; Busy and Done unchanged [HW 7-9/139]. |
| `EXB : 6` | MSKO: A is a 16-bit mask; a set bit disables interrupts from the device that owns that bit. Clears the channel's interrupt flag [HW 7-9/139]. |
| `EXB : 7` | Interrupt mode: A bit 0 enables interrupts from the channel (end of a delayed function), bit 1 enables interrupts from the device controllers [HW 7-10/140]. |
| `EXB : 10` | A = data bus (the value fetched by the last `: 1`, `: 2` or `: 3`) [HW 7-10/140]. |
| `EXB : 11` | A = status 1 (table below). |
| `EXB : 13` | A = status 2: the same with the device address register in bits 5-0 [HW 7-11/141]. |
| `EXB : 14`, `: 15`, `: 16` | DOA, DOB, DOC: send A to register A, B or C of the selected device. |
| `EXB : 17` | Send control: A bit 0 Start, bit 1 Clear, bit 2 Pulse, bit 3 I/O reset [HW 7-14/144]. |

Delayed functions (1, 2, 3, 4, 6, 14, 15, 16, 17) set channel Busy and clear Done at issue and
complete at least 1 microsecond later with Done set and Busy clear; the next delayed function
must wait for that Done [HW 7-14/144]. The simulator completes them at once (sim).

Status 1 [HW 7-12/142]:

| Bits | Meaning |
|---|---|
| 5-0 | device code of the highest priority interrupting device |
| 7 | DMA enabled |
| 8 | channel interrupts enabled |
| 9 | controller interrupts enabled |
| 10 | delayed function executing |
| 11 | channel Busy |
| 12 | channel Done |
| 13 | INTR: interrupt request from a device |
| 14 | SELB: Busy of the addressed device |
| 15 | SELD: Done of the addressed device |

Device codes and mask bits [HW 7-10/140]; cfg's `Interrupt` numbers are these mask bits:

| Device code | Mask bit | Device |
|---|---|---|
| 60 | 6 | Ampex disk drive |
| 22 | 5 | Data General tape drive (or Kennedy) |
| 17 | 3 | Gould printer (15: Versatec) |
| 70 | 1 | Chronolog clock |
| 50 | 1 | Hayes clock |
| 27 | 8 | CDC cartridge disk drive |

Interrupt request of channel 17 in the simulator (sim): (mode bit 0 and channel Done) or (mode
bit 1 and some device has its interrupt enabled by the mask and active). The simulator treats
a mask of 0 as "enable all" and 0xFFFF as "disable all" (sim).

S81 appendix H (PDF 454) says the expander "supports block transfers to only the first
100,000 octal parcels" of Local Memory. This software gives the expander disk buffers at
0xD800 to 0xF800 and the tape a buffer at 0x4D5C (log), and the simulator uses the full 16-bit
address (sim). Use 16 bits.

Usage (log): the kernel sends `EXB : 6` with A = 0xFFE7 once at start (as part of the
all-channel clear, where A happens to hold that value) and A = 0 afterwards; `EXB : 7` with
0 or 2 only (controller interrupts on or off; never bit 0); device addresses 0, 17, 22, 50,
51 and 60; controls Start and Clear, and Pulse for the printer; never I/O reset. `EXB : 13`
is never used. After an interrupt the kernel issues `EXB : 4`, `EXB : 11` to learn the device,
then DIA/DIB/DIC with `EXB : 10`, then Clear to the device.

Everything below about the devices' registers comes from the simulator and from the log; the
Cray manuals read do not describe them (the tape is the Data General drive of
`015-000021`, art).

### 10.2 Tape, device 22 (boot tape)

| Register | Use |
|---|---|
| DOA | command in bits 5-3: 0 read, 1 rewind, 3 space forward, 4 space backward, 5 write, 6 write end-of-file, 7 erase |
| DOB | Local Memory address of the buffer |
| DOC | twos complement of the word count (records for space commands) |
| Start | run the command; Done and an interrupt at the end |
| Clear | drop the interrupt |
| DIA | status: bit 0 ready, 2 write-protected, 7 beginning of tape, 8 end of file, 9 end of tape, 10 parity error, 12 illegal, 13 rewinding, 14 data late, 15 error summary |
| DIB | address after the transfer |
| DIC | remaining count |

- A read moves up to the count from the current record; the IOS uses records of 2,048 parcels
  (4,096 bytes), the last one of a file may be short (sim). Reads and spaces stop at an
  end-of-file mark and report bit 8 with bit 15 (sim).
- The boot (log): 152 reads with DOB = 0x4D5C, DOC = 0xF800 (2,048 words), DOA = 0, Start;
  each followed by `EXB : 4`, `EXB : 11` (0xB092: device Done, INTR, channel Done, DMA, device
  22), DIA (1 = ready), DIB (0x555C = buffer + 2,048). Then one DOA = 0x0008 (rewind); a
  later DIA reads 0x0081. Nothing else is done with the tape.
- Parcels are stored on tape high byte first (sim).

`.tap` format (sim: `tap_file.cpp`): a sequence of records, each a 4-byte little-endian
length, the data, and the same 4-byte length again. A length of 0 (4 bytes, no data, no
trailer) is an end-of-file mark. `boot_tape.tap` is 640,240 bytes:

| File | Records | Bytes | Content (`target/cos_117/build_boot_tape`) |
|---|---|---|---|
| 0 | 152 x 4,096 | 622,592 | `iop_overlay1.bin`, the overlay file (AMAP first) |
| 1 | 1 x 4,096 | 4,096 | `tapeboot.bin` (TAPELOAD) |
| 2 | 1 x 4,096 | 4,096 | `diskboot.bin` (DISKLOAD) |
| 3 | 2 x 4,096 | 8,192 | `dmp.bin` (DMP) |

A real deadstart tape has TAPELOAD, DISKLOAD, DMP, the kernel and the overlays as files 0 to 4
[OPS 2-4/26]. The simulator's tape puts the overlays first because its kernel is preloaded
and simply reads "the next file" from the load point.

### 10.3 Disk, device 60 (expander disk: COS binary, parameter files, jobs)

80 Mbyte Ampex drive. cfg: 823 cylinders, 5 heads, 35 sectors of 512 bytes (256 parcels);
73,740,800 bytes.

| Step | Registers |
|---|---|
| options | DOA = 0x4000 (always this value in the run) |
| cylinder | DOB = cylinder, DOC = 5 |
| head | DOB = head, DOC = 1 |
| first sector | DOB = sector, DOC = 2 |
| sector count | DOB = count, DOC = 3 |
| go | DOB = Local Memory address, DOC = command (0 read, 010 write, 020 format, 0120 return to zero), Start |
| end | device Done and interrupt; DIA returns the options word with bit 0 clear; DIB the address after the last parcel; Clear drops the interrupt |

- The IOS always moves 8 sectors (2,048 parcels) and only uses sectors 0, 8, 16 and 24 as
  starting sectors: 32 sectors per track, the last 3 unused (log; art, `parallels.md`).
- Image (sim): no header; sector offset = 512 x ((cylinder x 5 + head) x 35 + sector); parcels
  high byte first. Sector 0 holds DISKLOAD (the text `FILE @DK0:` is in it).
- The file system is the IOS's own; `exp_disk_create` builds it
  (`target/cos_117/build_exp_disk`): volume `IOS`, fixed files DISKLOAD and DMP, directories
  `STATION` (`IOPKERNEL`, `COS_117`, `DEADSTART`, `RESTART`, `INSTALL`, job files), `MARK` and
  `BIN`. The FPGA only has to serve sectors.
- In the run only reads occur (774), each a `START` load or a station `SUBMIT`/`FETCH` (log).

### 10.4 Printer, device 17

Gould printer (sim only). DOA = command with a Pulse: 6 clear interrupt, 4 text mode, 3 new
line, 1 graphics mode, 0 new page. DOB = twos complement word count, then DOC = buffer address
prints the text, two characters per parcel, high byte first. DIA = status, 0x4000 when done.
Not touched during a boot to the station; used for job output and the `IAIOP LOG` listing
(log). A device that accepts everything and reports done is enough.

### 10.5 Clock, devices 50 and 51 (Hayes Chronograph)

A serial clock that speaks Hayes AT commands (sim only; art, `the-matrix.md`). The machine
this software came from had no clock; the simulator enables the driver with patches
(section 13).

- Device 51 transmits to the clock: DOA = character, Start; an interrupt on device 51 when it
  has gone; Clear.
- Device 50 receives: an interrupt on device 50 when a character is waiting; DIA returns it;
  Clear acknowledges and lets the next one arrive.
- Dialogue at start (log): `ATLC` CR gives `0` CR; `ATVD` CR gives `0` CR; `ATVT` CR gives
  `0` CR; `ATRD` CR gives `YYMMDD` CR; `ATRT` CR gives `HHMMSS` CR. `ATSD` and `ATST` set the
  date and time.
- The kernel then prints the date and time and does not ask for them. Without a clock it
  prints `ENTER DATE:` and `ENTER TIME:` and takes `mm/dd/yy` and `hh:mm:ss` [OPS 2-5/27].
- COS accepts years 1980 to 1999 only; the simulator maps the host year into that range
  (cfg `YearLimit`; sim).

## 11. Other channels

### 11.1 ERA, error logging (MIOP channel 16)

On serial numbers 20 and below this channel reports memory and 100 Mbyte channel errors from
the other IOPs, Buffer Memory and central memory; Done sets when any error is latched
[HW 7-41/171]. Functions: 0 idle (clear all error flags and Done), 6 and 7 Interrupt Enable,
10 read error status (9 bits: IOP-1, IOP-2, IOP-3 Local Memory, Buffer Memory, central memory,
100 Mbyte input A, output B, input C, output D), 11 to 13 read three parameter words selected
by a bit in A [HW 7-43/173 to 7-47/177]. On serial 21 and above the errors go to a
maintenance computer and "are not available to the MIOP" [HW 7-47/177].

Functions 11 to 13 decode the accumulator in the interface, so the program must put a
logical product instruction immediately before them [HW 7-44/174]; S81 appendix H gives the
same rule for functions 10 to 13 of the error logging and block multiplexer channels. An
FPGA interface that samples A with the function strobe has no such restriction.

The kernel issues `ERA : 6`, `ERA : 0` and `ERA : 7` once and nothing else in the whole run
(log). Minimum: accept functions 0, 6, 7; never set Done; return 0 for 10 to 13. Busy 0.

### 11.2 Front-end concentrator (MIOP channels 24 and 25)

A second CIA/COA-type pair for a front-end computer (`CONC ( 3)` in the kernel's
configuration). The function set is that of sections 5.4 and 5.5 (sim: `iop_concentrator.cpp`,
which carries the data over TCP). With nothing connected the kernel prints `Concentrator
ordinal 3 initialized` and `Concentrator ordinal 3 input transfer length error` and carries
on (log). 23 functions on channel 24 and 47 on channel 25 in the whole run. The station on
channel 40/41 does not use it. A pair whose input never completes and whose output completes
without a receiver is enough; whether the channels can be absent is not tested (Q9).

### 11.3 Block multiplexer channel (XIOP channel 20)

Functions in [HW 7-48/178 on]. The XIOP's start-up test on channel 20 issues (log): `BMA : 6`,
`: 16`, `: 0`, `: 5`, `: 7`; `: 13` (returns 0x2000 in the simulator) and `: 12`; `: 1` with
A = 3; then writes patterns with `: 14`, `: 15` and `: 16` and reads them back with `: 10`,
`: 11` and `: 12`. 190 functions in all. Channels 21 to 23 only get the initial clear. The
simulator's model (`iop_bmx.cpp`) is, in its author's words, enough "to let the OS continue,
determine that there's something terribly wrong and never touch the interfaces again" (art,
`the-matrix.md`). If IOP 3 is built, give channel 20 that register read-back behaviour and
nothing more.

## 12. Boot sequence, from power-on to the station

Device touched at each step in brackets. "sim" marks what the simulator does differently from
a real machine.

| # | Step | Devices | Operator |
|---|---|---|---|
| 0 | Overview from the 1981 manual: a tape bootstrap loads into MIOP memory, loads more software from tape into Buffer Memory, the MIOP deadstarts the BIOP from Buffer Memory, and then the operating system is loaded into central memory (S81 PDF 47). | | |
| 1 | Real machine: set the maintenance panel switches to 22 (tape) or 60 (disk), press IOP-0 MC and DEADSTART. File 0 (TAPELOAD) or DISKLOAD is read into MIOP Local Memory at 0 and started [OPS 2-4/26; SWI 2-51/83]. It prompts `FILE @MT0:`. | expander tape or disk, kernel console | types `3` (the kernel is tape file 3) |
| 1s | sim: `iop_kern.bin` (40,960 bytes) is already in Buffer Memory at word 0 and the MIOP gets a Buffer Memory deadstart: 65,536 parcels copied to Local Memory, then the MOS interrupt starts it at 0. | MOS | none |
| 2 | Kernel entry: OR[0] = 0, save OR[511], clear the operand registers, clear exit stack locations 1 to 15. | PXS | |
| 3 | For each channel 3 to 47: function 6, function 0. | every channel | |
| 4 | Local Memory test. Buffer Memory test (4,096-byte blocks out and back). Prints `MOS TEST COMPLETE`. | MOS, TO 47 | |
| 5 | Clear Local Memory above the kernel. Read the overlay file from tape: 152 records of 2,048 parcels, each overlay written to Buffer Memory as it is found. Rewind. | EXB tape 22, MOS | |
| 6 | Deadstart IOP 1: patch the IOP number into the kernel image in Buffer Memory, `AOA : 1` 3 then 0, wait, restore. Same for IOP 3 on channel 13. | MOS, AO 7, AO 13 | |
| 7 | `LME : 0`, `LME : 7`, `ERA : 7`; consoles cleared and enabled; start handshake with IOP 1 and IOP 3. | LME, ERA, TI/TO, AI/AO | |
| 8 | IOP 1 and IOP 3 run the same entry code, find their number, load tables from Buffer Memory, test their channels (BIOP: one word to central memory 0x80, disk echo and status tests, reserve/release of nine drives). | MOS, HOA, DKx, BMA, AI/AO | |
| 9 | MIOP prints the banner `IOP-0 KERNEL, VERSION 4.2.2, Sn302/25, * Leading Edge * 06/06/89 12:35:38`, runs its start-up commands `CONFIG` (prints the channel table) and `AUTODMP ON`. | TO 47, RTC | |
| 10 | Clock: reads date and time from the Hayes clock and prints them. Without a clock: `ENTER DATE:` and `ENTER TIME:`. | EXB 50, 51 | date and time if no clock |
| 11 | Kernel prompt. | TI 46 | `START COS_117 DEADSTART` |
| 12 | MFINIT: master clear the mainframe, load the 184-word test, release, receive one word, read back memory. `MFINIT: COMPLETE`. | COA, CIA, AO/AI to BIOP, MOS, HOA, HIA | |
| 13 | Master clear again. Read `COS_117` from the expander disk in 4,096-byte blocks; pass each through Buffer Memory to the BIOP, which writes it to central memory from address 0 (279,032 words). Then the parameter file `DEADSTART` (69 words) after it, and two words in the exchange package area (0x0C, 0x0F). | EXB disk 60, MOS, AO/AI, HOA | |
| 14 | Release Master Clear: the CPU exchanges to the package at 0 and runs COS start-up. MIOP sends the date/time packet. | COA | |
| 15 | `CPU <-> MIOP CHANNEL INIT`, `I` and `J` packets, `CPU <-> MIOP LINKAGE COMPLETE`, `START COMPLETE`. Concentrator messages. | CIA, COA, 24, 25 | |
| 16 | COS runs: disk requests arrive as `A` packets; the BIOP seeks and reads sectors and writes them to central memory; replies go back as packets. | CIA, COA, AO/AI, DKx, HOA, HIA, MOS | |
| 17 | | TI 46 | `STATION` |
| 18 | The station display appears on the first console. | TO 41, TI 40 | `LOGON` |
| 19 | COS answers with its version line; the `L` flag shows. Message 0: `ENTER CONFIGURATION CHANGES OR 'GO' TO CONTINUE`. | | `STMSG`, then `REPLY,0,GO` |
| 20 | Start-up recovery reads the label and tables of every drive. The Buffer Memory resident device has no label after a cold start; a message asks what to do (number 10 in the run). | DKx, MOS | `REPLY,10,CONTINUE` |
| 21 | `STMSG,I` shows progress; the system is up when the recovery messages end. | | `STMSG,I`, then for example `CLASS,ALL,ON`, `LIMIT,5`, `STATUS` |

Notes:

- The first start on empty disks uses `START COS_117 INSTALL`; it formats the drives and
  takes much longer (art, `a-brave-new-world.md`). The ready-to-run images are already
  installed.
- Typed lines come from `cos-cray1/boot.sh` and were replayed for the log. The run
  used 94 seconds of IOP time from deadstart to the logged-on station, most of it waiting for
  the scripted typing.
- A real restart keeps Buffer Memory and uses `SYSDUMP`/`RESTART` [OPS 2-8/30]; not covered.

For the FPGA the simplest equivalent of step 1 is the simulator's: load `iop_kern.bin` into
Buffer Memory at 0 (with the Buffer Memory patches of section 13 applied or not), keep the
overlay file as tape file 0, hold IOPs 1 to 3 in reset, and give IOP 0 a Buffer Memory
deadstart. TAPELOAD and DISKLOAD are then not needed, and the mechanism by which the
DEADSTART button reads the first tape file into Local Memory, which the manuals read do not
describe (Q2), is not needed either.

## 13. The simulator's patches

Buffer Memory patches (`BufferMemoryPokes` in cfg; the address is a parcel address in the
kernel image, the value a parcel):

| Address | Value | What it changes | Needed at real speed? |
|---|---|---|---|
| 0x42B7 | 0x0200 (EXIT) | The Local Memory test routine returns at once. | No. The test is fast on hardware. Keep the original so the test runs. |
| 0x43DA | 0x0000 | The Buffer Memory test ends quickly (cfg comment); with the patch the log shows one 512-word block read, written and read back at address 0. | No, if Buffer Memory moves about 100 Mbytes/s: three passes over 32 Mbytes take about a second. Patch only if the FPGA's Buffer Memory is slow. |
| 0x4476 | 0x0000 | Skips the loop that clears Local Memory from 0x4D34 up before overlays are loaded; the simulator's memory is already zero. | **Do not apply** unless Local Memory is guaranteed zero at deadstart. |
| 0x3939 | 0x0028 | Device table entry `CK0`: device address 50 octal, which enables the Hayes clock driver (the image has 0: no clock). | Only if a clock is emulated; otherwise the operator types the date and time. |

Tape patches (`Pokes` on file 0 of the tape). In cfg the address of a patch counts units of
its own size, so a `Size 2` patch at 0x04610 is at byte offset 0x8C20 (sim:
`poked_tap_file.cpp`). The table gives cfg's address:

| Offset | Value | What it changes | Needed at real speed? |
|---|---|---|---|
| 0x04610 (parcel; byte 0x8C20) | 0x1001 | Overlay `INDD29`, code parcel 0x6C: `A = 500` becomes `A = 1`, so the disk buffer echo test runs once per drive instead of 500 times. | No. 500 passes over nine drives are a few million IOP instructions and 4,500 pairs of 512-parcel transfers: seconds at real speed. |
| 0x3D24B, 0x3D6A3, 0x3D7C3, 0x3D399, 0x3D49B (bytes) | 0x28 | Overlay `XCLOCK`: use device 50 for the clock's replies and restore the right device address on exit. | Only with the clock patch above. |

Commented out in cfg, so not needed by the simulator either: a patch to end a loop in `XDK`,
patches to `START0` (skip MFINIT, skip a delay), a time-out in `BMXCON`, and four kernel
patches that force waiting on inter-IOP transfers (for the multi-threaded simulator).

Things the simulator does instead of patches, which an FPGA at real speed must handle (sim):

- It holds the source IOP after `AOA : 1` until the target has started (section 3.6).
- All device operations finish instantly: a disk read is done when the function returns, an
  expander function is Done at once, a 512-word memory transfer takes no time. The software
  is written for real delays, so slower is safe; the risks are only where it uses fixed
  delays (the deadstart wait) or time-outs.
- The simulated RTC follows wall-clock time, not instruction count.

## 14. Files of the ready-to-run system

### 14.1 `system/`

| File | Size (bytes) | Format |
|---|---|---|
| `biop_dk20.img` to `biop_dk32.img` (nine: 20, 21, 22, 24, 25, 26, 30, 31, 32) | 606,781,440 each | DD-29 images, section 8.6; installed COS system (`MD-1-20A` is the master device) |
| `exp_disk.img` | 73,740,800 | expander disk, section 10.3 |
| `boot_tape.tap` | 640,240 | section 10.2 |
| `target/cos_117/iop_kern.bin` | 40,960 | IOP kernel, 20,480 parcels high byte first; loaded at Buffer Memory word 0. Identical to the copy in `cray-sim/target/cos_117/`. |
| `target/cos_117/iop_overlay1.bin`, `iop_overlay2.bin` | 622,592 each | two builds of the overlay file; number 1 is on the tape |
| `target/cos_117/tapeboot.bin`, `diskboot.bin` | 4,096 each | TAPELOAD, DISKLOAD |
| `target/cos_117/dmp.bin` | 8,192 | DMP |
| `target/cos_117/disk_content/` | - | sources of the expander disk: COS binaries with headers, parameter files, jobs, utilities |
| `cos_117.cfg` | 9,292 | 2017 version of the configuration (same devices) |
| `pr0.txt` | 794,360 | printer output of an earlier session |
| `bin/`, `sw/` | - | 2017 simulator binaries and source, including `ampex80term` |

`cos-cray1/run/` holds working copies (`exp_disk.img` there is a rebuilt one;
`exp_disk.orig.img` equals the one above).

### 14.2 `Cos1.17DiskImageForCray-1x-mp/`

`cos_117.img`, 67,338,240 bytes: the raw recovered 80 Mbyte disk pack, from archive.org
(2012). The size equals 822 x 5 x 32 sectors of 512 bytes; that geometry is inferred from the
size only. Not bootable as it is; the pieces cut from it (kernel at
0x4F1000, overlays at 0 and 0x4FB000, COS at 0x09A000 and 0x2D0000, parameter files) are
listed in art, `the-hunt-for-the-red-bootcode.md`.

### 14.3 `ios-devices-experiments/`

| File | Content |
|---|---|
| `fnlog_boot.txt.gz` | the function log of the boot to the station (3.0 million lines). `F iop P channel function A-in value busy done` (hex, channel and function octal); `I iop P channel in-reset` for an interrupt taken; `N iop P channel function A` for a function sent to a channel with no device |
| `miop_boot_narrative.txt`, `biop_boot_narrative.txt`, `xiop_boot_narrative.txt` | the same log per IOP, condensed (console text joined, expander operations grouped, Buffer Memory traffic left out, at most 6 of each function shown) |
| `summarize_iop.py` | makes the narratives |
| `ios_fnlog.h` | the logging helper added to the private simulator copy (environment variable `IOS_FNLOG`) |
| `boot2.sh` | boot script with a selectable configuration |
| `console_kernel.raw`, `console_station.raw` | bytes sent to the kernel console and the station |

The simulator copy itself was in a scratch directory and is not kept. The hook is three
`fprintf` calls in `IopCpu_c` (after each channel function, on each interrupt taken, on each
function to a missing channel). For the IOP 3 tests two more changes were made: an IOP marked
`Exists no` ignores a deadstart, and the simulator does not hold the source IOP for it. The
AMAP patch was applied as `Size 2` tape patches at 0x12, 0x25, 0x26, 0x53 and 0x54.

## 15. Open questions, with the best answer available

| # | Question | Best answer |
|---|---|---|
| Q1 | Can IOP 3 be configured out? | Half solved (section 1.3). With five parcels of AMAP zeroed the IOS starts and the station logs on with two IOPs; the MIOP then halts in CDEM (`HALT 020`) when COS sends its tape packet. Still needed: a COS or CDEM patch for that packet. `SM-0043G` (PDF 84 to 96) describes the configuration parameters; the overlay disassembly is in `cray-sim/target/cos_117/disasm/iop_overlay1/` (`CDEM.asm`). Until solved, build three IOPs. |
| Q2 | How does the DEADSTART button load file 0 from tape or disk into Local Memory? | Not in HW, SWI or OPS. Avoid it: deadstart from Buffer Memory as the simulator does. |
| Q3 | Master Clear and XA. | The manual forces XA to 0 [XMP 3-14/64]; the simulator leaves XA alone. COS's package is at 0, so force 0. |
| Q4 | I/O Clear "activates the MCU input channel" [XMP 3-21/71]. | The simulator makes all channels inactive, and the software works: COS sets CL and CA itself. If channel 10 were active with CA = 0 the date packet could be stored at address 0. Leave it inactive and hold the first Ready. |
| Q5 | Does the mainframe input channel end a transfer at CA = CL without a Disconnect? | The manual says yes [XMP B-2/218]; the simulator says no. The software never sends more or less than the buffer, so either works. Implement both conditions. |
| Q6 | The "long delay" in `INDD29` (tape patch 0x04610). | Decoded: it is the pass count (500) of the disk buffer echo test, not a wait for the drive. No patch is needed at real speed. How long 500 passes take on the FPGA should still be measured once. |
| Q7 | Ready Waiting in `CIA : 11`. | The simulator leaves bit 15 set from the first early Ready until `CIA : 4`, and the software accepts 0x8000 in every status. Hardware probably clears it when the waiting parcel is consumed. Either is accepted. |
| Q8 | Console line speed and framing. | Not in the manuals read. Irrelevant for an internal terminal; for a real UART pick 9600 8N1 and treat the data as 7-bit. |
| Q9 | Can channels 24/25 (concentrator), 42 to 45 (spare consoles), 16 (ERA) be left unconnected? | Functions to missing channels are harmless at start, but AMAP lists these devices and their drivers start. Keep stubs: ERA never Done; consoles complete output; concentrator as in 11.2. |
| Q10 | `AOx` Busy polarity and `AIx : 10` with no data. | Manual: Busy from `: 14` until the receiver reads [HW 5-18/58]. The simulator inverts Busy and the kernel does not test it. Build the manual's version, and make every `AIx : 10` set Done on the peer's output channel (the start handshake needs that). |
| Q11 | `PFR : 10` clears Done? | Manual yes [HW 5-10/50], simulator no. The IOS never uses channel 1. |
| Q12 | Buffer Memory size smaller than 4 MW? | The kernel takes the size from AMAP (`MOS SIZE 100K`) and COS places `BMR-0-20` in it; addresses up to 0x316800 were used. Not tested with less. Build 4 MW. |
| Q13 | RTC count format. | Manual: count restarts every millisecond, `RTC : 10` returns 0 to 39,999 [HW 5-12/52]. Simulator: free-running. Only the BIOP's start-up tests read it. Build the manual's version. |
| Q14 | Does anything need device interrupts from the expander with mode bit 0 (channel interrupt)? | Never set in the run; delayed functions are polled. Implement it anyway; it is one AND gate. |
| Q15 | Disk timing. | The driver stacks a head select behind a read and relies on Done interrupts; no status polling. Real seek and rotation delays are not required. Unknown whether zero-delay completion (Done in the same instruction) is safe on hardware where interrupts are taken at once; the simulator does it. A delay of a few hundred microseconds per sector is the cautious choice. |

## Appendix A. Channel functions issued during the boot to the station (log)

Counts per function, `fnlog_boot.txt.gz`. Channels not listed received only the initial
functions 6 and 0.

| IOP | Channel | Functions (octal) and counts |
|---|---|---|
| 0 | 0 IOR | 10: 229,550 |
| 0 | 2 PXS | 7: 1; 10: 344,297; 11: 203,228; 14: 279,092; 15: 103,049 |
| 0 | 3 LME | 0: 2; 6: 1; 7: 1 |
| 0 | 4 RTC | 0: 93,854; 6: 1; 7: 1 |
| 0 | 5 MOS | 0: 29,026; 1, 2, 3: 33,043 each; 4: 21,146; 5: 11,897; 6: 1 |
| 0 | 6 AIA | 0: 1; 6: 1; 7: 1; 10: 4,567 |
| 0 | 7 AOA | 0: 4,568; 1: 2; 6: 1; 7: 1; 14: 4,565 |
| 0 | 12 AIC | 0: 1; 6: 1; 7: 1; 10: 4 |
| 0 | 13 AOC | 0: 5; 1: 2; 6: 1; 7: 1; 14: 2 |
| 0 | 16 ERA | 0: 1; 6: 1; 7: 1 |
| 0 | 17 EXB | 0: 1; 1: 748; 2: 748; 3: 596; 4: 1,344; 5: 2,510; 6: 576; 7: 3,512; 10: 2,092; 11: 1,347; 14: 728; 15: 2,902; 16: 2,902; 17: 1,329 |
| 0 | 20 CIA | 0: 3,632; 1: 3,627; 2: 3,627; 3: 3; 4: 2; 6: 4; 7: 3; 10: 3,624; 11: 3,626 |
| 0 | 21 COA | 0: 3,621; 1: 3,616; 2: 3,616; 3: 4; 4: 7; 6: 4; 7: 3; 10: 3,615; 11: 3,615 |
| 0 | 24 | 0: 6; 1: 2; 2: 2; 3: 3; 6: 5; 7: 2; 10: 1; 11: 2 |
| 0 | 25 | 0: 10; 1: 3; 2: 3; 3: 6; 4: 10; 6: 9; 7: 3; 11: 3 |
| 0 | 40 TIA | 0: 79; 6: 78; 7: 78; 10: 77 |
| 0 | 41 TOA | 0: 4,356; 6: 4,355; 7: 4,593; 14: 4,354 |
| 0 | 46 TID | 0: 34; 6: 33; 7: 33; 10: 32 |
| 0 | 47 TOD | 0: 1,844; 6: 1,841; 7: 1,879; 14: 1,863 |
| 1 | 0 IOR | 10: 208,037 |
| 1 | 2 PXS | 7: 1; 10: 108,493; 11: 108,705; 14: 27,709; 15: 13,854 |
| 1 | 4 RTC | 0: 93,855; 6: 1; 7: 1; 10: 9,108 |
| 1 | 5 MOS | 0: 19,002; 1, 2, 3: 20,161 each; 4: 11,104; 5: 9,057; 6: 1 |
| 1 | 6 AIA, 7 AOA | 10: 4,568; 14: 4,566; 0: 4,568 on channel 7 |
| 1 | 12 AIC, 13 AOC | 10: 4; 14: 3 |
| 1 | 14 HIA | 0: 342; 1, 2, 3, 4: 341 each; 6: 1 |
| 1 | 15 HOA | 0: 7,511; 1, 2, 3, 5: 7,510 each; 6: 1 |
| 1 | 20 DKA | 0: 2,989; 1: 5; 2: 2,938; 3: 12; 4: 188; 5: 34; 6: 5; 7: 5; 10: 2; 11: 534; 14: 2,950; 15: 500 |
| 1 | 21 to 32 | the same functions, fewer (63 to 547 reads per drive) |
| 1 | 43 TOB | 14: 1 |
| 3 | 0, 2, 4 | as the others; RTC 0: 93,832 |
| 3 | 5 MOS | 4: 32; 5: 77 |
| 3 | 6 AIA | 10: 8,926 (a polling loop at start) |
| 3 | 7, 10, 11 | 14: 3 each; 10: 4 on channel 10 |
| 3 | 20 BMA | 0: 5; 1: 2; 5: 1; 6: 5; 7: 3; 10: 32; 11: 32; 12: 22; 13: 6; 14: 32; 15: 32; 16: 18 |
| 3 | 43 TOB | 14: 1 |

Interrupts taken during the boot: on the MIOP from channels 4, 5 (the deadstart), 6, 7, 13,
17, 20, 21, 25, 40, 41, 46, 47; on the BIOP from 4, 5, 6, 7, 12, 13 and the disk channels; on
the XIOP from 4, 5, 6, 7, 10, 11 and 20.

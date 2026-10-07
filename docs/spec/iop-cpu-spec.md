# I/O Processor (IOP) specification: the processor of the Cray I/O Subsystem, as the COS 1.17 IOS kernel needs it

> A working specification, written while the core was built and kept as it was. Manuals are
> named by their file names on [Bitsavers](http://www.bitsavers.org/pdf/cray/). `cray-sim/` is the
> [cray-sim](https://github.com/andrastantos/cray-sim) project, `system/` its ready-to-run
> COS 1.17; `cos-cray1/` and directories of experiments are working material that is not in
> this repository.

Written 2026-10-05 from the Cray manuals and the cray-sim project. Scope: the IOP processor itself, that is
its registers, Local Memory, instruction set, program exit stack, interrupt system, the built-in
channels 0 to 4 (they are part of the processor) and dead start. The peripheral channels, Buffer
Memory, the mainframe channels and the review of the old Verilog are in other notes. Channel 5
(Buffer Memory) and channels 6 to 13 (IOP to IOP) appear here only as far as dead start needs
them.

## 0. Conventions and sources

**Bit numbering.** The IOP manuals number bits from the right as powers of two [B 20, 1-8].
In this note bit n is 2^n, the same as Verilog index `[n]`. A parcel is `[15:0]`.

**Number bases.** Instruction codes (f), I/O function codes and channel numbers are octal, as in
the manuals. Memory addresses taken from the kernel image are hexadecimal with `0x`. Everything
else is decimal.

**Citations.** `[X n, p]` means PDF page n, printed page p of:

| Tag | Document | File |
|---|---|---|
| B | HR-0030 rev B, I/O Subsystem Model B Hardware Reference, May 1986 (236 pages). The authority for this note. | `manuals/ios/HR-0030-...May_1986.OCR.pdf` |
| 1S | HR-0808, CRAY-1 S Hardware Reference, Nov 1981, Part 3 (I/O Subsystem, PDF 222 to 340) and appendix G. Scan; text read from the docling OCR, page numbers from its `page_no`. | `manuals/cray-1/HR-0808_CRAY-1S_HWref_Nov81.pdf`, `manual-ocr/cray-1/HR-0808_CRAY-1S_HWref_Nov81.json` |
| IA | TR-IA, IOS Architecture training manual, July 1987 | `manuals/ios/TR-IA-IOS_Architecture_Software_Training_Manual-July_1987.OCR.pdf` |
| T | T0201D, CRAY-1 S IOS Software Workbook, Sept 1981 | `manuals/ios/T0201D-...September_1981.OCR.pdf` |
| SM | SM-0046 G, IOS Software Internal Reference, Sept 1988 | `manuals/ios/SM-0046-G-...September_1988.OCR.pdf` |
| OG | SG-0051 D, IOS Operations Guide, Oct 1984 | `manuals/ios/SG-0051D-01_IO_Subsystem_[IOS]_Operations_Guide_Oct84.pdf` |
| CD | CSM-1009, IOS Models C and D System Programmer Reference, Apr 1989. A later model; used only where it clarifies wording. | `manuals/ios/CSM-1009-000_...Apr89.pdf` |
| sim | cray-sim source, `cray-sim/simulator/sim_lib/<file>:<line>`. Non-commercial licence: behaviour is described here in my own words, nothing is copied. | |
| cfg | `cray-sim/cos_117.cfg:<line>` | |
| kern | The IOS kernel that COS 1.17 boots with: `cray-sim/target/cos_117/iop_kern.bin` (20480 parcels; byte-identical to `articles/modularcircuits/the-cray-files/the-hunt-for-the-red-bootcode_files/iop_boot.img`), read with the disassembly `cray-sim/target/cos_117/disasm/iop_kern.asm`. `[kern 22F7]` is parcel address 0x22F7. The overlay disassemblies cited in part 8 are in `cray-sim/target/cos_117/disasm/iop_overlay1/`. | |
| AT | Andras Tantos's articles, `articles/modularcircuits/the-cray-files/<file>.md:<line>` | |
| V | Old Verilog, `cray-1x/Verilog/iop/<file>:<line>`. Third reading only. | |

**Instruction pages.** In HR-0030 each instruction 000 to 077 has its own page: instruction f is
on PDF page 63 + f and printed page 6-(3 + f), with f read as a number (077 octal = 63, so PDF
126, printed 6-66). In HR-0808 the same instruction is on PDF page 269 + f, printed Part 3
6-(3 + f). Conditional branches 100 to 137 are on [B 127-128, 6-67 and 6-68] and
[1S 333, Part 3 6-67], the I/O instructions 140 to 177 on [B 129-130, 6-69 and 6-70] and
[1S 334, Part 3 6-68], the summary on
[B 205-208, A-1 to A-4]. The instruction table in part 4 relies on this rule instead of
repeating a page number in every row.

**"sim only".** A statement marked *sim only* has no support in a manual. A statement marked
*kernel* was read from the kernel image and shows what the real software does or needs.

**How far cray-sim can be trusted.** It boots this kernel and COS 1.17. Its author calls the
carry flag "the weakest part" of the IOP model [AT the-matrix.md:104]. Every carry rule in
part 4 was therefore checked against HR-0030 instruction by instruction; cray-sim and the
manual agree on all of them. The places where they do not agree on something else are collected
in part 10.

---

## 1. What matters most

1. The IOP is a 16-bit accumulator machine: A (16 bits) with a carry bit C that acts as bit 16
   of A in add, subtract and shift; B (9 bits); 512 operand registers; a 16-entry program exit
   stack addressed by E (4 bits); P (16 bits); one mode bit, the System Interrupt Enable flag I.
   There is no user or monitor mode, no memory protection and no halt instruction.
2. Add and subtract **toggle** C on a carry out of bit 15; they do not set it. A load or a
   logical product clears C. This is the rule the kernel's interrupt handler depends on to save
   and restore C [kern 22F7-22FD, 2332-2338].
3. Interrupts are taken between instructions. The hardware saves only P, on the exit stack, and
   clears I. The handler address is exit stack entry 0. Return is `I = 1` followed by `EXIT`;
   `I = 1` takes effect late so that the pair cannot be interrupted.
4. Dead start is an interrupt: the Buffer Memory channel copies 65,536 parcels into Local
   Memory, its Done flag raises an interrupt, and the interrupt sequence loads P from exit stack
   entry 0, which Master Clear set to 0.
5. The kernel jumps through operand register 0, which holds 0, all the time (`P = OR[0] + k`).
   Each such jump sets the Program Fetch Request flag. That flag must not interrupt unless the
   program has enabled channel 1, and the kernel never does (part 6.3).
6. The kernel enables the exit stack boundary interrupt and has no handler for it, so a boundary
   flag that sets when the manual says it should not is a fatal halt [kern 4A72, table at 0D2B].
7. The software contains delay loops and counted waits that assume real instruction and device
   times. The two that can break a boot are the MIOP's fixed wait while another IOP dead
   starts and the 400-iteration wait for the Peripheral Expander's Busy flag (part 8.3).

---

## 2. Registers and state

### 2.1 Program-visible state

| Name | Width | Purpose | Source |
|---|---|---|---|
| A, accumulator | 16 | One operand and the result of every operation; path between memory, operand registers and channels. "16-bit signed register". | [B 19, 1-7] [B 39, 4-3] |
| C, carry bit | 1 | Bit 2^16 of the accumulator for add, subtract and shift; branch criterion; receives a channel Busy or Done flag. | [B 19, 1-7] [B 40, 4-4] |
| B | 9 | Selects an operand register or a channel, or is an operand (zero-extended to 16 bits). Loaded from A bits 8 to 0. | [B 30, 3-6] [IA 36, 2-10] |
| Operand registers OR[0..511] | 16 each | Temporaries, index registers and the only way to address memory. Written only from A. | [B 37, 4-1] [IA 42, 2-16] |
| P | 16 | Address of the instruction awaiting issue. | [B 31, 3-7] |
| Exit stack XS[0..15] | 16 each | Entry 0: interrupt handler address. Entries 1 to 14: return addresses. Entry 15: address interrupted by the boundary interrupt. | [B 31-32, 3-7 and 3-8] |
| E | 4 | Points at the most recent exit stack entry. | [B 31, 3-7] [IA 40, 2-14] |
| I, System Interrupt Enable | 1 | Set by 003, cleared by 002 and by the interrupt sequence. | [B 59, 5-19] |
| Delayed-enable pending | 1 | Set by 003; turns into I = 1 later (part 7.3). Not named in the manuals; the HR-0808 timing line calls it "delay interrupt enable flag". | [1S 272, Part 3 6-6] |
| PFR flag, PFR register (9 bits), channel 1 interrupt enable | 1, 9, 1 | Program Fetch Request (part 6.3). | [B 35, 3-11] [B 49-50, 5-9 and 5-10] |
| Exit Stack Boundary flag, channel 2 interrupt enable | 1, 1 | Part 5. | [B 50, 5-10] |
| Local Memory Parity Error flag, error register (5 bits), channel 3 interrupt enable | 1, 5, 1 | Part 6.5. | [B 51-52, 5-11 and 5-12] |
| Real-time clock counter, Done flag, channel 4 interrupt enable | 17, 1, 1 | Part 6.6. | [B 52, 5-12] |

Tantos's article says E "points to the first empty slot" [AT the-return-of-the-cray.md:128].
The manuals and his simulator both increment E before storing [B 59, 5-19]
[sim cray_iop.h:339-346], so E points at the newest entry.

### 2.2 Internal registers (not visible to a program except through timing)

Instruction stack (32 parcels, two banks of 16, filled in bursts of 4 from addresses that are
multiples of 4) [B 27-28, 3-3 and 3-4]; Instruction Issue register II (16 bits) [B 29, 3-5];
Register Pointer RP and Destination Pointer DP (9 bits each) [B 30, 3-6]; Memory Address
register MA (16 bits) [B 38, 4-2]; Addend register (16 bits) [B 40, 4-4]; a background
accumulator for branch address arithmetic, which is why branches leave A and C alone
[B 39, 4-3].

### 2.3 Values after Master Clear and dead start

The manuals give only these facts:

- Master Clear clears exit stack entry 0 to zero [B 51, 5-11].
- "On the deadstart interrupt, the P register is set to 0 and program execution begins at
  memory location 0. The exit stack pointer, E, is set to the current value plus 1"
  [B 51, 5-11]. The 1981 text adds "wrapping around to 0 if the current value is 15"
  [1S 257, Part 3 5-11]. So E is not said to be cleared; it is advanced by the interrupt.
- A Master Clear signal goes to every channel interface when the IOP is dead started
  [B 44, 5-4].
- The first instruction after a dead start must not be an I/O instruction (040-043, 140-147,
  154-157, 160-167, 174-177) because A must be loaded first [B 211, B-3].

cray-sim (*sim only* for the rest of this table) and the recommended FPGA reset value:

| State | cray-sim after Master Clear | Recommended |
|---|---|---|
| P | 0 [sim cray_iop.cpp:768] | 0 |
| I | 1, so that the dead start interrupt is accepted; the interrupt then clears it [sim cray_iop.cpp:770, 836] | same |
| Delayed-enable pending | 0 [sim cray_iop.cpp:771] | 0 |
| E | 0, then 1 after the dead start interrupt [sim cray_iop.h:324, 341] | same |
| XS[0..15] | all 0 [sim cray_iop.h:324] | all 0 (the manual requires XS[0] = 0) |
| Boundary flag, channel 2 interrupt enable | 0, 0 [sim cray_iop.h:324] | 0, 0 |
| PFR flag, channel 1 interrupt enable | 0, 0 [sim cray_iop.h:301] | 0, 0. **Channel 1 enable must be 0**: the kernel never touches channel 1 and sets the flag in the fourth instruction it executes (part 6.3). |
| RTC Done, channel 4 interrupt enable | 0, 0 [sim cray_iop.h:409] | 0, 0 |
| A, C, B, operand registers | not changed by Master Clear; all 0 when the simulator starts [sim cray_iop.cpp:767-776, cray_iop.h:582-596] | 0 at power-up, kept across Master Clear |
| Processor | "in reset": executes nothing until the first interrupt is taken [sim cray_iop.cpp:817-842, cray_iop.h:596] | same |

*Kernel* evidence that operand registers survive a dead start on the real machine: the first
thing the kernel does after its entry jump is to copy OR[511] to memory location 0x000A, before
it clears registers 1 to 511 [kern 41E4-41EF].

---

## 3. Local Memory

- 65,536 parcels of 16 bits; a 16-bit address selects one parcel [B 18, 1-6] [B 21, 2-1]
  [B 22, 2-2]. There is no relocation, no limit register and no protection.
- Address bits 1 to 0 select the bank within a section, bits 3 to 2 the section, bits 15 to 4
  the chip and the address within it [B 22-23, 2-2 and 2-3]. Sixteen banks of 4096 parcels, in
  four independent sections [B 21, 2-1]. This interleaving matters only for timing: sequential
  addresses fall in different banks, so one parcel can be fetched every clock period.
- Protection is by three voted copies and two parity bits per parcel [B 23, 2-3]; the 1981
  machine had parity only [1S 229, Part 3 2-3]. Errors are reported on channel 3. An FPGA
  memory has no errors to report.
- The computation section can reach memory only through an operand register that holds the
  address [B 37, 4-1].
- Timing: operand read to A in 7 clock periods; section cycle 4 clock periods (6 for an I/O
  write) [B 21, 2-1].

**Instruction layout.** A parcel holds f in bits 15 to 9 (7 bits) and d in bits 8 to 0 (9 bits).
A 2-parcel instruction has a 16-bit constant k in the parcel that follows; the next instruction
is in the parcel after k [B 27, 3-3] [B 61-62, 6-1 and 6-2] [B 75, 6-15]. Instructions start
at any parcel address; there is no alignment. The 2-parcel instructions are 014 to 017, 075,
077 and the conditional forms of 075 and 077: 124 to 127 and 134 to 137. d is always an
unsigned 9-bit number; add and subtract of d, and forward and backward branches, are separate
instructions [B 62, 6-2].

cray-sim reads k from (P + 1) modulo 65,536 and P wraps modulo 65,536
[sim cray_iop.cpp:854-856, 891] (*sim only*; the manuals do not discuss wrap).

Instructions that do not use d must ignore it. The kernel places halt codes in the d field of
a PASS that follows a conditional call to its fatal-error routine; when the call is not taken
the PASS executes [kern 2303-2304, 231E-231F].

**Image byte order.** In `iop_kern.bin` each parcel is stored high byte first; parcel 0 is
0x7003. When cray-sim copies between Buffer Memory and Local Memory, Buffer Memory word w holds
Local Memory parcels 4w to 4w+3 with parcel 4w in the most significant 16 bits
[sim cray_iop_built_in_channels.cpp:222, 234, utils.h:98-105] (*sim only*; the manual gives the
word-to-parcel grouping but not the order).

---

## 4. Instruction set

### 4.1 Definitions

Let CA be the 17-bit value {C, A}.

| Name | Effect |
|---|---|
| LOAD(x) | A = x, C = 0 |
| AND(x) | A = A and x, C = 0 |
| ADD(x) | CA = (CA + x) mod 2^17, with x a 16-bit unsigned value. A becomes the 16-bit sum; C is **complemented** if the 16-bit addition carried out of bit 15. |
| SUB(x) | CA = (CA + (not x and 0xFFFF) + 1) mod 2^17. A becomes A - x mod 2^16; C is complemented if A >= x as unsigned numbers (no borrow), including x = 0. |
| SHR(n), SHL(n) | End-off shift of CA by n places, zero fill. n >= 17 gives CA = 0. |
| ROR(n), ROL(n) | Circular shift of the 17 bits of CA. n = 17 leaves CA unchanged. |

Sources. Adder: 17-bit operand from the accumulator and carry, 16-bit operand from the Addend
register [B 38, 4-2]. "The instruction complements the Carry flag if a carry is propagated"
[B 73, 6-13]. Subtraction is complement, add, then add 1, with the carry complemented if
either addition carries [B 74, 6-14]; at most one of the two can carry, so the single 17-bit
sum above is the same thing. "For unsigned 16-bit arithmetic, the carry bit is toggled on x - y
if x > y or x = y" [B 38, 4-2]; worked examples in [IA 48-49, 2-22 and 2-23]. (CSM-1009 says
"toggles if the result is negative or 0" [CD 35, 2-14]; that contradicts HR-0030, the
training manual and cray-sim and is taken to be an error.) Loads clear C: stated per instruction in
HR-0030, and in general in [IA 44, 2-18] and [CD 37, 2-16]. Shifter: 17 bits, 5-bit count from
the Addend register, "If the count is greater than 16 for an end-off shift, the zero-filling
clears the result" [B 39, 4-3]. cray-sim implements ADD and SUB exactly as above
[sim cray_iop.cpp:753-765].

Shift counts. The manual says the low-order 5 bits of the Addend register give the count and
the high-order bits are ignored [B 67, 6-7], so the count is d mod 32 or B mod 32. It does not
say what a circular shift does for counts 18 to 31; a true 17-bit rotation would use the count
mod 17. cray-sim differs from that reading in two places (*sim only*): for end-off shifts it
uses the whole 9-bit value, so a d or B of 32 or more always clears CA; and its circular left
shift zero-fills the low bits for counts 18 to 31 [sim cray_iop.cpp:726-751]. For counts 0 to
17 all readings agree. A linear sweep of the kernel image finds end-off counts 1 to 16,
circular right counts 2 to 12 and 17 and circular left counts 1 to 7; the few larger values it
reports sit in data tables. So the difference is not exercised by fixed-count shifts in the
kernel. Counts taken from B (044 to 047) cannot be checked statically.

### 4.2 Decoding

| f (octal) | Group |
|---|---|
| 000-003 | Control: PASS, EXIT, I = 0, I = 1 |
| 004-007 | Shift CA by d |
| 010-013, 014-017 | A with d; A with k (2 parcels) |
| 020-027 | A with operand register d |
| 030-037 | A with memory addressed by operand register d |
| 040-043 | Channel flag to C |
| 044-047 | Shift CA by B |
| 050-057 | A with B |
| 060-067 | A with operand register B |
| 070-077 | Unconditional branches, mode = f bits 2 to 0 |
| 100-137 | Conditional branches, mode = f bits 4 to 2, condition = f bits 1 to 0 |
| 140-157 | I/O, channel d, function = f bits 3 to 0 |
| 160-177 | I/O, channel B, function = f bits 3 to 0 |

In the groups 010, 020, 030, 050 and 060 the low three bits of f mean: 0 load, 1 logical
product, 2 add, 3 subtract, 4 store, 5 add and store, 6 increment, 7 decrement (in group 010,
4 to 7 repeat 0 to 3 with k).

### 4.3 Table

"P" is the number of parcels. "CP" is the clock period, counted from issue at CP 0, in which
HR-0030 delivers the result; it is given only as timing background (part 8). "OR[d]" is the
operand register selected by d, "OR[B]" the one selected by B, "M[x]" the Local Memory parcel
at address x. Every instruction leaves A, C, B, operand registers and memory unchanged unless
the row says otherwise.

| f | APML | P | Operation | C | CP |
|---|---|---|---|---|---|
| 000 | PASS | 1 | Nothing. d ignored. | - | 0 |
| 001 | EXIT | 1 | P = XS[E]; then E = E - 1, except that when E was 0 the decrement is blocked and the Exit Stack Boundary flag is set (part 5). | - | 2 |
| 002 | I = 0 | 1 | I = 0; cancels a pending delayed enable. | - | 1 |
| 003 | I = 1 | 1 | Sets the delayed-enable pending flag (part 7.3). | - | 2 |
| 004 | A = A > d | 1 | SHR(d): C moves toward bit 0, zeros enter at C. | shifted | 3 |
| 005 | A = A < d | 1 | SHL(d): bits leave through C and are lost, zeros enter at bit 0. | shifted | 3 |
| 006 | A = A >> d | 1 | ROR(d): bit 0 of A returns to C. | shifted | 3 |
| 007 | A = A << d | 1 | ROL(d): C returns to bit 0 of A. | shifted | 3 |
| 010 | A = d | 1 | LOAD(d) | 0 | 1 |
| 011 | A = A & d | 1 | AND(d); clears A bits 15 to 9. | 0 | 1 |
| 012 | A = A + d | 1 | ADD(d) | toggle on carry | 3 |
| 013 | A = A - d | 1 | SUB(d) | toggle if A >= d | 3 |
| 014 | A = k | 2 | LOAD(k) | 0 | 2 |
| 015 | A = A & k | 2 | AND(k) | 0 | 2 |
| 016 | A = A + k | 2 | ADD(k) | toggle on carry | 4 |
| 017 | A = A - k | 2 | SUB(k) | toggle if A >= k | 4 |
| 020 | A = dd | 1 | LOAD(OR[d]) | 0 | 2 |
| 021 | A = A & dd | 1 | AND(OR[d]) | 0 | 2 |
| 022 | A = A + dd | 1 | ADD(OR[d]) | toggle on carry | 4 |
| 023 | A = A - dd | 1 | SUB(OR[d]) | toggle if A >= OR[d] | 4 |
| 024 | dd = A | 1 | OR[d] = A | - | 2 |
| 025 | dd = A + dd | 1 | ADD(OR[d]); OR[d] = A | toggle on carry | 5 |
| 026 | dd = dd + 1 | 1 | C = 0; A = 1; ADD(OR[d]); OR[d] = A. Result also in A. | 1 only if OR[d] was 0xFFFF | 5 |
| 027 | dd = dd - 1 | 1 | C = 0; A = 0xFFFF; ADD(OR[d]); OR[d] = A. Result also in A. | 1 if OR[d] was not 0, else 0 | 5 |
| 030 | A = (dd) | 1 | LOAD(M[OR[d]]) | 0 | 7 |
| 031 | A = A & (dd) | 1 | AND(M[OR[d]]) | 0 | 7 |
| 032 | A = A + (dd) | 1 | ADD(M[OR[d]]) | toggle on carry | 9 |
| 033 | A = A - (dd) | 1 | SUB(M[OR[d]]) | toggle if A >= operand | 9 |
| 034 | (dd) = A | 1 | M[OR[d]] = A | - | 6 |
| 035 | (dd) = A + (dd) | 1 | ADD(M[OR[d]]); M[OR[d]] = A. Result also in A. | toggle on carry | 13 |
| 036 | (dd) = (dd) + 1 | 1 | C = 0; A = 1; ADD(M[OR[d]]); M[OR[d]] = A. Result also in A. | 1 only if the parcel was 0xFFFF | 13 |
| 037 | (dd) = (dd) - 1 | 1 | C = 0; A = 0xFFFF; ADD(M[OR[d]]); M[OR[d]] = A. Result also in A. | 1 if the parcel was not 0 | 13 |
| 040 | C = 1, iod = DN | 1 | C = Done flag of channel d | forced | 5 |
| 041 | C = 1, iod = BZ | 1 | C = Busy flag of channel d | forced | 5 |
| 042 | C = 1, IOB = DN | 1 | C = Done flag of channel B. d ignored. | forced | 5 |
| 043 | C = 1, IOB = BZ | 1 | C = Busy flag of channel B. d ignored. | forced | 5 |
| 044 | A = A > B | 1 | SHR(B) | shifted | 3 |
| 045 | A = A < B | 1 | SHL(B) | shifted | 3 |
| 046 | A = A >> B | 1 | ROR(B) | shifted | 3 |
| 047 | A = A << B | 1 | ROL(B) | shifted | 3 |
| 050 | A = B | 1 | LOAD(B), B zero-extended | 0 | 1 |
| 051 | A = A & B | 1 | AND(B) | 0 | 1 |
| 052 | A = A + B | 1 | ADD(B) | toggle on carry | 3 |
| 053 | A = A - B | 1 | SUB(B) | toggle if A >= B | 3 |
| 054 | B = A | 1 | B = A bits 8 to 0 | - | 3 |
| 055 | B = A + B | 1 | ADD(B); B = A bits 8 to 0. Sum also in A. | toggle on carry | 4 |
| 056 | B = B + 1 | 1 | C = 0; A = 1; ADD(B); B = A bits 8 to 0. B = 511 gives A = 512, B = 0. | 0 | 4 |
| 057 | B = B - 1 | 1 | C = 0; A = 0xFFFF; ADD(B); B = A bits 8 to 0. B = 0 gives A = 0xFFFF, B = 511. | 1 if B was not 0 | 4 |
| 060 | A = (B) | 1 | LOAD(OR[B]) | 0 | 2 |
| 061 | A = A & (B) | 1 | AND(OR[B]) | 0 | 2 |
| 062 | A = A + (B) | 1 | ADD(OR[B]) | toggle on carry | 4 |
| 063 | A = A - (B) | 1 | SUB(OR[B]) | toggle if A >= OR[B] | 4 |
| 064 | (B) = A | 1 | OR[B] = A | - | 2 |
| 065 | (B) = A + (B) | 1 | ADD(OR[B]); OR[B] = A | toggle on carry | 5 |
| 066 | (B) = (B) + 1 | 1 | C = 0; A = 1; ADD(OR[B]); OR[B] = A | 1 only if OR[B] was 0xFFFF | 5 |
| 067 | (B) = (B) - 1 | 1 | C = 0; A = 0xFFFF; ADD(OR[B]); OR[B] = A | 1 if OR[B] was not 0 | 5 |
| 070 | P = P + d | 1 | P = P + d | - | 5 in stack |
| 071 | P = P - d | 1 | P = P - d | - | 5 in stack |
| 072 | R = P + d | 1 | Push P + 1; P = P + d | - | 5 in stack |
| 073 | R = P - d | 1 | Push P + 1; P = P - d | - | 5 in stack |
| 074 | P = dd | 1 | If OR[d] = 0 set the PFR flag and PFR register = d. P = OR[d] | - | 9 or more |
| 075 | P = dd + k | 2 | PFR as 074. P = OR[d] + k | - | 9 or more |
| 076 | R = dd | 1 | PFR as 074. Push P + 1; P = OR[d] | - | 9 or more |
| 077 | R = dd + k | 2 | PFR as 074. Push P + 2; P = OR[d] + k | - | 9 or more |
| 100-137 | (branch), C = 0 / C # 0 / A = 0 / A # 0 | 1 or 2 | The eight modes of 070 to 077 under a condition (part 4.4). Not taken: continue with the next instruction, skipping k if the mode has one. | - | as unconditional |
| 140-147 | iod : 0 to 7 | 1 | Send function 0 to 7 and A to channel d | - | not given |
| 150-153 | iod : 10 to 13 | 1 | Send the function to channel d; A = the 16 bits the interface returns | 0 | not given |
| 154-157 | iod : 14 to 17 | 1 | Send function 14 to 17 and A to channel d | - | not given |
| 160-177 | IOB : 0 to 17 | 1 | The same sixteen functions on channel B. d ignored. | as 140-157 | not given |

Notes on the table.

- In all "P + d", "P - d", "P + 1" and "P + 2" expressions P is the address of the first
  parcel of the branch instruction itself. All address arithmetic is modulo 2^16.
- 026, 027, 036, 037, 056, 057, 066, 067 leave the result in A. The kernel tests A after
  `OR[x] = OR[x] - 1` and `OR[x] = OR[x] + 1` to end loops [kern 2365-2366, 3F91-3F92].
- 027 and 057: the manual computes "all ones plus operand" with C first cleared [B 86, 6-26]
  [B 110, 6-50], so C ends as 1 whenever the operand was not zero. This is not a borrow flag.
- 056: the kernel clears operand registers 1 to 511 with `B = B + 1` in a loop that ends when
  `A = B` reads zero, so B must wrap from 511 to 0 [kern 41E8-41EF].
- The clearing of C by the input functions 10 to 13 is not stated in section 6 of HR-0030. It
  is stated for PXS : 10 and PXS : 11 in [IA 78, 3-12] and [T 76, 4.8], follows from "the carry bit
  register clears each time the accumulator loads" [CD 37, 2-16], and is what cray-sim does
  [sim cray_iop.cpp:1145, 1179].
- Timing column: HR-0808 counts one clock period less for most instructions because it puts
  the first transfer in CP 0 with the issue, for example 004 completes at CP 2 instead of CP 3
  [1S 273, Part 3 6-7] and 040 at CP 3 instead of CP 5 [1S 301, Part 3 6-35]. For 035 to 037
  the accumulator is free for the next instruction at CP 11 and the memory write completes at
  CP 13 [B 92, 6-32].

### 4.4 Conditional branches

f = 100 + 4 x mode + condition (octal arithmetic: 100, 104, 110, ... 134 are condition 0 of
modes 0 to 7).

| Mode (f bits 4 to 2) | Same as | Condition (f bits 1 to 0) | Taken when |
|---|---|---|---|
| 0 | 070 P = P + d | 0 | C = 0 |
| 1 | 071 P = P - d | 1 | C = 1 |
| 2 | 072 R = P + d | 2 | A = 0 (all 16 bits) |
| 3 | 073 R = P - d | 3 | A is not 0 |
| 4 | 074 P = dd | | |
| 5 | 075 P = dd + k | | |
| 6 | 076 R = dd | | |
| 7 | 077 R = dd + k | | |

Source: [B 127-128, 6-67 and 6-68]. A and C are never changed by a branch [B 39, 4-3]. cray-sim
decodes the same way and, when a 2-parcel conditional branch is not taken, advances P by 2
[sim cray_iop.cpp:919-951, 1054, 1098].

---

## 5. Branch addressing and the program exit stack

### 5.1 Addressing

- Relative branches (modes 0 to 3) add or subtract the unsigned 9-bit d to the address of the
  branch instruction, in 16-bit two's complement arithmetic [B 119, 6-59] [B 120, 6-60]. The
  reach is 511 parcels each way. The kernel uses backward return jumps of more than 255
  parcels, for example d = 263 [kern 4A0B].
- Absolute branches (modes 4 to 7) take the address from operand register d, plus k in modes
  5 and 7 [B 123-126, 6-63 to 6-66].
- A return jump stores the address of the next sequential instruction: P + 1 for 072, 073, 076
  and their conditional forms, P + 2 for 077 and its conditional forms [B 121, 6-61]
  [B 126, 6-66].
- On the real machine relative branches of at most 9 parcels forward or 11 parcels back stay
  inside the instruction stack and cost 5 clock periods; absolute branches always refill the
  stack from memory [B 29, 3-5] [IA 32, 2-6]. This is timing only.

### 5.2 Push (return jump or interrupt)

E = E + 1 modulo 16, then XS[E] = return address [B 59, 5-19] [B 121, 6-61].

- If a **return jump** advances E to 14, the Exit Stack Boundary flag is set [B 125, 6-65]
  [B 32-33, 3-8 and 3-9]. The return jump still completes: the return address is in entry 14
  and P holds the subroutine address. If the boundary interrupt is enabled it is then taken
  before the first instruction of the subroutine, which puts that address in entry 15
  [B 33, 3-9].
- If an **interrupt** advances E from 13 to 14 the flag is **not** set [B 34, 3-10, NOTE].
- If E is 15 at a push it wraps to 0 and the handler address in entry 0 is overwritten
  [B 33, 3-9] [B 209, B-1].

HR-0030 states the boundary rule only in the descriptions of 076 and 077; the general
description [B 32-33] speaks of "a subroutine call", so it is applied here to 072 and 073 and
to all conditional return jumps as well.

### 5.3 EXIT

P = XS[E]. Then E = E - 1. If E was 0 the decrement is blocked and the Exit Stack Boundary
flag is set [B 64, 6-4]. With E = 0 the address loaded is entry 0, the interrupt handler:
"When the E pointer reaches 0 and an exit instruction issues, an interrupt sets and the
program jumps to the interrupt handler routine" [B 33, 3-9], and the handler then finds E = 0
with the boundary flag present [B 34, 3-10].

*Kernel:* the interrupt handler reads E on entry and treats E = 0 as fatal
[kern 22FE-2304]. That fits the manual: an EXIT at E = 0 arrives in the handler without a push.

### 5.4 Access through channel 2

PXS : 10 reads E, PXS : 11 reads XS[E], PXS : 14 sets E from A bits 3 to 0, PXS : 15 writes
A to XS[E] (part 6.4). The hardware needs 4 clock periods for such a transfer, and the manual
demands 5 clock periods before an EXIT, a return jump, enabling interrupts, or using a value
just read [B 34, 3-10] [B 51, 5-11] [B 209, B-1]. The recommended delay is three circular
shifts of 17 places, which leave CA unchanged [B 34, 3-10]. The 1981 manual asked for 4 clock
periods and suggested reading the value back instead [1S 240, Part 3 3-10] [1S 257].

*Kernel:* all 5 PXS : 15 and 18 of the 19 PXS : 14 in the image are followed directly by
three `A = A >> 17` [kern 41F1-41F4, 41F8-41FB, 4205-4208]; the nineteenth has two other
instructions before the stack is used [kern 2272-2275]. The shifts also follow some reads, with
the value read still in A: the handler reads E, executes the three shifts and then tests A, so
ROR(17) must preserve A and C exactly [kern 22FE-2303]. Other reads are used by the very next
instruction, a conditional branch or an add [kern 41F5-41F6, 41FC-41FD], so an input function
must have delivered A, and cleared C, by the time the next instruction executes. If an
implementation makes channel 2 functions take effect at once, the shifts are harmless.

*Kernel:* at start-up the kernel sets E = 1, then repeats "write 0 to XS[E]; E = E + 1" until
PXS : 10 reads 0, and then writes the handler address 0x22F7 to XS[0] [kern 41F0-4208]. The
loop ends only if PXS : 14 keeps 4 bits and PXS : 10 returns 0 in the upper 12 bits.

### 5.5 What the software does with the boundary interrupt

The manual intends the boundary interrupt for software that spills the stack to memory
[B 33-34, 3-9 and 3-10]. The training manual says "Our Operating System software does not
restructure the Exit Stack" [IA 78, 3-12]. *Kernel:* it enables the channel 2 interrupt
[kern 4A72] but the handler table entry for channel 2 is zero, which the dispatcher turns into
a fatal halt [kern table at 0D2B, 2320-2328]. The kernel reads and writes E and the stack
entries itself in many places (50 channel 2 data functions in the image), which is how it
switches activities and abandons callers [kern 36DA-36E0]. So a boundary flag that sets where
the manual says it should not would halt the kernel; see part 10 for cray-sim's looser rule.

---

## 6. Input and output instructions, and the channels built into the processor

### 6.1 Encoding

- Channel number: d for 040, 041 and 140 to 157; B for 042, 043 and 160 to 177 [B 129, 6-69].
  Both are 9 bits. Model B has 40 channels, 0 to 47 [B 41, 5-1]; later documents and the
  software use 42, 0 to 51 [IA 68, 3-2] [cfg 84]. *Kernel:* the interrupt dispatcher rejects a
  channel number of 42 or more with a fatal halt [kern 231C-231F].
- Function: the low 4 bits of f, 0 to 17, sent to the interface with a strobe [B 43, 5-3].
- Data out: A is presented to the interface with every function and is valid only in the
  strobe clock period [B 43, 5-3]. Whether it is used depends on the interface.
- Data in: functions 10 to 13 (instructions 150 to 153 and 170 to 173) replace A with 16 bits
  from the interface [B 129, 6-69] and clear C (note in part 4.3). Functions 0 to 7 and 14 to
  17 leave A and C unchanged.
- The processor never waits for a channel: "There is no mechanism for the I/O channel control
  to delay execution of further instructions" [B 129, 6-69]. A program waits by testing Busy
  and Done or by interrupt.
- Functions common to nearly all interfaces: 0 clears Busy and Done and idles the channel,
  6 clears the Channel Interrupt Enable flag, 7 sets it [B 131, 7-1]. Conventionally 10 to 13
  read and 14 to 17 write.
- Programming rules the real hardware imposes: allow 1 clock period after any function before
  testing Busy or Done, 1 clock period after function 6 or 7 before IOR : 10, and 3 clock
  periods before a function 6 or 7 affects the system interrupt [B 129, 6-69] [B 210, B-2].
  An implementation in which channel functions act immediately satisfies all of them.

### 6.2 Busy and Done tests

040 to 043 copy one flag to C [B 95-98, 6-35 to 6-38]. Each interface has a Busy and a Done
flag [B 42, 5-2]. Channel 0 is always Done and never Busy, so `C = 1, IOR = DN` sets C and
`C = 1, IOR = BZ` clears it [B 49, 5-9]. *Kernel:* `C = 1, IOR = BZ` is its usual way to clear
C, for example [kern 0375, 03D8].

The manuals do not say what an unconnected channel returns. *sim only:* 042 and 043 on a
channel with no device give C = 0; 040 and 041 on such a channel stop the simulator; functions
0 and 6 to such a channel are ignored and any other function is an error
[sim cray_iop.cpp:1136-1138, 1163-1172, 1511, 1522, 1539-1542, 1561-1564]. *Kernel:* at
start-up it issues function 6
and then function 0 to every channel from 3 to 39 decimal whether a device is present or not
[kern 4246-424E], so those two functions must be harmless on an empty channel.

### 6.3 Channel 0, interrupt request (IOR), and channel 1, program fetch request (PFR)

| Function | Effect | Source |
|---|---|---|
| IOR : 10 | A = number of the highest-priority channel that is requesting an interrupt, 0 if none. "Loaded into the low-order 9 bits ... Only the 6 least significant bits are used"; upper bits 0. | [B 49, 5-9] [T 74, 4.6] |
| PFR : 0 | Clear the PFR flag (the channel's Done flag; there is no Busy flag) | [B 49, 5-9] |
| PFR : 6, PFR : 7 | Clear, set the channel 1 interrupt enable | [B 49, 5-9] |
| PFR : 10 | A = PFR register (9 bits, upper bits 0); "The Done flag is cleared" | [B 50, 5-10] |

A channel requests an interrupt when its Done flag and its Channel Interrupt Enable flag are
both set [B 44, 5-4] [B 132, 7-2]. Priority is by channel number, lowest first: "Channel 0,
Channel 1, and Channels 2 through 47 in descending priority" [B 41, 5-1]; "highest priority
(lowest number)" [IA 74, 3-8]. The value changes when the channel's Done flag or enable flag is
cleared [B 49, 5-9]. IOR : 10 works whether or not I is set [B 132, 7-2]. cray-sim scans
upward from channel 0 and returns the first channel whose Done and enable are both set
[sim cray_iop_built_in_channels.cpp:17-31]. (HR-0808 says only "four least significant bits"
[1S 255, Part 3 5-9]; with channels up to 47 that cannot be right.)

The PFR flag is set during 074 to 077 and 120 to 137 "when the instruction sequence finds a
zero value in operand register d"; the PFR register receives d [B 35, 3-11] [B 49, 5-9]. The
idea was demand loading of code, one operand register per overlay [IA 76-77, 3-10 and 3-11].
"This feature is not utilized by our Operating System software" [IA 76, 3-10].

*Kernel:* this is true and has a consequence. Operand register 0 is kept at 0 and used as the
base of every absolute jump and call in the kernel (`P = OR[0] + k`, `R = OR[0] + k`; a
linear sweep of the image finds 496 absolute branches with d = 0). The reset vector does it in
the fourth instruction executed [kern 0000-0005]. Every
one of these sets the PFR flag, and the jump must still go to 0 + k. The kernel image contains
no parcel that would be an I/O instruction or a flag test on channel 1 by d, and its start-up
loop over the channels begins at 3 [kern 4246-424E]. So the flag stays set for good, and the machine works only because the
channel 1 interrupt enable is off after Master Clear and IOR : 10 does not report a channel
that is not enabled. cray-sim behaves this way [sim cray_iop.h:296-301,
cray_iop.cpp:1026-1034].

### 6.4 Channel 2, program exit stack (PXS)

| Function | Effect | Source |
|---|---|---|
| PXS : 0 | Clear the Exit Stack Boundary flag (the channel's Done flag; no Busy flag) | [B 50, 5-10] |
| PXS : 6, PXS : 7 | Clear, set the channel 2 interrupt enable; the flag is not changed | [B 50, 5-10] |
| PXS : 10 | A = E in bits 3 to 0, upper bits 0; C = 0 | [B 50, 5-10] [IA 78, 3-12] |
| PXS : 11 | A = XS[E]; C = 0 | [B 50, 5-10] [IA 78, 3-12] |
| PXS : 14 | E = A bits 3 to 0 | [B 50, 5-10] |
| PXS : 15 | XS[E] = A | [B 50, 5-10] |

cray-sim implements exactly these [sim cray_iop_built_in_channels.cpp:52-76]. Functions 13 and
16 exist only on the later Model C [IA 78, 3-12].

### 6.5 Channel 3, Local Memory error (LME)

LME : 0 clears the parity error flag, LME : 6 and 7 clear and set the interrupt enable,
LME : 10 reads bank (bits 1 to 0), section (bits 3 to 2) and byte (bit 4) of the error into A
[B 52, 5-12]. cray-sim never raises the flag and returns 0
[sim cray_iop.h:378-393, cray_iop_built_in_channels.cpp:103-117]. *Kernel:* it issues LME : 0
and LME : 7 at start-up [kern 4A73-4A74]; its LME handler reads every memory parcel and tests
the flag after each read [kern 234F-2366]. With no errors the handler never runs.

### 6.6 Channel 4, real-time clock (RTC)

A 17-bit counter increments every clock period. On reaching 234177 octal (79,999) it sets the
channel Done flag, clears to 0 and continues, so Done sets every 80,000 clock periods, which
is 1 ms at 12.5 ns. There is no Busy flag. The counter cannot be written [B 52, 5-12]
[IA 82, 3-16].

| Function | Effect |
|---|---|
| RTC : 0 | Clear Done |
| RTC : 6, RTC : 7 | Clear, set the channel 4 interrupt enable |
| RTC : 10 | A = counter bits 16 to 1 (0 to 39,999, that is 116077 octal); one count is 2 clock periods |

Source [B 52-53, 5-12 and 5-13]. The 1981 workbook notes that only some machines could read
the counter [T 77, 4.9]. cray-sim's clock is described in part 8.4.

### 6.7 Channel 5 and channels 6 to 13

These are specified in the peripheral note. What the processor needs from them is in part 9.

---

## 7. Interrupts

### 7.1 Sources and enables

There is one interrupt level. The request is the OR, over all channels, of Done and Channel
Interrupt Enable [B 44, 5-4] [B 132, 7-2]. Channel 0 has no enable flag and never requests,
although its Done flag reads as set (cray-sim: [sim cray_iop.h:280-281]). The built-in sources are
the PFR flag (channel 1), the Exit Stack Boundary flag (2), the Local Memory Parity Error flag
(3) and the RTC Done flag (4). An interrupt is taken when a request is present and I is set,
"upon completion of an instruction in the currently executing program" [B 59, 5-19].

### 7.2 Sequence

1. I = 0.
2. E = E + 1; XS[E] = P, where P is the address of the next instruction of the interrupted
   program. The boundary flag is not set by this push (part 5.2).
3. P = XS[0]. E is not used for this read.

Source [B 59, 5-19] [B 34, 3-10]. Nothing else is saved. cray-sim does the same three steps
[sim cray_iop.cpp:836-838].

The handler finds out who interrupted with IOR : 10, services that channel, and repeats until
IOR : 10 returns 0 [IA 74-75, 3-8 and 3-9]. It returns with `I = 1` then `EXIT` [B 60, 5-20].

*Kernel:* the handler at 0x22F7 [kern 22F7-2338] shows what software must be able to do:

- Entry: `I = 0`, `PASS`.
- Save A in an operand register. Save C: `A = A > 16` leaves the old C in bit 0 of A and
  clears C. Save B with `A = B`.
- Read E and XS[E] through channel 2 (the interrupted address).
- `IOR : 10`; `B = A`; if not zero, check the number is below 42 and jump through a table of
  42 handler addresses at 0x0D2B. The handlers use `IOB : 0` and the like, and jump back to
  the `IOR : 10`. Entry 0 of the table is the exit path, taken when IOR : 10 returns 0, so an
  interrupt entry with nothing pending is harmless.
- Exit: restore B; `A = saved C`; `A = A < 16` moves bit 0 into C and leaves A = 0;
  `A = A + saved A` cannot carry, so C survives; `I = 1`; `EXIT`.

At start-up the table has handlers for channels 0, 3, 4, 6 to 13, 16, 17 and 40 to 47; the
entries for 1, 2 and 5 are zero [kern 0D2B-0D54].

### 7.3 The delayed enable of 003

The three readings:

| Source | Rule |
|---|---|
| HR-0030, HR-0808 | "the System Interrupt Enable flag is delayed until the next nonbranch or non-I/O instruction is issued. Instructions 40 through 43 and 70 through 137 do not enable interrupts" [B 60, 5-20] [B 66, 6-6] [B 209, B-1] [1S 452, G-1]. Also: "A jump or exit instruction must be completed before interrupts are actually set" and "A 2-instruction window is necessary for an interrupt to occur" [B 59, 5-19]. If 002 follows 003, 002 wins [B 66, 6-6]. |
| CSM-1009 (Model C and D) | "The program enables interrupts after completion of the instruction immediately following the 003 instruction" [CD 42, 2-21]. |
| cray-sim (*sim only*) | 003 sets a pending flag. After each later instruction the flag becomes I = 1 unless that instruction was 001, 003, 040 to 043, any branch 070 to 137 whether taken or not, or an I/O instruction 140 to 177 addressed to a channel that has a device. 002 clears I and the pending flag [sim cray_iop.cpp:886-889, 1238, 1251-1252, 1263-1264, 964-1082, 1146, 1180, 1513-1568]. |
| Old Verilog | Intends "not 040 to 043 and not 070 to 137"; as written, an operator precedence slip holds off only 040 to 043 [V iop_inst_decode.v:243-251]. |

All readings agree on what matters: an interrupt cannot be taken between `I = 1` and the
instruction after it, and a branch after `I = 1` does not open the window. They differ on
whether EXIT or an I/O instruction opens it one instruction earlier. *Kernel:* `I = 1` is
followed by `EXIT` [kern 1C5B, 2337], by a return jump [kern 0972, 2D22], by an absolute jump
[kern 3F6D], by `PASS` [kern 1A6F] or by a load, an add and then `I = 0` [kern 2C05-2C08]; never by
an I/O instruction. Every one of these works under any of the readings.

Recommended rule: 003 sets pending; pending becomes I = 1 at the completion of the first later
instruction that is not 001, 003, 040 to 043, 070 to 137 or 140 to 177; 002 clears both. This
is cray-sim's rule with the empty-channel exception removed, and it is within the manual's
wording ("nonbranch or non-I/O", "jump or exit").

Two hardware quirks the manual warns about, neither of which software may rely on:

- If 003 is executed while I is already set and an interrupt arrives before the next
  non-branch instruction, the pending flag survives into the handler and re-enables interrupts
  at the handler's first non-branch instruction. Hence handlers start with 002
  [B 59, 5-19]. cray-sim keeps the pending flag across interrupt entry too
  [sim cray_iop.cpp:835-838].
- "The instruction following the 002 instruction may be skipped if an interrupt occurs while
  002 is executing", hence 002 is always followed by PASS [B 65, 6-5] [B 210, B-2]; the APML
  assembler adds the PASS itself [IA 54, 2-28]. cray-sim has no such effect. An implementation
  should make 002 atomic: an interrupt is taken either before it or not at all.

### 7.4 System and user distinction

There is none. No instruction is privileged, memory is unprotected, and any program can write
the exit stack, including the handler address in entry 0. The only mode bit is I.

---

## 8. Timing a program can observe

### 8.1 The real machine

- Clock period 12.5 ns from an 80 MHz crystal oscillator, adjustable slightly for maintenance;
  "When operations require exact timing information ... maintenance personnel should be
  contacted to verify the clock period" [B 20, 1-8]. The IOS clock is independent of the
  mainframe clock [B 44, 5-4].
- Execution times are in the CP column of part 4.3. Register operations take 1 to 5 clock
  periods, a memory read 7 to 9, a read-modify-write 13, a channel flag test 5, a branch inside
  the instruction stack 5, a branch that refills the stack about 9 or more [B 119, 6-59]
  [B 123, 6-63]. A channel function costs about 1 clock period for output and 4 to 6 for
  input [T 72, 4.4].
- No manual that was read states an instruction rate (searched for "instructions per second"
  and "MIPS"). **Derived estimate:** weighting the CP column by the opcode mix of the kernel
  image gives about 4.4 clock periods per instruction, about 18 million instructions per
  second. Allowing for stack refills and memory conflicts, 10 to 20 million instructions per
  second is a fair range. This number is mine, not Cray's.

### 8.2 The real-time clock in the software

The kernel's channel 4 handler clears Done and counts milliseconds; every 100th interrupt it
activates the clock demon and resets the count [kern 2378-238D] [SM 81, 2-49]. The demon,
every tenth of a second, runs the event timer that gives every software time-out (TPUSH,
PAUSE, device and channel time-outs, in tenths of a second); every second it updates
statistics and checks the MIOP to mainframe output channel time-out; every minute the MIOP
sends a heartbeat to each IOP and reports one that did not answer; it keeps the day clock
[SM 81-83, 2-49 to 2-51]. So the 1 ms Done interval sets the date, the time of day and all
long time-outs, and it must be the same in every IOP.

RTC : 10 occurs once in the kernel [kern 35B8]. The 1981 workbook gives the counter value as
the time stamp of trace entries [T 322, 18.2]. Nothing found depends on the value.

### 8.3 Code in the IOS software that depends on instruction time

Found by a linear sweep of `iop_kern.bin` and `iop_overlay1.bin` for short backward loops
that only count, and for counted loops around a Busy or Done test. The sweep cannot tell code
from data, so the overlay counts are approximate.

| What | Where | Count | Depends on |
|---|---|---|---|
| Hold Master Clear and deadstart bits on another IOP | [kern 4935-493A] | 8 iterations of 4 instructions | at least 200 ns [B 57, 5-17] |
| Wait for the other IOP's dead start load before restoring Buffer Memory | [kern 493D-4941] | 40,000 iterations of `A = A - 1`, `PASS`, `P = P - 2, A # 0` | about 10 CP each, 5 ms, against a load of about 2 ms [B 211, B-3] |
| Wait for Peripheral Expander (channel 17) Busy to clear. In the kernel's own tape reader a time-out aborts the dead start; in the expander interrupt handler it abandons the request; in overlays it reports a channel time-out. | [kern 48B3-48B8, 48BC-48C1, 36D2-36D7]; about 15 more in overlays | 400 iterations of 5 instructions | about 18 CP each, 90 µs (derived) |
| Wait for Buffer Memory channel Busy to clear in the dump path | [kern 3F8C-3F92] | 65,535 iterations of 4 instructions | |
| Waits on Done of a channel selected by B | overlays, about 20 sites | 4,096 or 65,536 iterations (A starts at 4096 or at 0) | |
| Master Clear pulse to the mainframe on the output channel (function 4) | `MFINIT.asm:96-103, 177-181`, `START0.asm:125-128` in the overlay disassemblies | 10,000, 1,536 and 4,096 iterations | minimum pulse width only |
| Other pure delays after a channel function | overlays, about 16 sites | 10 to 500 iterations | minimum time only |

What follows for an implementation:

1. **Dead start of another IOP.** The MIOP patches one parcel of Buffer Memory, releases the
   target, counts 120,000 instructions and then restores the parcel (part 9.3). The target's
   load must have read that parcel before the restore. Either make the 65,536-parcel load take
   less time than 120,000 MIOP instructions, or stall the MIOP from the release until the load
   is complete. cray-sim does the load inside the releasing I/O instruction, and in its
   multi-threaded build it stalls the releasing IOP until the target has started
   [sim iop_iop2iop.cpp:30-41, 64-72] [AT parallels.md:93-95].
2. **Counted Busy waits.** The kernel reads its overlays from the expander tape with the
   400-iteration wait after each expander function, and gives up with "TAPE ERROR: DEAD START
   ABORTED" if Busy is still set when the count runs out [kern 48B3-48C8, 48DD-48EB]. A device
   that answers through slow glue (for example a tape or disk image served by the HPS) must
   either clear Busy within a few hundred IOP instructions, or the IOP must be stalled while
   the operation is outstanding. cray-sim does not model device times, so these waits do not
   expire there; the peripheral note has the details.
3. **Pure delays** only cost time. At a lower instruction rate they take proportionally
   longer; 120,000 instructions at 1 million per second is 0.12 s.
4. **Relative speed of the IOPs.** In cray-sim's multi-threaded build the MIOP could run far
   ahead of the BIOP and exhaust the message buffers between them (kernel halt 015) while
   loading the mainframe image [AT parallels.md:99-175]. On the real machine all IOPs run from
   one clock. All IOPs in the FPGA should execute at the same rate.

Tantos wrote in 2013 that the simulation "is not cycle-accurate ... so far I haven't seen any
code that would rely on timing" [AT the-matrix.md:211]; the items above are what turned up
afterwards.

### 8.4 What cray-sim does about time (*sim only*)

| Item | Behaviour | Source |
|---|---|---|
| Instruction time | None. One "tick" of an IOP runs up to 100 instructions (`InstructionBurstSize`); the burst ends early at EXIT, `I = 1`, a taken return jump, a channel flag test or an I/O instruction. The IOPs of a cluster take turns, one per cluster tick. | [sim cray_iop.cpp:842-846, 998-1569 (the lines that end a burst); iop_cluster.cpp:129-135] [cfg 85] [AT the-matrix.md:108] |
| Interrupt latency | Interrupts are looked for once per tick, before the burst, so an interrupt can wait up to 100 instructions and a short enable window inside one burst is never used. | [sim cray_iop.cpp:817-846] |
| RTC Done | From the host's wall clock: every 51st tick it compares elapsed host time, converted to 80 MHz periods, with the time of the last interrupt and sets Done when more than `TimerLimit` (79,999) periods have passed. | [sim cray_iop_built_in_channels.cpp:142-151, cray_iop.h:415-417] [cfg 86] [AT turbo.md:133] |
| RTC : 10 | Elapsed host time in 80 MHz periods, divided by 2, low 16 bits. It does not wrap at 40,000 as the hardware counter does. | [sim cray_iop_built_in_channels.cpp:134-135] |
| Buffer Memory transfers | Complete inside the I/O instruction; Done is set at once. | [sim cray_iop_built_in_channels.cpp:172-185, 200-262] |
| Kernel memory test | Patched out: parcel 0x42B7 of the image becomes EXIT. | [cfg 63] |
| Clearing of the end of memory by the overlay loader | Patched out: parcel 0x4476 becomes PASS. | [cfg 64] |
| Buffer Memory test | Shortened: parcel 0x43DA becomes PASS. | [cfg 65] |
| Long delay in the DD-29 initialisation overlay | Shortened by a patch to the boot tape image. | [cfg 102] |
| Optional patches, disabled in the shipped configuration | A long loop in the expander disk overlay, a delay and two calls in START0, a time-out in the block multiplexer overlay, and the START3 change from NOWAIT to WAIT. | [cfg 103-120] |
| Releasing another IOP | Stall of the releasing IOP until the target runs; optional throttle of an IOP whose output word has not been taken. Only checked in the multi-threaded build. | [sim iop_iop2iop.cpp:64-83, cray_iop.cpp:809-813] |

The three image patches are for boot speed, not correctness. An FPGA build can apply the same
three parcels to the kernel image or let the tests run.

---

## 9. Dead start and Master Clear

### 9.1 The hardware mechanism

**Any IOP, from Buffer Memory.** If an IOP is master cleared with the deadstart signal set,
its Buffer Memory interface clears its Local Memory address, Buffer Memory address and block
length registers; when Master Clear drops it starts a read (the equivalent of MOS : 4) from
Buffer Memory address 0 to Local Memory address 0 and sets the channel 5 interrupt enable. A
block length of zero means 65,536 parcels [B 54, 5-14]. When the transfer completes, Done
interrupts the IOP [B 55, 5-15]. The transfer takes about 2 ms [B 57, 5-17] [B 211, B-3]. The
interrupt sequence then does what it always does: I = 0, E = E + 1, XS[E] = P, P = XS[0], and
XS[0] was cleared by Master Clear, so execution starts at 0 [B 51, 5-11].

**One IOP starting another.** Function 1 on an IOP output channel (7, 11 or 13) loads a
control register from A: bit 0 Master Clear, bit 1 deadstart, bit 2 dead dump (or, without
Master Clear, a Master Clear of Buffer Memory), bit 3 short transfer (4,096 parcels instead of
65,536). The bit numbers were read from the page image; the OCR text drops them. Set Master Clear and deadstart together for at
least 200 ns, then clear both; the deadstart bit has no effect without Master Clear
[B 56-58, 5-16 to 5-18]. The 1981 manual has only the first three bits
[1S 263, Part 3 5-17]. A dead dump is the reverse copy and does not interrupt.

**The MIOP.** The first dead start is from the Peripheral Expander tape or disk: the operator
sets the maintenance panel switches to 22 (tape) or 60 (disk) and presses the IOP-0 MC and
DEADSTART buttons; the first file (TAPELOAD or DISKLOAD) is read into MIOP Local Memory from
address 0 and started; it asks which file holds the Kernel and loads it [OG 26-29, 2-4 to 2-7]
[SM 83, 2-51]. The MIOP then initialises Buffer Memory and dead starts the other IOPs
[B 16, 1-4] [SM 84, 2-52]. The expander's part in this is in the peripheral note.

### 9.2 State at the first instruction

| Item | Value | Source |
|---|---|---|
| P | 0 | [B 51, 5-11] |
| I | 0 (cleared by the interrupt sequence) | [B 59, 5-19] |
| E | previous E + 1; 1 in cray-sim | [B 51, 5-11] [sim cray_iop.h:324, 341] |
| XS[0] | 0 | [B 51, 5-11] |
| XS[E] | the P that was interrupted; 0 in cray-sim | [sim cray_iop.cpp:837] |
| Local Memory | Buffer Memory words 0 to 16,383 | [B 55, 5-15] |
| Channel 5 | Done set, Busy clear, interrupt enable set; so IOR : 10 would return 5 | [B 55, 5-15] [sim cray_iop.h:436-437] |
| Other channel enables | cleared by Master Clear in cray-sim for channels 1, 2 and 4; set for the IOP to IOP channels | [sim cray_iop.h:301, 324, 409; iop_iop2iop.h:50, 69] |
| A, C, B, operand registers | whatever they held | part 2.3 |

*Kernel:* what it does first, which shows what it does not assume.

1. Jump over two parcels, clear OR[0], jump to the initialisation code [kern 0000-0005].
2. Save OR[511] in memory, clear OR[1] to OR[511] [kern 41E4-41EF].
3. Set E = 1, clear XS[1] to XS[15], write the handler address to XS[0] [kern 41F0-4208].
4. Read its own IOP number from parcel 0x0CF0 [kern 4209-420D].
5. Test channel 5: if Done and Busy are both set (a failed load) it halts [kern 423E-4245].
6. Issue function 6 and function 0 to channels 3 to 39 decimal [kern 4246-424E]. This turns
   off the channel 5 enable that dead start set and clears its Done flag.
7. Run the Local Memory test if the OR[511] value saved in step 2 was zero [kern 424F-4254].
   Whether a nonzero value has other effects was not traced. cray-sim leaves the register zero
   and patches the test routine instead (part 8.4).
8. Much later, enable channel 2, clear and enable channel 3, enable channel 4
   [kern 4A72-4A75], and only after that execute the first `I = 1`.

So the kernel sets up everything it uses except the channel 1 and channel 2 enables and
flags, which must come out of Master Clear cleared.

### 9.3 How the MIOP starts another IOP (*kernel*)

Routine at 0x4904, called for IOP 1 always and for IOPs 2 and 3 if configured
[kern 4A09-4A13]. Tantos explains it in [AT parallels.md:23-93].

1. Look up the input channel that connects to the target; stop if there is none.
2. Write the target's IOP number into local parcel 0x0CF0 and copy the 4-parcel group that
   contains it to Buffer Memory word 0x33C with a blocking transfer. Every IOP boots the same
   image from Buffer Memory address 0; parcel 0x0CF0 tells it which IOP it is.
3. B = that input channel + 1 (the output channel); A = 3; `IOB : 1`. Master Clear and
   deadstart are now set.
4. Put local parcel 0x0CF0 back to 0. Count 8 loop iterations.
5. A = 0; `IOB : 1`. Both bits clear; the target begins loading.
6. Count down 40,000 iterations.
7. Copy the restored group to Buffer Memory, so the image is correct for the next target.

How the kernel image reaches Buffer Memory address 0 after a tape or disk dead start was not
traced; in cray-sim it is already there (part 9.4).

### 9.4 cray-sim (*sim only*)

- The kernel image file is loaded into Buffer Memory at word 0 before anything runs, and IOP 0
  is dead started from Buffer Memory like any other IOP; the tape bootstrap (TAPELOAD) is
  skipped [cfg 12, 75-76] [sim iop_cluster.cpp:87-93].
- Dead start is Master Clear of the IOP and all its channels, then an immediate 16,384-word
  copy and Done on channel 5 [sim cray_iop.h:252, 436-437]. The IOP leaves its "in reset"
  state at the next tick, when the interrupt is taken [sim cray_iop.cpp:829-838].
- Function 1 on an output channel starts the target when the low two bits go from 11 to 00
  [sim iop_iop2iop.cpp:30-41]. Dead dump, and Master Clear without deadstart, stop the
  simulator [sim iop_iop2iop.cpp:32-33]. The short-transfer bit is ignored.
- While "in reset" an IOP would also be started by any other enabled interrupt, because the
  same test is used [sim cray_iop.cpp:817-838].

---

## 10. Where cray-sim and the manuals differ

| # | Subject | Manual | cray-sim | Third reading; effect on the kernel |
|---|---|---|---|---|
| 1 | Boundary flag on a push | Set only when a return jump advances E to 14; an interrupt that takes E to 14 does not set it [B 125, 6-65] [B 34, 3-10] | Set by any push, call or interrupt, that leaves E at 14 or 15 [sim cray_iop.h:345-346] | Old Verilog follows the manual [V iop_peri_pxs.v:111-112]. The kernel halts on a boundary interrupt, so the manual's narrower rule is the safe one. |
| 2 | EXIT with E = 0 | Flag set, decrement blocked, E stays 0 [B 64, 6-4] | Flag set, E becomes 15 [sim cray_iop.h:353-363] | Old Verilog follows the manual [V iop_peri_pxs.v:101, 115-116]. Kernel treats handler entry with E = 0 as fatal [kern 22FE-2304]. |
| 3 | Which instructions hold off `I = 1` | 040-043 and 070-137; text also says "jump or exit" [B 60, 5-20] [B 59, 5-19] | Also 001, 003 itself, and 140-177 on a channel with a device | Part 7.3. No effect on the kernel. |
| 4 | End-off shift count of 32 or more (d or B) | Low 5 bits only [B 67, 6-7] | Whole value; result cleared [sim cray_iop.cpp:726-730, 739-743] | Old Verilog uses 5 bits [V iop_inst_exec.v:389-390]. Not used with fixed counts in the kernel. |
| 5 | Circular left shift by 18 to 31 | Not stated | Low bits filled with zeros [sim cray_iop.cpp:745-751] | Old Verilog has the same defect [V iop_inst_exec.v:391]. Not used with fixed counts in the kernel. |
| 6 | PFR : 10 | Also clears the PFR flag [B 50, 5-10] | Returns the register, flag unchanged [sim cray_iop_built_in_channels.cpp:44-45] | Unused by the kernel. |
| 7 | PFR flag on a conditional absolute branch that is not taken | Not stated | Not set [sim cray_iop.cpp:1027-1031] | Unused by the kernel. |
| 8 | Interrupt point | After every instruction [B 59, 5-19] | Once per burst of up to 100 instructions | Part 8.4. |
| 9 | Skip after 002; re-enable inside the handler | Hardware quirks [B 65, 6-5] [B 59, 5-19] | First absent, second present | Part 7.3. |
| 10 | Real-time clock | 17-bit counter of clock periods, wraps at 79,999 | Host wall clock, no wrap | Part 8.4. |
| 11 | E and the stack at Master Clear | XS[0] cleared; E advanced by the dead start interrupt [B 51, 5-11] | E and all 16 entries cleared, then E = 1 | The kernel sets E and clears entries 1 to 15 itself. |
| 12 | Channel with no device | Not stated | Flag tests by B give 0; functions 0 and 6 ignored; anything else is an error or stops the simulator | The kernel sends functions 6 and 0 to all channels 3 to 39. |
| 13 | Number of channels | 40, numbered 0 to 47 [B 41, 5-1] | 42, numbered 0 to 51 [cfg 84] | Model C documents say 42 [IA 68, 3-2]; the kernel accepts interrupt numbers below 42. |

In rows 1 and 2 the manual is explicit and the old Verilog agrees with it, so the manual should
be implemented. A reference model that must match cray-sim instruction for instruction needs a
switch for rows 1, 2, 4, 5 and 6. Rows 1, 2 and 6 are not reached by a kernel that is running
normally: under cray-sim's looser rule 1 the kernel would halt if E ever reached 14, and it
does not. Rows 4 and 5 are not reached by shifts with a fixed count.

---

## 11. Open questions

| # | Question | Best available answer | Source |
|---|---|---|---|
| 1 | Does an EXIT at E = 0 also run the interrupt sequence (push), or only jump to XS[0] with the flag set? | Only the jump: P = XS[0], E stays 0, flag set, nothing pushed. The manual describes the handler as entered with E = 0 and the boundary request present, which fits no push. Whether the still-pending request then causes a second, ordinary interrupt entry is not stated. The kernel halts either way. | [B 33-34, 3-9 and 3-10]; old Verilog comment [V iop_peri_pxs.v:1-3] |
| 2 | Is I changed by an EXIT at E = 0? | Unknown. Leave I as it is. | none |
| 3 | Does the boundary flag set for the relative return jumps 072, 073 and the conditional return jumps? | Yes, for every return jump that advances E to 14. | [B 32-33, 3-8 and 3-9]; explicit only for 076, 077 [B 125-126] |
| 4 | Circular shifts by 18 to 31 | Rotate by the count modulo 17, which is what a 17-bit circular shifter gives and what cray-sim does for right shifts. Count how often it happens in a trace before relying on it. | [B 39, 4-3]; part 4.1 |
| 5 | End-off shifts when B or d is 32 or more | Use the low 5 bits (manual). cray-sim clears the result. | [B 67, 6-7]; part 10 row 4 |
| 6 | Do I/O instructions 140-177 and EXIT hold off the delayed enable? | Treat them as holding it off. Works under every reading and matches cray-sim. | part 7.3 |
| 7 | Is the PFR flag set when a conditional absolute branch is not taken? | No (cray-sim). The manual says only "during execution". | [B 35, 3-11]; [sim cray_iop.cpp:1027-1031] |
| 8 | When the PFR flag is set and channel 1 is enabled, is the branch completed before the interrupt? | Yes: the branch completes, a return jump pushes its return address, and the interrupt then pushes the new P. | "with an interrupt request at the completion of the interrupted instruction" [B 35, 3-11]; [B 33, 3-9]; cray-sim does this |
| 9 | Which bits of d or B select the channel, and what does a number above 47 (or 51) address? | Unknown. Decode 6 bits for IOR : 10 results; treat numbers with no device as an empty channel: flags 0, input 0, functions ignored. The kernel image has no I/O instruction with d above 51. | [B 49, 5-9]; part 6.2 |
| 10 | Do functions 10 to 13 clear C? | Yes. | [IA 78, 3-12] [T 76, 4.8] [CD 37, 2-16]; cray-sim |
| 11 | Reset value of E, of XS[1..15], of the operand registers, A, B, C | Not specified; the kernel does not depend on any of them. Use the values in part 2.3. | [B 51, 5-11]; [kern 41E4-4208] |
| 12 | Channel interrupt enables after Master Clear | Channels 1 to 4 cleared; channel 5 set by dead start. The kernel needs channel 1 cleared and clears 3 to 39 itself. | [B 55, 5-15]; cray-sim; [kern 4246-424E] |
| 13 | Self-modifying code and the 32-parcel instruction stack: does a store to a parcel already in the stack take effect? | The manuals do not say. cray-sim has no instruction stack, always fetches from memory, and boots the system, so always fetching from memory is sufficient. | [B 27-29, 3-3 to 3-5]; [sim cray_iop.cpp:854-856] |
| 14 | Instruction rate of the real machine | 10 to 20 million per second, derived from the per-instruction clock periods. | part 8.1 |
| 15 | Exact clock period at which an interrupt is recognised, and the "2-instruction window" | Model it at instruction boundaries. | [B 59, 5-19] |
| 16 | Does anything in the overlays (as opposed to the kernel) use PFR, shift counts above 17 or the boundary interrupt? | Not checked instruction by instruction. The training manual says the operating system does not use PFR or stack restructuring. A trace counter for each in the reference model would settle it during the first boot. | [IA 76, 3-10] [IA 78, 3-12] |

---

## 12. Checks taken from the kernel

Sequences the real kernel executes during every boot. Each fixes a detail of the model; they
make good unit tests for the RTL and the reference model.

| # | Sequence | Required result | Where |
|---|---|---|---|
| 1 | `A = A > 16` with C = c | A = c, C = 0 | [kern 22FA] |
| 2 | `A = x` (x is 0 or 1), `A = A < 16`, `A = A + y` | C = x, A = y | [kern 2334-2336] |
| 3 | `A = A >> 17` three times | A and C unchanged | [kern 2300-2303] |
| 4 | `A = n`, `A = A - 42`, then branch on C | C = 1 exactly when n >= 42 | [kern 231C-231E] |
| 5 | `B = B + 1` from 511 | B = 0, A = 512, and `A = B` then reads 0 | [kern 41EA-41EF] |
| 6 | `PXS : 14` with A = 16; `PXS : 10` | E = 0; A = 0, C = 0 | [kern 41FC-41FE, 41F5] |
| 7 | `OR[x] = OR[x] - 1` then `P = P - n, A # 0` | A holds the decremented value | [kern 3F91-3F92] |
| 8 | `R = OR[52]` with a PASS that carries a code in d as the next parcel; the routine reads its return address with `PXS : 11` | XS[E] = address of that PASS | [kern 2303-2304, 211C] |
| 9 | `P = OR[0] + k` with OR[0] = 0 | P = k; PFR flag set; no interrupt | [kern 0005] |
| 10 | `IOB : 6`, `IOB : 0` on channels 3 to 39 | No effect on empty channels | [kern 4246-424E] |
| 11 | `I = 1`, `EXIT` with a request pending | The interrupt is taken after the EXIT, at the earliest, never before it | [kern 2337-2338] |
| 12 | `IOR : 10` with nothing pending | A = 0 | [kern 230C-2312] |
| 13 | Read E, subtract 1, `PXS : 14`, three shifts, jump | The return address on top of the stack is discarded | [kern 36DA-36E0] |
| 14 | `R = P - 263` | Target is 263 parcels before the instruction; return address is the next parcel | [kern 4A0B] |

Opcodes that a linear sweep of the kernel image never finds: 061, 062, 063, 065, 066, 067,
110, 114, 120, 121, 122, 134, 135. All others occur. The overlays were not swept for this.

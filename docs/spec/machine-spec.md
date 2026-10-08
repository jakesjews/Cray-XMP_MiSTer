# Machine specification: a 1982 CRAY-1 (rev F) plus the X-MP/1 features COS 1.17 needs

> A working specification, written while the core was built and kept as it was. Manuals are
> named by their file names on [Bitsavers](http://www.bitsavers.org/pdf/cray/). `cray-sim/` is the
> [cray-sim](https://github.com/andrastantos/cray-sim) project, `system/` its ready-to-run
> COS 1.17; `cos-cray1/` and directories of experiments are working material that is not in
> this repository.
>
> When this was written the CPU could also be built as a CRAY-1: a parameter `XMP` chose, the
> reference model had `Cpu::Xmp`, the assembler `MACHINE XMP`, and the X-MP's tests were in
> `tests/xmp`. All of that is gone. The CPU is what this text calls `XMP = 1`, the tests are in
> `tests/directed`, and nothing is built as `XMP = 0`.

Written 2026-10-05 from the Cray manuals. RTL line numbers are for commit `5b73aab`.

## 0. Conventions and sources

**Bit numbering.** The manuals number bits from the left: bit 0 is the most significant bit of a
64-bit word. In this note "bit n" always means that. The same bit is 2^(63-n) and Verilog index
`[63-n]`. Exchange package fields are given all three ways. Instruction codes are octal.

**Page citations.** Every page number is a PDF page (not the printed page) of one of these files:

- `[C n]` rev C, Nov 1977: `Cray-1-Reference-Manual.pdf` (204 pages; OCR text
  `manual-ocr/Cray-1-Reference-Manual.md`). The printed page is added where useful.
- `[F n]` rev F, May 1982: `manuals/cray-1/HR-0004F-...May_1982.OCR.pdf` (218 pages).
- `[E n]` rev E, May 1979: `manuals/cray-1/2240004E_Hardware_Reference_197905.pdf`.
- `[1S n]` CRAY-1 S, Nov 1981: `manuals/cray-1/HR-0808_CRAY-1S_HWref_Nov81.pdf`.
- `[X n]` X-MP/1, Aug 1986: `cray-1x/docs/hw/CSM-0111000-B-...August_1986.OCR.pdf`.
- `[H32 n]` X-MP dual, July 1984: `cray-1x/docs/hw/HR-0032-...July_1984.OCR.pdf`.
- `[H97 n]` X-MP four-processor, Aug 1986: `cray-1x/docs/hw/HR-0097B_...pdf`.
- "sim" is the instrumented cray-sim in `cos-cray1/sim/simulator/sim_lib/`. It is a second
  reading, not an authority. "probe" is `cos-cray1/results/probe_*.json` read with
  `analyze_probe.py`.

**What rev F is.** Its record of revisions [F 3] says rev F is rev E (May 1979) with change packet
E-01 (May 1980) and "no other changes". Rev E added the "standard options": vector population
instructions, programmable clock interrupt, monitor mode interrupt, corrected the multiply
description and rewrote sections 5 and 6. E-01 documented the symmetric multiply. Rev D was rev C
with C-01 and C-02; C-01 "changes the nomenclature for two flags in the exchange package" [F 2].

**How the comparison was made.** Rev F sections 1, 3, 4, 5 and appendices A to C were compared
with the rev C OCR text sentence by sentence with a fuzzy matcher, and every hold-issue and
timing line of section 4 was compared separately. Appendix D was compared entry by entry.
Section 6 was rewritten in rev E; it was read directly for what a program sees (instructions,
interrupts, errors, master clear, memory access, clocks). Its channel signal timing pages (6-7
to 6-17) were not compared. Figures 3-5 and 3-8 were read as images. In the sections compared,
what is not listed in part 1 was found the same in meaning.

---

## Part 1. CRAY-1 rev C to rev F: differences a program or operating system can see

### 1.0 Summary

| # | Subject | Rev C | Rev F | RTL impact |
|---|---|---|---|---|
| 1 | F register bits 31 and 32 | 31 Console interrupt, 32 RTC interrupt [C 79, 3-37] | 31 Programmable clock interrupt, 32 MCU interrupt [F 74] | yes |
| 2 | Programmable clock option | none; 0014jk always sets RTC [C 98] | 0014j4/5/6/7, II, ICD, request [F 96-97, 195-196] | yes |
| 3 | Scalar population count parity | 026ijx, k ignored [C 113] | 026ij0 count, 026ij1 parity (option) [F 111] | yes |
| 4 | Vector population count and parity | 174ijx is always the reciprocal [C 156] | 174ij1, 174ij2 (option); 174 needs k = 0 [F 56, 155, 156] | yes |
| 5 | Monitor mode interrupt option | M is 4 bits; monitor mode masks all but memory error [C 77-78] | M is 5 bits with the option: word n+1 bit 39 [F 73-77] | optional |
| 6 | Floating-point multiply pyramid | irregular truncation, one round bit, not commutative [C 66-68] | symmetric truncation at 2^-56, constant 9, two round bits, commutative [F 63, 65] | already rev F |
| 7 | Multiply underflow edge | zero result when result exponent <= 17777 [C 64] | same, but exponent sum 20000 with a normalising shift delivers exponent 17777 and the coefficient [F 61] | already rev F |
| 8 | Half-precision product width | 31 bits, low bit zero after a left shift [C 68] | "30-bit result", 18 low bits zero [F 65, 66]; 1S and X-MP say 29 bits, 19 zero | open question |
| 9 | 0012 | clears the channel interrupt and error flags [C 98] | "and/or deactivate the channel" [F 94]; section 6 rewritten | I/O only |
| 10 | I/O interrupt and error conditions | 3 causes, 5 error conditions, same for all channels [C 178-179] | by channel module type [F 175-179] | I/O only |
| 11 | I/O clear at dead start | clears all input channel addresses and makes them active [C 87] | only the MCU input channel is cleared and activated [F 84] | I/O only |
| 12 | Programmed master clear | one 4-step sequence [C 181] | two 8/9-step sequences that start with 0012 on both channels [F 189-191] | I/O only |
| 13 | Exchange and exit time | 50 CPs [C 97, 103]; 49 CPs for an interrupt [C 83] | 50 = 36 exchange + 14 fetch [F 93, 101]; 49 "exchange and fetch" [F 80] | none |
| 14 | Scalar memory hold issue | 100-137 "in the CIP register" [C 189] | "in the NIP register"; 070 "destination register conflict" [F 203] | none |
| 15 | 4 million words | says the instruction format could address four million words directly but that no such memory is planned [C 167] | sentence removed; still 262,144 / 524,288 / 1,048,576 words [F 21, 23, 163] | none |
| 16 | VL and chaining | not mentioned | caution: do not change VL between operations that chain [F 43-44] | none |

Items found unchanged: A, B, S, T, V, VL, VM, P (22 bits), BA, LA (18 bits, units of 16 words),
XA, the exchange package layout apart from word n+1 bit 39 and the names of F bits 31 and 32, the
exchange sequence rules, field protection, instruction formats [F 87-91] against [C 93-94], 033
[F 115-116] against [C 117], 034-037, 072 to 075, every other instruction's hold-issue list and
execution time, appendix A numbers, the 20-bit channel CA and CL registers, channel numbers 2 to
31 octal, memory sizes and the 20-bit memory address.

### 1.1 F register bits 31 and 32

- Rev C figure 3-8: bit 31 "Console interrupt", bit 32 "RTC interrupt" [C 79]. The 1975
  preliminary manual lists the same two in the same order.
- Rev F figure 3-8: bit 31 "Programmable clock interrupt" (footnote: supports the Programmable
  Clock option), bit 32 "MCU interrupt" [F 74]. The X-MP manuals agree with rev F [X 61].
- The other seven flags are the same: 33 floating-point error, 34 operand range error, 35 program
  range error, 36 memory error, 37 I/O interrupt, 38 error exit, 39 normal exit.

So between rev C and rev F the clock interrupt is at bit 31 and the interrupt from the maintenance
control unit (the "console") is at bit 32. Whether rev C's figure had the two labels the wrong way
round or the hardware changed cannot be told from these manuals (open question Q1). A rev F
machine, and COS, need: **bit 31 (2^32, `[32]`) = PCI, bit 32 (2^31, `[31]`) = MCU**.

### 1.2 Programmable clock option (complete definition)

Sources: [F 95-97] instruction 0014, [F 195-196] section 6, [F 209] appendix D, [F 74] flag.
The X-MP/1 text [X 69-71, 121-122] and the 1S text [1S 80] are the same and add the points marked
(X).

**Registers**

- II, interrupt interval register, 32 bits.
- ICD, interrupt countdown counter, 32 bits.
- An enable (set by 0014j6, cleared by 0014j7).
- The programmable clock interrupt request, one bit.

**Instructions.** All are 0014jk, monitor mode only, selected by k. "When the Programmable Clock
Option is not installed, none of these subfunctions is recognized and the instruction is always
interpreted as an enter real-time clock register instruction" [F 96].

| Code | CAL | Action in monitor mode |
|---|---|---|
| 0014j0 | `RT Sj` | RTC <- (Sj). j = 0 clears RTC. |
| 0014j4 | `PCI Sj` | II <- (Sj) bits 2^31..2^0 and ICD <- the same value. (X) j = 0 or (Sj) = 0 clears both. |
| 0014j5 | `CCI` | Clear the request "if the request was previously set by an interrupt count down to zero". |
| 0014j6 | `ECI` | Enable requests. |
| 0014j7 | `DCI` | Disable requests until the next 0014j6. |

- j is not used by k = 5, 6, 7 (rev F writes 0014j5; the X-MP CAL forms are 001405, 001406, 001407).
- k = 1, 2, 3 are not defined on the CRAY-1 with the option (Q3). On the X-MP k = 3 is the
  cluster number and k = 1, 2 are interprocessor interrupt (multiprocessor models only).
- Hold issue [F 97]: 034-037 in process; exchange in process; "Aj or Sj or Ak reserved" (the
  same line rev C prints for all of 001x; the X-MP manual reduces it to "Sj reserved (except S0)"
  [X 122]). Execution time: instruction issue 1 CP. No result register, no functional unit.
- Not in monitor mode: "instruction becomes a no-op but all hold issue conditions remain
  effective" [F 97].

**Counting** [F 196]

1. ICD "runs continuously but counts down, decrementing by one each clock period until the
   contents of the counter are zero."
2. "At this time, it sets the programmable clock interrupt request. The counter then samples the
   interval value held in the interrupt interval register and repeats the countdown to zero
   cycle."
3. "A programmable clock interrupt request can be set only after the 0014j6 instruction has been
   executed to enable the interrupt." The enable gates the setting of the request. It does not
   stop the counter.
4. "When the programmable clock interrupt request is set, it remains set until a 0014j5
   instruction ... is executed." 0014j7 does not clear a request that is already set.
5. "A programmable clock interrupt request only causes an interrupt when not in monitor mode; a
   request set in monitor mode is held until the system switches to user mode."
6. II keeps its value until the next 0014j4 [F 195].
7. Nothing is defined at dead start: "the monitor program should insure the state ... by clearing
   (0014j5) and disabling (0014j7)" [F 196].

**How the request reaches the F register.** F bit 31 sets when the request is set and the active
package is not in monitor mode. Setting it starts an exchange like any flag. The flag is stored in
the outgoing package. The request itself is not part of the package and stays set until 0014j5.
If the monitor returns to a user package without issuing 0014j5 the flag sets again at once.

**Unit.** One count per clock period: 12.5 ns, 80 MHz, intervals of 12.5 ns to 53.7 s on a
CRAY-1 [F 195]. (X) 8.5 ns or 9.5 ns on an X-MP/1 [X 69].

**Reference model**

```
state: II[31:0], ICD[31:0], EN, REQ          (dead start: undefined; use 0)
each clock period:
  if 0014j4 issues in monitor mode:  II <= Sj[31:0]; ICD <= Sj[31:0]
  else if ICD == 0:                  ICD <= II;  if EN: REQ <= 1
  else:                              ICD <= ICD - 1
  if 0014j5 issues in monitor mode:  REQ <= 0      (a set in the same clock wins: choice)
  if 0014j6 issues in monitor mode:  EN <= 1
  if 0014j7 issues in monitor mode:  EN <= 0
F[31] sets while REQ && !monitor_mode
```

Read literally the counter passes through II, II-1, ..., 1, 0 and then reloads, so requests are
II + 1 clock periods apart, although the text calls II "the number of clock periods that are to
elapse between" requests (Q2).

### 1.3 Vector Population Instructions Option

One option adds three instructions [F 51, 56, 111, 155, 156, 209, 212].

**026ij1, `Ai QSj`** [F 111]. Count the one bits of (Sj); put the least significant bit of the
count in the least significant bit of Ai. "The actual population count is not transferred", so
Ai is 0 or 1. Same unit, hold-issue list and time as 026ij0: Ai ready 4 CPs, issue 1 CP,
(Ai) = 0 if j = 0. Without the option it "operates as a 026ij0 instruction". 026ij0 is unchanged
(count in the low 7 bits of Ai, upper 17 bits zero). k = 2 to 7 are not defined; the X-MP uses
026ij7 for a shared register.

**174ij1, `Vi PVj`** [F 156]. For each of VL elements: count the one bits of the Vj element; the
count goes to the low 7 bits of the Vi element, "the remaining higher order bits ... are zeroed".

**174ij2, `Vi QVj`** [F 156]. For each of VL elements: the least significant bit of the count goes
to the least significant bit of the Vi element. "The actual population count results are not
transferred" (the other 63 bits are zero).

- "If this option is not installed, these instructions are executed as vector reciprocal
  approximation instructions" [F 156]. With the option, "the k field must be 0 for the floating
  point reciprocal approximation instruction" [F 57, 155]. 174ij3 to 174ij7 are then undefined.
- Unit: the vector population count unit. It is a separate unit, but "174 in process; unit busy
  for (VL) + 4 CPs" is its hold-issue condition, the same line as the reciprocal's [F 154, 156],
  and appendix D lists both under "F.P. Rcpl" [F 212]. A population count and a reciprocal
  therefore do not run at the same time (an inference from that line, Q4).
- Hold issue [F 156]: 034-037 in process; exchange in process; Vi reserved; "Vk reserved" (as
  printed; it must mean Vj, Q4); 174 in process.
- Times [F 156]: issue 1 CP; Vi ready 13 CPs if VL < 5, VL + 8 CPs if VL > 5; Vj ready 5 CPs if
  VL <= 5, VL CPs if VL > 5; unit ready VL + 4 CPs; chain slot ready 8 CPs. With the rule "chain
  slot = functional unit time + 2" [F 43] the unit time is 6 clock periods (the reciprocal's
  chain slot is 16, unit time 14).

### 1.4 Monitor Mode Interrupt option

- Rev C: M has 4 bits, all in word n+2: 36 correctable memory error mode, 37 floating-point error
  mode, 38 uncorrectable memory error mode, 39 monitor mode. "When this bit [39] is set, all
  interrupts other than memory errors are inhibited" [C 77].
- Rev F: "M is 4 bits without MMI option; 5 bits with option" [F 77]. The fifth bit is word n+1
  bit 39 (2^24, `[24]`), "interrupt monitor mode select" [F 73-74].
  - Not in monitor mode: any of the nine flags can set.
  - Monitor mode 1 (n+2 bit 39 = 1, n+1 bit 39 = 0): only the memory error flag can set, and only
    if mode bit 36 or 38 is set. "An exchange sequence can be initiated by a 000 or a 004
    instruction even though the associated error exit flag or normal exit flag is not set" [F 76].
    This is the rev C behaviour.
  - Monitor mode 2 (both bits 1): "all F register flags other than the PC interrupt, MCU
    interrupt, I/O interrupt, and normal exit flags can be set and an exchange sequence will be
    initiated" [F 76].
- Floating-point overflow interrupts need "monitor mode one ... not in effect" [F 60].
- Without the option word n+1 bit 39 has no meaning. The 1S manual does not have the option
  (M is 4 bits there [1S 70]); the X-MP has it as IMM. COS does not need it
  (`cos-cray1/README.md`).

### 1.5 Floating-point multiply

**Pyramid.** Rev C figure 3-5 and its table [C 66-67] show an irregular cut through the low
partial products, a truncation constant, one round bit at 2^-49 with product bit 2^-49 forced to
zero, and say "A x B is not necessarily the same as B x A" [C 68]. Rev F (E-01 pages) [F 63, 65]:

- Coefficients are fractions, bits 2^-1 to 2^-48. The partial product of multiplicand bit 2^-p and
  multiplier bit 2^-q has weight 2^-(p+q). In the figure only weights 2^-2 to 2^-56 are summed;
  everything to the right of 2^-56 is never formed.
- A constant of nine units of 2^-56 (binary 1001 at 2^-53..2^-56) is always added.
- Rounded multiply (066, 164, 165): round bits at 2^-50 and 2^-51.
- Half-precision (065, 162, 163): round bits at 2^-31 and 2^-32.
- "The multiplication is commutative."
- Error range -0.23 x 2^-48 to +0.57 x 2^-48 [F 63].

The X-MP/1 manual describes the same pyramid in more words [X 101-103].

**Half-precision result.** Rev C: "a 31-bit result (2^-1 to 2^-30[sic]) is transmitted back. If
the result requires a left shift, the bottom bit will be zero" [C 68]. Rev F: "a 30-bit result
(2^-1 to 2^-30) is transmitted back" [F 65] and "the 18 low-order bits ... are returned as zeros"
[F 66]. CRAY-1 S: "the 19 low-order bits ... are returned as zeros" [1S 109]. X-MP: "the 29 most
significant bits of the normalized result are transmitted" [X 103]. See Q5.

**Underflow.** Rev C: "detected when the result exponent is less than or equal to 17777" [C 64].
Rev F: detected "when the sum of the exponents is less than or equal to 17777 ... However, if the
sum of the exponents is 20000 and a normalizing left shift occurs, an exponent of 17777 is sent
to the result register along with the computed coefficient" [F 61]. The test is made before the
normalising shift.

**Unnormalised operands.** Rev C: the result "is not guaranteed to be normalized if the input
operands are not normalized" [C 60]. Rev F: "not guaranteed to be correct" [F 57].

**Integer multiply.** Same hardware rule in both (both exponents zero: no normalising shift,
exponent zero, upper 48 bits of the product). Rev F adds how to use it: 24-bit integers in bits
16 to 39 of each operand give a 48-bit product starting at bit 16 [F 58, 61].

### 1.6 I/O instructions 0010 to 0012 and 033

- 0010jk, 0011jk: no change. No-op if j = 0, outside monitor mode, or if (Aj) is not a channel
  number. Both revisions print the bound two ways: the text says "less than 2 or greater than 25"
  (decimal; 25 is channel 31 octal, the last one), the special-case line says "(Aj) < 2 or
  (Aj) >= 31 octal". k = 0 sets CA or CL to 1 [F 94-95] = [C 98].
- 0012jx: rev C "clear the interrupt flag and error flag for the channel indicated by (Aj)"
  [C 98]. Rev F adds "and/or deactivate the channel" [F 94]; its master clear sequences use 0012
  to stop a channel ("this stops the input channel activity just initiated") [F 190]. k is not
  used on a CRAY-1; the X-MP gives 0012j1 a meaning.
- 033: no change in text, times or special cases [F 115-116] = [C 117]. j = 0: lowest numbered
  channel with an interrupt request. j != 0: k even gives CA of channel (Aj), k odd its error
  flag in the low bit. Not privileged. (Ai) = 0 if (Aj) = 1. Wait 2 CPs after 0012.
- New statement: "Setting of the current and limit registers is limited to monitor mode" [F 194].
- I/O interrupt causes, rev F [F 175]: on any output channel (CA) = (CL) (at the resume of the
  last parcel on the common module); (CA) = (CL) on an input channel of the DV module type;
  disconnect received on any input channel; a channel error. Rev C [C 178]: (CA) = (CL),
  disconnect on input, channel error.
- The channel memory scanner: rev C "a memory request that is accepted causes the requesting
  channel to miss the next time slot" [C 175]; rev F "whether accepted or rejected" [F 191].
- Dead start: rev F's I/O clear "clears the input channel address register of the channel
  connected to the MCU and activates the input channel connected to the MCU subsystem. All other
  input channels remain inactive" [F 84]. Rev C: "clears the input channel address registers to
  zero and sets an active status" [C 87].

### 1.7 Timing notes

- Error and normal exit: "50 CPs; this time includes an exchange sequence (36 CPs) and a fetch
  operation (14 CPs)" [F 93, 101]. Interrupt: 49 CPs [F 80]. Rev C gives only 50 and 49.
- 174ij1 and 174ij2 times are in 1.3. Appendix A's unit table has no row for the population unit
  in either revision [F 201].
- 073 after 003 or 175, the scalar and vector reservation rules and the unit times in appendix A
  are unchanged [F 199-201] = [C 185-188].
- Hold issue for scalar memory references: see row 14 of the summary.

---

## Part 2. X-MP/1 features COS 1.17 needs

Scope is set by `cos-cray1/README.md`. All definitions are from the X-MP/1 manual unless marked.

### 2.1 Exchange package

Figure 3-3 and table 3-1 [X 56-57]; field text [X 57-63]. Unlisted bits are unused; store zero.

| Word | Bits | 2^n | Verilog | Field |
|---|---|---|---|---|
| 0 | 1 | 2^62 | [62] | PN processor number. "Always 0"; a constant put into the stored package, never loaded [X 57] |
| 0 | 2-3 | 2^61, 2^60 | [61:60] | E error type: bit 2 uncorrectable, bit 3 correctable |
| 0 | 4-11 | 2^59..2^52 | [59:52] | S syndrome |
| 0 | 16-39 | 2^47..2^24 | [47:24] | P, 24 bits: 22-bit word address and 2-bit parcel |
| 0 | 40-63 | 2^23..2^0 | [23:0] | A0 |
| 1 | 0-1 | 2^63, 2^62 | [63:62] | R read mode: 00 I/O, 01 scalar, 10 vector/B/T, 11 fetch or exchange |
| 1 | 2-4 | 2^61..2^59 | [61:59] | CS chip select of the error address |
| 1 | 7-11 | 2^56..2^52 | [56:52] | B bank of the error address |
| 1 | 16-34 | 2^47..2^29 | [47:29] | IBA instruction base address, 19 bits, units of 32 words |
| 1 | 35 | 2^28 | [28] | WS waiting for semaphore (status) |
| 1 | 36 | 2^27 | [27] | FPS floating-point error status |
| 1 | 37 | 2^26 | [26] | BDM bidirectional memory mode |
| 1 | 39 | 2^24 | [24] | IMM interrupt monitor mode |
| 1 | 40-63 | | [23:0] | A1 |
| 2 | 0 | 2^63 | [63] | VNU vector not used |
| 2 | 16-34 | 2^47..2^29 | [47:29] | ILA instruction limit address, 19 bits, units of 32 words |
| 2 | 35 | 2^28 | [28] | IOR operand range error mode |
| 2 | 36 | 2^27 | [27] | ICM correctable memory error mode |
| 2 | 37 | 2^26 | [26] | IFP floating-point error mode |
| 2 | 38 | 2^25 | [25] | IUM uncorrectable memory error mode |
| 2 | 39 | 2^24 | [24] | MM monitor mode |
| 2 | 40-63 | | [23:0] | A2 |
| 3 | 0 | 2^63 | [63] | ESVL enable second vector logical ("not available on all systems") |
| 3 | 15 | 2^48 | [48] | F: DL deadlock |
| 3 | 16-23 | 2^47..2^40 | [47:40] | XA |
| 3 | 24-30 | 2^39..2^33 | [39:33] | VL |
| 3 | 31 | 2^32 | [32] | F: PCI programmable clock interrupt |
| 3 | 32 | 2^31 | [31] | F: MCU interrupt |
| 3 | 33 | 2^30 | [30] | F: FPE floating-point error |
| 3 | 34 | 2^29 | [29] | F: ORE operand range error |
| 3 | 35 | 2^28 | [28] | F: PRE program range error |
| 3 | 36 | 2^27 | [27] | F: ME memory error |
| 3 | 37 | 2^26 | [26] | F: IOI I/O interrupt |
| 3 | 38 | 2^25 | [25] | F: EEX error exit |
| 3 | 39 | 2^24 | [24] | F: NEX normal exit |
| 3 | 40-63 | | [23:0] | A3 |
| 4 | 0 | 2^63 | [63] | EAM enhanced addressing mode ("only on 8-million-word systems") |
| 4 | 16-34 | 2^47..2^29 | [47:29] | DBA data base address, 19 bits, units of 32 words |
| 4 | 35 | 2^28 | [28] | PS program state |
| 4 | 38-39 | 2^25, 2^24 | [25:24] | CLN cluster number |
| 4 | 40-63 | | [23:0] | A4 |
| 5 | 16-34 | 2^47..2^29 | [47:29] | DLA data limit address, 19 bits, units of 32 words |
| 5 | 40-63 | | [23:0] | A5 |
| 6, 7 | 40-63 | | [23:0] | A6, A7 |
| 8-15 | 0-63 | | [63:0] | S0 to S7 |

Multiprocessor models also have word 3 bit 14 (ICP, interrupt from internal CPU) and word 1
bit 38 (SEI, selected for external interrupts) [H32 63, 66-67]. The X-MP/1 figure leaves both
unused.
COS loads packages with word 1 bit 38 set 18 times in the study (probe); nothing needs it.

**Differences from the CRAY-1 rev F package** [F 74]:

| Field | CRAY-1 rev F | X-MP/1 |
|---|---|---|
| E | word 0 bits 0-1 | word 0 bits 2-3 |
| S | word 0 bits 2-9 | word 0 bits 4-11 |
| R | word 0 bits 10-11; 00 scalar, 01 I/O, 10 vector, 11 fetch | word 1 bits 0-1; 00 I/O, 01 scalar, 10 vector/B/T, 11 fetch/exchange |
| error address | B word 0 bits 12-15, RA word 1 bits 0-15 | CS word 1 bits 2-4, B word 1 bits 7-11 |
| PN | none | word 0 bit 1 |
| P | word 0 bits 18-39 (22 bits) | word 0 bits 16-39 (24 bits) |
| base | BA word 1 bits 18-35, 18 bits, x16 words; one pair for code and data | IBA word 1 bits 16-34, 19 bits, x32 words; DBA word 4 bits 16-34 |
| limit | LA word 2 bits 18-35, x16 | ILA word 2 bits 16-34, x32; DLA word 5 bits 16-34 |
| word 1 bits 35-39 | bit 35 is the low bit of BA; 39 IMM (option) | 35 WS, 36 FPS, 37 BDM, 39 IMM |
| word 2 bit 35 | low bit of LA | IOR |
| word 2 bits 36-39 | ICM, IFP, IUM, MM | the same |
| word 2 bit 0 | unused | VNU |
| word 3 | XA 16-23, VL 24-30, F 31-39 | the same, plus DL at bit 15 and ESVL at bit 0 |
| word 4 | A4 only | EAM, DBA, PS, CLN, A4 |
| word 5 | A5 only | DLA, A5 |

A bit of a base or limit field has the same address weight in both layouts (bit 34 is 32 words
in both). The X-MP field is the CRAY-1 field without its 16-word bit and with two more bits on
the left.

**Store and load rules**

- Loaded from the package: P, IBA, ILA, DBA, DLA, all M bits, VNU, ESVL, XA, VL, F, EAM, PS, CLN,
  A0-A7, S0-S7. Not loaded: PN, E, S, R, CS, B. (The manual says "not read into the CPU" only for
  PN; the rest follows from the field descriptions.)
- "If any bit [of F] remains set, another exchange occurs immediately" [X 61]; the monitor "must
  clear the flags in the F register area of the package" first.
- B, T, V, VM, SB, ST and SM are not exchanged [X 55].
- VNU [X 60]: stays set if no 076, 077 or 140-177 issued in the interval; cleared by the first
  one; "once cleared, the bit remains clear until reset through a memory store to the dormant
  Exchange Package". COS does not need it (it may be stored as 0, README).
- WS and FPS "indicate the state of the CPU at the time of the exchange" [X 60].
- Exchange address, XA x 16, lower 4096 words: unchanged [X 62].
- Interrupt P: "address of first program instruction not yet issued" [X 58]. Exit P: the parcel
  after the exit [X 118, 129]. Same as the CRAY-1.

### 2.2 Modes and flags

**M register**, 9 bits [X 59-60]:

| Bit | Name | Meaning | Changed during an interval by | COS needs |
|---|---|---|---|---|
| w1 35 | WS | exchanged while a test and set was holding in CIP | hardware | no |
| w1 36 | FPS | a floating-point error occurred, whatever IFP is | hardware; cleared by 002100 or 002200 [X 125] | no |
| w1 37 | BDM | block reads and writes may run concurrently | 002600 sets, 002500 clears | no (may be carried only) |
| w1 39 | IMM | "enables all interrupts in monitor mode except PC, MCU, I/O, and ICP" | no | no |
| w2 35 | IOR | enables operand range interrupts | 002300 sets, 002400 clears | may be treated as always 1 outside monitor mode |
| w2 36 | ICM | correctable memory error interrupts | no | store and load only |
| w2 37 | IFP | floating-point error interrupts | 002100 sets, 002200 clears | yes |
| w2 38 | IUM | uncorrectable memory error interrupts | no | store and load only |
| w2 39 | MM | "inhibits all interrupts except memory errors, error exit, and normal exit" | no | yes |

- 002100 and 002200 take effect at issue + 1 CP and apply to operations already in progress;
  002300 and 002400 likewise [X 126].
- In the study COS never executed 0021 to 0026; it sets these bits in the packages. Loaded
  packages had IOR, ICM, IFP, IUM, MM and BDM set (probe). Library code uses 0021/0022.

**F register**, 10 flags on the X-MP/1 [X 61-62]:

| Bit | Flag | Set when | COS needs |
|---|---|---|---|
| 15 | DL | "the CPU (CLN != 0) is holding issue on a test and set instruction" | not exercised |
| 31 | PCI | ICD reaches 0 (the request, part 1.2) | yes: 98,954 exchanges |
| 32 | MCU | "the MIOP sends this signal" | yes: 2 exchanges |
| 33 | FPE | range error in a floating-point unit and IFP set | yes |
| 34 | ORE | data reference outside DBA..DLA and IOR set | yes |
| 35 | PRE | instruction fetch outside IBA..ILA | yes |
| 36 | ME | memory error and its mode bit set | never sets here |
| 37 | IOI | "a 6 Mbyte channel or the 100 Mbyte to SSD channel completes a transfer" | yes: 8,969 exchanges |
| 38 | EEX | "if not in MM, set by an error exit instruction (000)" | yes |
| 39 | NEX | "if not in MM and IMM, set by a normal exit instruction (004)" | yes |

"Any flag (except the ME flag) can be set in the F register only if the active Exchange Package
is not in monitor mode ... the flag remains cleared and no exchange sequence is initiated"
[X 62]. 000 and 004 still exchange in monitor mode, with no flag.

### 2.3 Cluster number register and 0014j3

- CLN is 2 bits on the X-MP/1 (word 4 bits 38-39). "Loaded from the Exchange Package or if the CPU
  is in monitor mode, through instruction 0014j3" [X 37].
- 0014j3: CLN <- j. CAL forms 001403, 001413, 001423, 001433 [X 121]. Privileged: a pass
  instruction outside monitor mode. "For instruction 0014j3, hold issue 2 CPs"; issue 1 CP
  [X 122]. j = 4 to 7 are not defined (Q11).
- CLN = 1, 2, 3 select one of three sets of shared registers. CLN = 0: "prevents any access to
  shared registers", "instructions regarding the shared registers become no-ops, except for the
  instructions returning values to Ai or Si, which return a zero value" [X 38, 121].
- COS: 592 executions, all in monitor mode; cluster 1 for EXEC and STP, cluster 2 for jobs (probe).

### 2.4 Shared registers and semaphores

Three sets, each with eight 24-bit SB, eight 64-bit ST and 32 one-bit SM registers [X 37]. "The
SM register is 32 bits with SM0 being the most significant bit" [X 127]. None of these
instructions is privileged: appendix A marks 0010 to 0015 as privileged and none of these
[X 208-213], and COS runs them in user mode (29 test-and-sets, 16 SB writes and 16 ST writes;
probe). User and monitor mode behave the same except for the deadlock flag below.

| Code | CAL | CLN = 1, 2, 3 | CLN = 0 | Times and holds |
|---|---|---|---|---|
| 0034jk | `SMjk 1,TS` | if SMjk = 1 hold issue; else SMjk <- 1 and issue | no-op, issues | issue 1 CP [X 127-128] |
| 0036jk | `SMjk 0` | SMjk <- 0 | no-op | issue 1 CP |
| 0037jk | `SMjk 1` | SMjk <- 1 | no-op | issue 1 CP |
| 026ij7 | `Ai SBj` | Ai <- (SBj) | Ai <- 0 | hold: Ai reserved; 027ij7 or 073ij3 issued 3 CPs earlier. Ai ready 1 CP [X 142-143] |
| 027ij7 | `SBj Ai` | SBj <- (Ai) | no-op | hold: Ai reserved. SBj ready 3 CPs [X 144] |
| 072i02 | `Si SM` | Si bits 0-31 <- SM0..SM31 (SM0 at 2^63); Si bits 32-63 <- 0 | Si <- 0 | hold: Si reserved. Si ready 1 CP [X 169-171] |
| 072ij3 | `Si STj` | Si <- (STj) | Si <- 0 | hold: Si reserved; 073ij3 or 027ij7 issued 3 CPs earlier. Si ready 1 CP |
| 073i02 | `SM Si` | SM0..SM31 <- Si bits 0-31 ("SM00 receives the sign bit") | no-op | hold: Si reserved. SM ready 1 CP [X 170-171] |
| 073ij3 | `STj Si` | STj <- (Si) | no-op | hold: Si reserved [X 170-171] |

- jk for 0034, 0036, 0037 is 0 to 31 decimal [X 127]. 32 to 63 are not defined (Q12).
- j in 026ij7, 027ij7, 072ij3, 073ij3 names register 0 to 7. j = 0 is SB0 or ST0, not the
  special value.
- A read issued within 2 CPs after a write of the same register gets the old value [X 143, 171].
  A core without that pipeline delay should deliver the new value, or hold issue.
- 072i00 stays `Si RT` and 073i00 stays `Si VM`. 026ij0, 026ij1, 027ij0 keep their meaning.

**Test and set** [X 38, 116, 127]

1. The semaphore is tested when the instruction would issue. Clear: it is set and the instruction
   issues. Set: the instruction holds in CIP "until the value is 0".
2. "If the CPU holds issue on a test and set instruction, it receives a deadlock interrupt. No
   deadlock interrupt can occur in cluster 0." With one CPU every hold is a deadlock.
3. The deadlock flag is set "if not in monitor mode" [X 127]. In monitor mode (IMM clear) no
   flag sets and the CPU holds for good; nothing else can clear the semaphore on an X-MP/1.
4. "If a test and set instruction is holding in the CIP register and an interrupt occurs ...
   the instruction in the NIP register and the test and set instruction in the CIP register are
   discarded and the P register is adjusted to point to the discarded test and set instruction.
   The Waiting on Semaphore (WS) flag in the Exchange Package sets" [X 38]. The stored P is the
   address of the 0034 instruction, so it is retried when the package is next loaded.
5. A deadlock exchange is such an exchange: F bit 15 set, WS set, P at the 0034.

sim: 0034 on a set semaphore does not advance P and is retried every step; the deadlock flag is
logged, never raised (`cray_cpu_inst_0000_0037.h` lines 628-656). COS never held in the study.

### 2.5 Status register, 073i01 (`Si SR0`)

"Instruction 073i01 sets the low-order 32 bits to 1's and returns the following status to the
high-order bits of Si" [X 170]:

| Si bit (2^n) | Left bit | Content |
|---|---|---|
| 2^63 | 0 | CL: clustered, CLN != 0 |
| 2^57 | 6 | PS program state |
| 2^51 | 12 | FPS floating-point error occurred |
| 2^50 | 13 | IFP floating-point interrupt enabled |
| 2^49 | 14 | IOR operand range interrupt enabled |
| 2^48 | 15 | BDM bidirectional memory enabled |
| 2^40 | 23 | PN processor number, "always 0"; returns 0 if not in monitor mode |
| 2^33 | 30 | CLN bit 1; returns 0 if not in monitor mode |
| 2^32 | 31 | CLN bit 0; returns 0 if not in monitor mode |
| 2^31..2^0 | 32-63 | all ones |

All other bits of the high half are 0. Not privileged. Hold issue: Si reserved; Si ready 1 CP
[X 170-171]. j must be 0: 073i11, 073i21, 073i31 are performance monitor and maintenance codes
[X 169]. The dual-processor manual has the same table without the sentence about the low 32 bits
[H32 173]; the four-processor manual has the sentence [H97 168]. sim returns zeros in the low
half and does not hide PN and CLN in user mode (`cray_softcpu.cpp` GetSR). COS read it 1,495,165
times in monitor mode and twice in user mode (Q6).

### 2.6 Programmable clock on the X-MP

The same machine as part 1.2 [X 69-71, 121-122]: II and ICD 32 bits, 0014j4 loads both from the
low 32 bits of Sj (j = 0 or (Sj) = 0 clears both), 001405 clears the request, 001406 enables,
001407 disables, the request is held in monitor mode, flag at word 3 bit 31. Differences:

- Standard, not an option.
- One count per clock period of 8.5 or 9.5 ns.
- Hold issue is "Sj reserved (except S0)".
- "For instruction 0014j0, the value is entered into the RTC register 4 CPs after instruction
  0014j0 issues" [X 122].

COS use: 0014j4 102,285 times, 001406 102,284, 001405 202,373, 001407 3 times, all in monitor
mode (probe).

### 2.7 Base and limit registers

[X 58-59, 66-69]

- Four registers, each 19 bits holding "the high-order 19 bits of a 24-bit memory address. The
  low-order 5 bits ... are assumed to be 0". Fields begin on a multiple of 32 words and end one
  short of a multiple of 32. "The fields can overlap."
- **Instruction pair (IBA, ILA):** instruction fetches only. Absolute word = P word address +
  IBA x 32 "modulo two to the twenty-second power". The test is "made at instruction buffer fetch
  time". Outside the pair: program range error [X 67-69].
- **Data pair (DBA, DLA):** every operand reference: 10h to 13h (A and S), 034 to 037 (B and T)
  and 176, 177 (V) [X 68-69]. Absolute = operand address + DBA x 32 modulo 2^22. Outside the
  pair: operand range error if IOR is set.
- Neither pair: channel references (CA and CL are absolute) and the exchange package (XA x 16).
- Last valid address is (limit x 32) - 1. The limit is absolute, not relative to the base.
- Out of range, whatever the mode bits [X 67]: "A memory read reference beyond the assigned field
  limits issues and completes, but a zero value is transferred from memory. A memory write
  reference beyond the assigned field limits is allowed to issue, but no write occurs."
- Flags: PRE and ORE obey the monitor mode rule of 2.2. ORE also needs IOR [X 69]. What a fetch
  outside the instruction pair delivers in monitor mode, where PRE cannot set, is not stated.
- Lower bound: the text says an operand must be at an address "greater than or equal to" DBA,
  and also that a fetch below IBA "can only occur through a jump or branch ... beyond the memory
  capacity of the machine" [X 67-68]. With the modulo that means a reference whose sum carries
  out of 2^22 lands below the base. sim reduces the sum modulo the memory size and tests only the
  limit (`cray_softcpu.cpp` ReadDataMem). See Q7.
- With EAM clear, "instructions 100 through 137 ... have address bits 2^22 and 2^23 replaced by
  database address bits 2^22 and 2^23" [X 62]: on a 4 MW machine only the low 22 bits of
  (Ah) + jkm are used.

**Rule to implement (4 MW):**

```
rel22  = ((Ah) + sign_extend(jkm))[21:0]         (or (A0) + n*(Ak), (A0) + n)
sum    = rel22 + {DBA, 5'b0}                     (25 bits, not reduced modulo anything)
valid  = sum < {DLA, 5'b0}  &&  sum < MEMORY_WORDS
read:  valid ? memory[sum] : 0          write: only if valid
ORE   <= !valid && IOR && !MM           (IOR may be forced to 1: README)
fetch: the same with P[23:2], IBA, ILA; PRE <= !valid && !MM
```

COS values (probe): monitor package IBA 0, ILA 37000, DBA 0, DLA 17777740 (octal words); every
other package has IBA = DBA and ILA = DLA. COS reads DBA and DLA back from stored packages, so
they must be stored exactly as loaded.

### 2.8 P and memory addressing for 4 million words

- 4 MW is 2^22 words: 22-bit word addresses [X 29-30]. The constant is in the image (README).
- P is 24 bits: "the high-order 22 bits ... indicate the word address ... relative to the base
  address. The low-order 2 bits indicate the parcel" [X 52]. Stored in word 0 bits 16-39.
- Branches 006 to 017 take "the low-order 24 bits of the ijkm field ... The high-order bit of
  the ijkm field is ignored" [X 131-133]. (CRAY-1: low 22 bits; a program range error if either
  of the two low bits of i is set [F 90, 103].) 005 takes all 24 bits of Bjk [X 130].
- With the high bit of i set, 01hijkm is `Ah exp`, a 24-bit constant to Ah [X 114, 209]. COS
  never executed it: every 006 to 017 it ran had i = 0 (probe, 3 x 10^9 instructions).
- A registers are 24 bits. jkm is 22 bits and sign-extended (COS uses negative displacements with
  an index, 545,036 times in user mode; probe). 020 and 021 are unchanged.
- Exchange packages stay in the lower 4096 words. Channel references are absolute, so CA and CL
  must hold 22 bits to reach 4 MW (an inference; a CRAY-1 has 20 bits [F 194]).
- Instruction buffers hold 128 parcels each on an X-MP [X 53]; not visible to a program.
- The CRAY-1 S reached 4 MW with a 24-bit P and the unchanged 18-bit BA and LA in units of 16
  words [1S 66, 70-71, 78]. The COS image needs the X-MP/1 layout, not that one.

### 2.9 Instructions COS treats as no-ops

| Code | Meaning on an X-MP | Required here | COS use (probe) |
|---|---|---|---|
| 002700 | CMR: "does not issue until all memory references before this instruction are at the stage of execution where completion occurs in a fixed amount of time" [X 125-126] | issue only when no memory reference is in progress, then nothing | 353,162 monitor, 15 user |
| 001402 | `IP 0`, clear the received interprocessor interrupt; multiprocessor models [H32 125]; not in the X-MP/1 manual | pass | 1, monitor |
| 0012j1 | `MC,Aj`: clear the channel interrupt and error flags; output channel: set device master clear; input channel: clear a held ready [X 119] | clear the interrupt and error flags; no other effect | 6, monitor |
| 0017jk | not defined ("may produce indeterminate results" [X 117]); a CRAY-1 passes 001ijk for i = 5, 6, 7 [F 94] | pass, in user and monitor mode | 211,482,558, all user |

0012j0 after 0012j1 turns the master clear off [X 45]. 0015 and 0016 were never executed.

### 2.10 MCU and I/O interrupt flags

- **MCU, bit 32.** "Set when the MIOP sends this signal" [X 61]. Masked in monitor mode like
  every flag but ME [X 62], and by IMM [X 59]. The manual does not say whether a signal that
  arrives in monitor mode is kept (Q9). sim keeps it and also lets it through in monitor mode
  (`cray_softcpu.h` InterruptMaskFromState).
- **I/O, bit 37.** Conditions [X 42]: "CPU is not waiting for an exchange. CPU is not in monitor
  mode. An interrupt is present." A channel's interrupt request is set by: (CA) = (CL) on an
  output channel at the resume of the last parcel; a disconnect on an active input channel; a
  channel error [X 42]. It stays set "until cleared by the monitor program" with 0012 [X 43].
  033 with j = 0 returns the lowest numbered requesting channel and does not clear anything.
  The flag is therefore a level: F bit 37 sets whenever any request is set and the CPU is not in
  monitor mode.
- Channels: 10 to 17 octal, even input, odd output [X 43]. 0010, 0011 and 0012 pass if j = 0 or
  if the low 4 bits of (Aj) are below 10 octal [X 119-120]. COS uses 10 and 11, and `CI` on 12 to
  17 (README).
- Dead start: "forces the XA register contents to 0 and also forces an interrupt" [X 64]; the
  package at 0 is exchanged in. The CRAY-1 does the same with a forced 000 [F 79].

---

## Part 3. What the RTL has today

OK = correct as far as reading shows. Paths are under `rtl/cray/`; `ft` is
`cray-1x/func_top.v`, `ic` is `cray-1x/xmp/intercpu_comms.v`.

### 3.1 Part 1 items (CRAY-1, `XMP = 0`)

| Item | Status | Evidence |
|---|---|---|
| F bit 31 = PCI, bit 32 = MCU | **Wrong for rev F** (it follows rev C) | bit 31 is the core's console interrupt, `ft:617-619`; bit 32 never sets, `ft:621` |
| Programmable clock | **Missing** at `XMP = 0`; present with faults at `XMP = 1` | enable forced off, `ft:1484`. Faults: (a) 0014j4/5/6/7 not gated by monitor mode, `ft:1475-1476, 1479, 1485-1486, 1495`; (b) a request raised in monitor mode is lost: the flag is forced to 0 there and the set is a one-clock pulse, `ft:617-618, 1500`; there is no request latch; (c) the counter stops when disabled, `ft:1496-1498` (manual: runs continuously); (d) at `XMP = 0` every 0014jk loads the RTC, `ft:1469` (right for rev C, wrong with the option) |
| 026ij1 | **Missing** | k is forced to 0, `ft:371`; `cray-1x/scalar_pop_lz.v` has no parity output; `cray-1x/a_res_lut.v:38, 63` looks only at k bit 2 |
| 174ij1, 174ij2 | **Missing** | decoded only for `XMP = 1`, `cray-1x/v_scheduler.v:141-156`; no unit, `ft:1182`, and no tracker for unit 6, `ft:894-946`; `xmp/vector_pop_parity.v` is not instantiated. At `XMP = 0` they run as the reciprocal, `v_scheduler.v:50`, which is the "option not installed" behaviour |
| Monitor mode interrupt option | **Missing** (an option) | word n+1 bit 39 stored as 0 and not loaded, `ft:504, 566`; flags masked by MM only, `ft:617-640, 668` |
| Multiply pyramid, round bits, commutative | OK | `fp_mul.v:14-21, 43-83, 113-127` |
| Multiply underflow and the 20000 edge | OK | `fp_mul.v:142-153` |
| Half-precision width | 29 bits (1S and X-MP reading) | `fp_mul.v:137`; rev F's text says 30 (Q5) |
| 0014j0 and 0013 privileged; exits set no flag in monitor mode | OK | `ft:1469, 552, 638-640` |
| Flags in a loaded package cause another exchange | OK | `ft:597-608, 668` |
| 0010 to 0012, 033, I/O interrupt | **Missing** (no channels) | `cray_cpu.v:255-268`; `ft:1421-1423` only lets them issue |
| Memory size 1 MW, P 22 bits, 20-bit limit | OK for rev F | `ft:237, 432-434`; `mem_fu.v:388-396` |
| Exchange and instruction times | not modelled (see `docs/CPU.md`) | |

### 3.2 Part 2 items (`XMP = 1` paths, untested)

| Item | Status | Evidence |
|---|---|---|
| Package store, words 0 to 3 | OK | `ft:489-494`: PN, P 16-39, IBA and ILA 16-34, mode bits, DL and ICP, XA, VL, F |
| Package store, words 4 and 5 | **Wrong** | `ft:495-496` store `data_base_addr[23:6]` and `data_limit_addr[23:6]` in bits 16-33 and 0 in bit 34: the 32-word bit of DBA and DLA is dropped. DLA 17777740 comes back as 17777700 |
| Package load | OK | P `ft:1560`; IBA, ILA, DBA, DLA from bits 16-34 `ft:526-544`; modes `ft:566-576`; flags `ft:597-608`; XA `ft:550`; VL `ft:1514`; CLN `ft:673` |
| PS | Missing (stored 0, not loaded) | `ft:242, 495`. Enough for COS (README) |
| VNU, E, S, R, CS, B | Missing (stored 0) | `ft:489-493`. Enough for COS |
| CLN field | 3 bits at 37-39 instead of 2 at 38-39 | `ft:241, 495, 673`. Same for values 0 to 3 |
| 0021, 0022 (IFP) | **Wrong** | decoded only when `!XMP`, `ft:577-578` |
| 0023 to 0026 | **Missing** | not decoded; IOR and BDM change only by exchange |
| ORE gated by IOR | not gated | `ft:628`. Allowed by the README |
| FPS, WS | carried only, never set | `ft:567-568, 491` |
| PCI flag and clock | **Wrong** | as 3.1 (b), (a), (c) |
| MCU flag | **Missing** | `ft:621`; no input |
| I/O flag | **Wrong** | `ft:636` is right in form, but `xmp/dma_fu.v:706` raises its request only in monitor mode, where `ft:636` forces the flag to 0. `dma_fu.v` does not decode 0012 at all, and 033i00 clears every pending request and is gated by monitor mode, `dma_fu.v:454, 711-728` |
| DL flag, deadlock | **Missing** | `ft:614` |
| EEX, NEX, FPE, PRE | OK | `ft:624, 631, 638-640` |
| 0014j3 | **Wrong** | `ft:674`: not gated by monitor mode, acts on `cip_vld` without issue, not gated by `XMP`, takes 3 bits of j |
| SB and ST registers | Plausible | select by cluster, zero on CLN 0: `xmp/intercpu_sb_mux.v:17-19`, `xmp/intercpu_st_mux.v:17-19`; writes need a cluster match, `ic:552, 560`. Five clusters of storage, three needed |
| 0034, 0036, 0037 single bits | Plausible | `ic:322-326, 348, 358-360, 662-683`. Hold test uses jk[4:0] but set and clear compare all 6 bits |
| 072i02 bit order | **Wrong** | `xmp/intercpu_sm_mux.v:18-22` puts SM31 at 2^63 (register index = jk). Manual: SM0 at 2^63 |
| 073i02 | **Wrong** | `ic:670` etc. take Si bits 2^31..2^0 with SMn = Si[n]. Manual: the high 32 bits, SM0 from the sign bit |
| Issue handshake of the shared-register block | **Wrong (by reading, not simulated)** | The block registers its inputs, `ic:150-179`, and answers a clock later from the registered copy, `ic:358-360`; `ft:872` accepts that answer for whatever shared-register instruction is in CIP now. The valid it sees is also forced low while any A or S result is pending, `ft:1452`. Sequence `072i02` then `0034jk` (COS does this at 32341a/b): the 0034 issues on the answer meant for the 072 and the block never performs it, so the semaphore is not set and a set semaphore would not hold. (It is held correctly only when the j field of the 0034 happens to name the S register the 072 is loading, because `cray_opnd.v:42` treats 0034 as `VM Sj`.) |
| RTC write at `XMP = 1` | **Wrong** | `ic:216, 535` ignore monitor mode (the registered mode, `ic:159`, is never used) |
| 073i01 | **Wrong** | decode `cray-1x/s_res_lut.v:109`; value `ft:768` has only PN at 2^41..2^40 and CLN at 2^34..2^32. No CL, PS, FPS, IFP, IOR, BDM; low half is 0; PN and CLN not hidden in user mode |
| Separate data pair | OK | `ft:243-246, 536-544, 1357-1358`; `mem_fu.v:388-396` |
| Limit and memory size | **Wrong for 4 MW** | limits clamp at 2^20 words, `ft:432`, `mem_fu.v:392`. Memory is 1 MW (`docs/CPU.md`); the core's I/O page is at words FFFF0-FFFFF hex, `cray_system.sv:128`, `mem_fu.v:104` |
| Out-of-range read value | differs | "delivers whatever the unit last read", `mem_fu.v:36-39, 297-301`. X-MP: zero |
| Operand address width | 24-bit sum, no wrap | `mem_fu.v:194, 395-396`. X-MP uses the low 22 bits (Q7) |
| P 24 bits, 24-bit branches | OK | `ft:237, 439, 1559-1560`; `cray-1x/brancher.v:72-74`; buffers take 22-bit word addresses, `cray-1x/i_buf.v:36, 90` |
| 002700 | OK as a pass | no decoder claims it, `ft:875`. A vector transfer may still be running behind it |
| 001402 | OK at `XMP = 1`; at `XMP = 0` it loads the RTC | `ft:1469, 1432` |
| 0012j1 | pass | `ft:1421-1423`; `dma_fu.v` ignores 0012 |
| 0017jk, 0015, 0016 | OK as a pass | `ft:875`; `cray_opnd.v:34-40` |
| VL in X-MP form, 023i01 | Missing | `ft:1513`, `cray-1x/a_res_lut.v:61`. Not used by COS (Q14) |

---

## Part 4. Work list and open questions

### 4.1 Work list

Each step leaves the CRAY-1 regression (`XMP = 0`, model comparison) passing and adds its own test.
Extend the reference model in `tools/crates/model` in the same step.

1. **Split the `XMP` parameter** into feature parameters, for example `PCLK`, `VPOP`, `MMI`,
   `XEP` (X-MP/1 package, four field registers, 24-bit P), `SHREG` (cluster and shared
   registers), `MEM_WORDS`. Remove the channel and four-CPU modules from the build. Test: no
   change at the old `XMP = 0`.
2. **F bits 31 and 32.** Move the console interrupt to bit 32 (the MCU flag) behind a request
   input, and free bit 31 for the clock. Update the monitor ROM. Test: console interrupt stores
   F = 2^31 in the package.
3. **Programmable clock** as in 1.2: II, ICD, enable, request latch; monitor mode gating;
   0014j0 the only RTC load when `PCLK`; j ignored for k = 5, 6, 7. Tests: period measured with
   the RTC; request raised in monitor mode is taken at the first user instruction; 0014j5,
   0014j7; all four are passes in user mode; no interrupt before 0014j6.
4. **026ij1, 174ij1, 174ij2.** Parity output in `scalar_pop_lz`; a population unit of 6 clocks
   on a seventh tracker, sharing the busy of the reciprocal unit; 174 with k >= 3 as today.
   Tests: random operands against the model, VL 1 to 64, chaining into and out of the unit,
   result register also the operand.
5. **(Optional) monitor mode interrupt bit** per 1.4.
6. **4 MW.** `MEM_WORDS` in `ft:432`, `mem_fu.v:392` and the memory mapper; move the I/O page
   (no CPU reference of COS reaches words 17777740 to 17777777 octal: its largest limit is
   17777740).
   Test: the memory test and a fetch above 1 MW.
7. **X-MP/1 package and field registers** (`XEP`): store DBA and DLA with bits 16-34
   (`ft:495-496`); CLN 2 bits; PS stored and loaded; 0021 to 0026 in this mode; IOR gate on ORE;
   zero for an out-of-range read; 22-bit operand addresses; branch range rule for 24-bit P.
   Test: a round trip that loads a package with every field set to a pattern, exits, and compares
   the stored package bit for bit; range tests on both pairs with IBA != DBA.
8. **Cluster register and shared registers** (`SHREG`), as one block inside the CPU with no
   registered handshake: CLN with monitor gating; 3 x (8 SB, 8 ST, 32 SM); SM0 at the sign bit;
   CLN 0 rules; operand holds for Ai and Si in `cray_opnd`. Test and set: hold in CIP; outside
   monitor mode set DL and WS and exchange with P at the 0034; an interrupt during a hold does
   the same without DL. Tests: every instruction in clusters 0 to 3 and in both modes;
   `072i02` directly followed by `0034jk`; set, test-and-set, deadlock exchange, clear, retry.
9. **Status register** per 2.5.
10. **Interrupt requests**: an MCU request input and a level I/O request; 0010 to 0012 and 033
    for channels 10 to 17 with 22-bit CA and CL; 0012j1 as 0012j0. The channel model itself is
    the I/O work described in the README.
11. **No-op audit** in COS mode: 002700 waits for memory to be idle; 001402, 0017jk, 0015, 0016
    pass in both modes and never touch the RTC.
12. **COS in simulation**: dead start the image and compare exchange packages and the
    instruction trace with cray-sim up to the first station message.

### 4.2 Open questions

- **Q1. F bits 31 and 32.** Rev C labels them console and RTC; rev F clock and MCU. No manual
  here says whether the hardware changed. The core's console interrupt should move to bit 32.
- **Q2. Clock period.** II or II + 1 clock periods between requests; what II = 0 does; state at
  dead start; which wins when 0014j5 and a countdown to zero meet in one clock.
- **Q3. 0014jk with k = 1, 2, 3** on a CRAY-1 with the clock option is undefined. Suggested: pass.
- **Q4. 174ij1/2**: the hold-issue list prints "Vk reserved" [F 156]; "Vi ready 13 CPs if
  VL < 5" against "<= 5" elsewhere; 174ij3 to 174ij7 with the option; whether the population unit
  and the reciprocal unit really share one busy.
- **Q5. Half-precision product**: 30 bits with 18 zeros (rev E and F) or 29 bits with 19 zeros
  (CRAY-1 S, X-MP, the RTL). The round bits at 2^-31 and 2^-32 fit the 29-bit reading.
- **Q6. 073i01**: ones in the low 32 bits (X-MP/1 and four-processor manuals) or not (dual
  manual, sim); PN and CLN hidden in user mode (manuals) or not (sim). COS has only been seen to
  work with the sim's form. Worth one run of the study with the manual's form.
- **Q7. Range checks**: is the lower bound tested, or only the limit after a modulo-2^22 sum?
  Are bits 2^22 and 2^23 of an operand address ignored (manual, sim) or an error (RTL today)?
  The rule in 2.7 ignores them and treats a carry as out of range.
- **Q8. Exit flags in monitor mode.** The manuals say none is set. sim stores NEX when the
  monitor exits (125,572 times in the probe). COS should not care; unverified.
- **Q9. MCU interrupt** arriving in monitor mode: held or lost? sim also delivers it in monitor
  mode. Suggested: latch the request, raise the flag outside monitor mode.
- **Q10. Flags in a loaded package.** The manuals say they cause another exchange. sim does not
  load F at all. COS on real hardware must have cleared them; unverified here.
- **Q11. CLN** width and 0014j3 with j = 4 to 7 (sim stops with an error). Suggested: 2 bits.
- **Q12. 0034, 0036, 0037 with jk >= 32** (sim stops with an error). Suggested: use jk[4:0].
- **Q13. Test and set in monitor mode on a set semaphore** holds for good on one CPU. Confirm
  COS never does it, or add a simulation check.
- **Q14. VL.** The X-MP stores 100 octal when the low 6 bits of (Ak) are 0 and never more than
  100 [X 124, 140]; the CRAY-1 stores the low 7 bits [F 98]. Only the VL field of a stored
  package and 023i01 show the difference. COS never executed 023i01 in the study.
- **Q15. Clock rate.** RTC and ICD count the core's 81.67 MHz clock; COS was built for an
  8.5 or 9.5 ns clock period. Time of day and time slices will scale unless both counters are
  advanced at the X-MP rate. The README lists the real-time clock rate as not tested.
- **Q16. Field width.** X-MP/1: 19 bits at 16-34. Dual: 17 bits at 18-34 [H32 63]. The same for
  4 MW.
- **Q17. 0012 "and/or deactivate the channel"** [F 94] and the rest of section 6 matter only
  when channels are built.
- **Q18. Should the rev F machine include the monitor mode interrupt option?** COS does not use
  it.
- **Q19. 003ijk with i = 1, 2, 3, 5** in COS mode: undefined on an X-MP, `VM Sj` on a CRAY-1.

---

## Part 5. Status after the work of 2026-10-05

Part 3 describes the RTL as it was before this work. What changed, by work list number:

1. `XMP` kept as one build parameter. The upstream channel and four-CPU modules
   (`rtl/cray/cray-1x/xmp/`) are gone; the mode is rebuilt inside `func_top.v`.
2. Done. Console (CTRL-C) request raises F bit 32, the MCU flag.
3. Done, always on. Reference model of 1.2 as written (period II + 1). Tests:
   `tests/rtl_only/pclk.cal`, also run on the DE10-Nano.
4. Done, always on: 026ij1, 174ij1, 174ij2; 6-clock unit on a seventh tracker, busy shared
   with the reciprocal.
5. Not done (optional, COS does not need it).
6. Done for `XMP = 1` in simulation: four million words, I/O page at 17777760 octal.
   The MiSTer build is still the CRAY-1 with one million.
7. Done: package layout, DBA and DLA with all 19 bits, PS, 2-bit CLN, 0021 to 0027,
   IOR gate, zero for a read outside the field, 22-bit operand addresses, 24-bit P.
8. Done: cluster number, three clusters of SB, ST and SM, test and set with deadlock
   flag, WS and P at the instruction.
9. Done, in the manual's form (Q6 is still open for the COS run).
10. Not done: no channels. MCU request input exists.
11. Done.
12. Not done: needs the I/O processors.

Decisions taken on the open questions: Q2 II + 1, II = 0 requests every clock, dead start
clears enable and request, a set in the same clock as 0014j5 wins. Q3 pass. Q4 shared busy;
174ij3 to 7 reciprocal. Q5 29 bits (settled by Cray's diagnostic, see `fp-multiply.md`).
Q7 limit only, after a 22-bit address. Q9 the request is a level and is taken on leaving
monitor mode. Q11 two bits. Q12 jk[4:0]. Q13 the model stops; the RTL waits. Q14 CRAY-1 rule.
Q19 `VM Sj`.

Also found and fixed on the way: the 067 complement constant (198 - P, not 9 - P), an
instruction buffer answering for its old block while being refilled, and vector recursion
switched off for `XMP = 1` (the X-MP manual has none).

The reference model (`tools/crates/model`, `Machine::for_cpu(Cpu::Xmp)`), the assembler
(`MACHINE XMP`) and the tests (`tests/xmp`, `randprog.py --xmp`) cover the mode.

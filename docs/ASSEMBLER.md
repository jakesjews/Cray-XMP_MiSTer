# The assembler and test programs

The CPU's tests are programs in CAL, the Cray Assembly Language, assembled
here and run on the reference model and on the simulated CPU. Nothing of this
runs on the MiSTer: there the machine runs COS.

## The assembler

`cray-xmp asm` assembles a subset of CAL, the Cray Assembly Language, with the
instruction syntax of the manual's Appendix D, plus the forms of the 1982
options: `Ai QSj`, `Vi PVj`, `Vi QVj`, `PCI Sj`, `CCI`, `ECI` and `DCI`.
Build it with `make tools`.

```sh
tools/target/release/cray-xmp asm prog.cal -o prog.img -l prog.lst
```

- A line is `LABEL  RESULT  OPERAND  comment`. A `*` in column 1 is a comment.
- **Numbers are octal** unless written `D'10` or `X'A`.
- `'TEXT'` is a character constant. `DATA 'TEXT'` packs eight characters a word.
- A label on an instruction is a parcel address, a label on data a word address.
- Directives: `=`, `ORG`, `BSS`, `BSSZ`, `CON`, `DATA`, `ALIGN`, `VWD`,
  `INCLUDE`, `MACRO` with `LOCAL` and `ENDM`, `MACHINE`, `END`.
- Data and `ORG` start on a word boundary. As in CAL, the unused parcels of
  a word of code before them are filled with the no-op `S1 S1&S1`, so a
  program can run through a label such as `HERE BSS 0`.
- `ALIGN` goes on at the next multiple of 20 octal words, the size of an
  instruction buffer.
- Symbols cannot be register names such as `A1`, `S3` or `B77`.
- `cray-xmp isa` prints every instruction form the assembler accepts.

Things that differ from Cray's CAL: no relocation or linking, no conditional
assembly, no `LOC`. Two things to know about: an `ORG` back over earlier
code or data overwrites it without a message, as in CAL, and a branch to a
forward label minus a constant is reported as a phase error.

Remember the register conventions of the instruction set: in an operand,
register number 0 is a constant. `(A0)` reads as 0 as an index or as `Aj`, and
as 1 as `Ak`. `(S0)` reads as 0 as `Sj` and as 2^63 as `Sk`.

To run a program on the reference model:

```sh
tools/target/release/cray-xmp-run prog.img --input 'abc'
```

## Memory images

`cray-xmp asm -o` writes a memory image: raw 64-bit words from word 0, most
significant byte first. Parcel 0 of a word is its top 16 bits. The reference
model and the CPU's test bench load it and dead start: they exchange to the
package at word 0.

At the CRAY-1 setting words 0 to 17 are the exchange package the registers
are loaded from (manual page 3-37):

- word 0: P in bits 2^24 to 2^45, A0 in the low 24 bits
- word 1: BA from bit 2^28, A1
- word 2: LA from bit 2^28, the mode bits from 2^24 (2^24 is monitor mode), A2
- word 3: XA from bit 2^40, VL from 2^33, the flags from 2^24, A3. The flags
  from 2^24 up: normal exit, error exit, I/O, memory error, program range,
  operand range, floating-point error, MCU, programmable clock.
- words 4 to 7: A4 to A7
- words 10 to 17: S0 to S7

A program with the line `MACHINE XMP` is for the X-MP setting and has the
X-MP's package (`tests/rt/rt_xmp.cal`).

## The test page

A CRAY has no console on the CPU. So that a test program can print, read and
stop, the reference model and the CPU's test bench answer for the top 16 words
of memory themselves. The core has no such page. Word addresses, octal, at the
CRAY-1 setting; at the X-MP setting the page is at `17777760` on:

- `3777760` console status. Read: bit 0 an input character waits, bit 1
  output ready, bit 2 console interrupt enabled, bit 3 requested. Write:
  bit 0 enables the console interrupt and clears a request.
- `3777761` console data. Read takes the next input character. Write prints
  the low 8 bits.
- `3777762` test exit: the value written ends the run and is its exit code.
- `3777763` a free-running clock counter.

The console interrupt raises the MCU interrupt flag and stays until the
status word is written. The test bench asks for it where a test tells it to
(`--ctrl-c`).

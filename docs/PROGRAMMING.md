# Writing programs

## The monitor

The core starts in a small monitor. Numbers are octal.

- `E addr (count)` examine memory, eight words if no count is given
- `D addr value ...` deposit values from addr on
- `G addr` start a program at word addr
- `C` continue the program
- `R` show the program's P, VL, A and S registers
- `1` to `7` run a demonstration: sieve, Mandelbrot, SAXPY, matrix product,
  division, console echo, memory test
- `?` help

A program started with `G` or a number runs as a job: in user mode, with base
address 0 and the largest limit address, from its own exchange package at word
20. When it exits, causes an error, or CTRL-C is typed, the machine exchanges
back to the monitor, which says why and where:

```
NORMAL EXIT  P 0000504B
```

P is the word address and the parcel, A to D. `C` resumes from there.

The monitor uses B00 for its subroutine calls and does not save or restore the
B, T and V registers, VL or VM between jobs.

The monitor's source is `software/monitor/monitor.cal`; the demonstrations are
the other files in that directory.

## Memory images

The menu's "Load memory image" loads a file into memory from word 0 and dead
starts it. The file is raw 64-bit words, most significant byte first. Parcel 0
of a word is its top 16 bits.

Words 0 to 17 are the exchange package the registers are loaded from
(manual page 3-37):

- word 0: P in bits 2^24 to 2^45, A0 in the low 24 bits
- word 1: BA from bit 2^28, A1
- word 2: LA from bit 2^28, the mode bits from 2^24 (2^24 is monitor mode), A2
- word 3: XA from bit 2^40, VL from 2^33, the flags from 2^24, A3. The flags
  from 2^24 up: normal exit, error exit, I/O, memory error, program range,
  operand range, floating-point error, MCU (CTRL-C), programmable clock.
- words 4 to 7: A4 to A7
- words 10 to 17: S0 to S7

A loaded image replaces the monitor. Reset from the menu brings the monitor back.

`software/examples/hello.cal` is a complete image: a package, a program that
prints a line and then echoes the keyboard.

## The console

The console is two words at the top of memory, octal addresses:

- `3777760` status: bit 2^0 is set when a typed character waits, bit 2^1 when
  a character can be written
- `3777761` data: read a character, or write one

See [CPU.md](CPU.md) for the rest of the I/O page.

## The assembler

`cray1 asm` assembles a subset of CAL, the Cray Assembly Language, with the
instruction syntax of the manual's Appendix D, plus the forms of the 1982
options: `Ai QSj`, `Vi PVj`, `Vi QVj`, `PCI Sj`, `CCI`, `ECI` and `DCI`.
Build it with `make tools`.

```sh
tools/target/release/cray1 asm hello.cal -o hello.cry -l hello.lst
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
- `cray1 isa` prints every instruction form the assembler accepts.

Things that differ from Cray's CAL: no relocation or linking, no conditional
assembly, no `LOC`. Two things to know about: an `ORG` back over earlier
code or data overwrites it without a message, as in CAL, and a branch to a
forward label minus a constant is reported as a phase error.

Remember the register conventions of the instruction set: in an operand,
register number 0 is a constant. `(A0)` reads as 0 as an index or as `Aj`, and
as 1 as `Ak`. `(S0)` reads as 0 as `Sj` and as 2^63 as `Sk`.

To try a program before loading it:

```sh
tools/target/release/cray1-run hello.cry --input 'abc'
```

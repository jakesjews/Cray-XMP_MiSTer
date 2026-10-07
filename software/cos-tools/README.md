# Programs for COS

COS 1.17 as it was recovered has no assembler, no compiler and few of its
utility commands. Kevin Jordan's [COS-Tools](https://github.com/kej715/COS-Tools)
has them written anew, and they can be built to run under COS themselves.
These are those builds. [tools/py/mkcos.py](../../tools/py/mkcos.py) installs
them on the disk drive that goes on the SD card.

Commands:

- `CAL`: the assembler for the Cray Assembly Language. `CAL,I=source,L=0.`
  assembles the dataset `source` into `$BLD`; without `L=0` it writes a
  listing to `$OUT`.
- `LDR`: the loader. `LDR,AB,DN=$BLD.` makes the program `$ABD`;
  `LIB=a:b` names libraries.
- `LIB`: the library manager. `LIB,L=$OUT,name.` lists a library.
- `DASM`: a disassembler for programs `LDR` made.
- `KFTC`: a FORTRAN 77 compiler. `KFTC,I=source,O=code.` writes assembly
  language for `CAL,X,I=code.`; the program is linked with `IOLIB`, `INTFLIB`,
  `RTLIB`, `EMLIB`, `SYSLIB` and `CLIB`. It needs `MEMORY,FL,USER.` before it.
- `LISPF4`: the LISP F4 interpreter, a dialect of InterLisp. It reads `$IN`
  and wants the dataset `LISPSYS` local. `(ROLLIN (OPEN0 'LISPINI T T))` loads
  its library of functions, `DEFINEQ` among them.
- `COPYD`, `COPYF`, `COPYR`, `SKIPD`, `SKIPF`, `SKIPR`: copy or pass over
  datasets, files and records. `COPYF,O=name.` in a job takes the next file
  of the job's own deck.
- `NOTE`: writes a line of text to a dataset.
- `CHARGES`: what COS runs at the end of every job. Without it each job ends
  with an abort.

Libraries, as datasets: `IOLIB`, `INTFLIB` and `RTLIB` of the FORTRAN
compiler; `EMLIB`, `SYSLIB` and `CLIB` of the Amsterdam Compiler Kit, which
FORTRAN programs need too; `COSLIB`, COS-Tools' interface to the system; and
`BASLIB` and `PASLIB`, the kit's BASIC and Pascal libraries. The kit's
compilers for C, BASIC and Pascal are cross-compilers and run on another
computer; these libraries are what a program compiled there is linked with.

`LISPSYS` is the interpreter's table of atoms and `LISPINI` its library of
functions, both text.

Built from COS-Tools at commit `6600331` with the C compiler of
[the ACK fork for the X-MP](https://github.com/kej715/ack) at commit
`03b8233`, whose runtime is linked into the programs. Nothing in their sources
was changed. On macOS with gcc 16 and Lua 5.5 the ACK build itself needed two
things: `-std=gnu17` for its C, and in `h/em_table_lib.lua` loop variables
that are assigned to had to be given another name.

Licences:

- COS-Tools: Apache License 2.0 ([LICENSE-COS-Tools](LICENSE-COS-Tools)),
  copyright 2021 Kevin E. Jordan.
- The Amsterdam Compiler Kit: [LICENSE-ACK](LICENSE-ACK).
- LISP F4: [LICENSE-LISPF4](LICENSE-LISPF4), copyright 1984 Mats Nordstrom and
  others; taken into COS-Tools from [Blake McBride's copy](https://github.com/blakemcbride/LISPF4).
- [LICENSE-cray-sim](LICENSE-cray-sim) is the licence of the cray-sim project,
  for non-commercial use. The boot tape and the expander disk that the
  package for the SD card is made from are that project's, and its licence
  asks to be passed on with them.

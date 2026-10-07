# Programs for COS

COS 1.17 as it was recovered has no assembler or compiler. Kevin Jordan's
[COS-Tools](https://github.com/kej715/COS-Tools) has an assembler for the
Cray Assembly Language and a loader that can themselves be built to run under
COS. These are those builds, so that programs can be written on the machine.

- `CAL`: the assembler. `CAL,I=source,L=0.` assembles the dataset `source` into
  `$BLD`; without `L=0` it writes a listing to `$OUT`.
- `LDR`: the loader. `LDR,AB,DN=$BLD.` makes the program `$ABD`.
- `COPYF`: copies a file of a dataset. `COPYF,O=name.` in a job takes the next
  file of the job's own deck.

[tools/py/mkcos.py](../../tools/py/mkcos.py) puts them on the expander disk,
with the job `JTOOLS` that makes them commands of COS.

Built from COS-Tools at commit `6600331` with the C compiler of
[the ACK fork for the X-MP](https://github.com/kej715/ack) at commit
`03b8233`, whose runtime is linked into them. Nothing in their sources was
changed. On macOS with gcc 16 and Lua 5.5 the ACK build itself needed two
things: `-std=gnu17` for its C, and in `h/em_table_lib.lua` loop variables
that are assigned to had to be given another name.

COS-Tools is under the Apache License 2.0 ([LICENSE-COS-Tools](LICENSE-COS-Tools)),
copyright 2021 Kevin E. Jordan. The Amsterdam Compiler Kit is under the
licence in [LICENSE-ACK](LICENSE-ACK).

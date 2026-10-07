# CRAY X-MP for MiSTer

This core is a CRAY X-MP supercomputer running the Cray Operating System,
COS 1.17. You start the machine the way its operators did: answer the
questions on the operator's console, dead start COS, log on at the station
and submit batch jobs. You can also work with COS directly at the station:
write a program in its text editor, assemble it and run it.

## What you need

- A [MiSTer](https://mister-devel.github.io/MkDocs_MiSTer/) with a keyboard. No SDRAM module is needed.
- The COS 1.17 system of the [cray-sim](https://github.com/andrastantos/cray-sim) project: the folder with
  `boot_tape.tap`, `exp_disk.img`, nine `biop_dk*.img` files and `target/cos_117/iop_kern.bin`.
- Python 3 on your computer.
- 6 GB free where your MiSTer keeps its games. **The drive must not be FAT32.**

## Setup

1. Copy `CrayXMP_YYYYMMDD.rbf` from [releases](releases) to the `_Computer` folder on the SD card.
2. Run `python3 tools/py/mkcos.py "COS folder" out`.
3. Copy the folders `out/games` and `out/_Computer` onto the SD card.
4. Start **COS 1.17** from the Computer menu.

## Starting COS

1. Wait for `ENTER DATE [MM/DD/YY]` and type a date, such as `10/05/89`.
2. Type a time, such as `09:30:00`.
3. Type `START COS_117 DEADSTART` and wait for `START COMPLETE`.
4. Type `STATION`, then press **F2** to see the station.
5. Type `LOGON`.
6. Type `STMSG`. COS asks for configuration changes. Type `REPLY,0,GO`, with the number in
   front of the question if it is not 0.
7. Type `STMSG,I` to see how far start-up is. `+` shows the next page of the list.
8. When the list stops at `RE-READING $EFT`, type `STMSG`. COS reports
   `NO LABEL WAS FOUND ON DEVICE BMR-0-20`. Type `REPLY,9,CONTINUE`, with the number in
   front of the report if it is not 9.
9. Type `STMSG,I` again until the list ends with `STARTUP COMPLETE`.
10. Type `CLASS,ALL,ON`, then `LIMIT,5`, so that jobs can run.

**Commands are in capital letters.** Caps Lock is on when the core starts.

## Running a job

1. On the station type `SUBMIT,JTEST30`. It is a test job that comes with the system and ends
   in an abort.
2. Press **F3** to see what the printer prints.

The printout is also in `games/CrayXMP/printer.txt` on the SD card, after what was printed before.

`FSTAT` on the operator's console lists the files on the system's disk, `JTEST30` among them.

## Working at the station

COS also takes statements one at a time and answers on the screen.

1. On the station type `SUBMIT,JTOOLS` and wait for its printout (**F3**). It installs the
   text editor, the assembler and the loader.
2. On the operator's console (**F1**) type `IAIOP LOG`.
3. On the station (**F2**) type `IAC`, then `/LOGON`.
4. Type `ACCOUNT,AC=CRAY,APW=XYZZY,UPW=QUASAR.`
5. Type `AUDIT.` to list the permanent datasets.
6. `/LOGOFF` ends the session, and `/BYE` brings the station's own display back.

**Statements end with a full stop.**

## Writing a program

Programs are in the [Cray Assembly Language](http://www.bitsavers.org/pdf/cray/CAL/SR-2003_CAL_Assembler_Version_2_Feb86.pdf).

1. In a session type `TEDI.` for the text editor, and give the text a name, such as `PROG`.
2. Type `AL`, then the program below, then a line that is only a full stop.
   **A line without a label starts with a space.**
3. Type `END` to save the text and leave the editor.
4. Type `CAL,I=PROG,L=0.` to assemble it. It says how many errors it found.
5. Type `LDR,AB,DN=$BLD.` to link it, and `$ABD.` to run it.

```
         IDENT     HELLO
         ENTRY     HELLO
         START     HELLO
HELLO    S0        4
         S1        ='Hello from COS'Z
         S2        O'17
         EX
         S0        0
         EX
         END
```

To see where the errors are, assemble with `CAL,I=PROG,L=LST.` and look at `LST` in the
editor: `O` opens another text, `T 1,30` shows its first 30 lines, `?` lists the commands.

## Screens

- **F1** the operator's console
- **F2** the station
- **F3** the printer

Keys go to the station while it is shown, and to the operator's console otherwise.

## Menu options

- Reset starts the machine again; COS has to be dead started again.
- Aspect ratio
- Text color: white, green, amber or cyan
- Font: 8x16 (31 kHz) or 8x8 (15 kHz)

## Troubleshooting

- `Load a boot file from the menu to start it.`: start **COS 1.17** from the menu, not **CrayXMP**.
- `INVALID COMMAND`: the command was typed in small letters. Press Caps Lock.
- `CS009 - UNKNOWN VERB` for `TEDI.` or `CAL`: run `SUBMIT,JTOOLS` first.
- `CRAY HALT` during start-up after the disk images were replaced: start **COS 1.17** from the
  menu again instead of using Reset.
- `Concentrator ordinal 3  VAX interface select error. Command aborted.` a while after `START`:
  COS looks for a front-end computer, which this machine does not have. It runs without one.

How the machine is built, where it differs from a real one and how to work on the core are in [docs](docs).

## Credits

Thanks to the people whose work this core rests on:

- [Chris Fenton](https://www.chrisfenton.com/homebrew-cray-1a/) for the CRAY-1 in an FPGA that the CPU started from,
  and for [reading COS 1.17 off its disk pack](https://www.chrisfenton.com/cos-recovery/).
- [andrastantos](https://github.com/andrastantos) for bringing that disk back to life in [cray-sim](https://github.com/andrastantos/cray-sim),
  which the core was checked against, and for telling how in [The Cray Files](https://www.modularcircuits.com/blog/articles/the-cray-files/).
- [kej715](https://github.com/kej715) for [COS-Tools](https://github.com/kej715/COS-Tools) and the [compiler kit for COS](https://github.com/kej715/ack):
  the assembler and the loader on the disk.
- [davidgiven](https://github.com/davidgiven) and the Vrije Universiteit for the [Amsterdam Compiler Kit](https://github.com/davidgiven/ack), which those two are compiled with.
- [Zorislav-Shoyat](https://github.com/Zorislav-Shoyat) for [Cray-on-FPGA](https://github.com/Zorislav-Shoyat/Cray-on-FPGA) and the
  [CAL translator](https://github.com/Zorislav-Shoyat/CAL-Cray-Assembly-Language-Translator), a second opinion on the CPU and on the assembler.
- Robert Hyatt, Albert Gower and Harry Nelson for Cray Blitz, and [swenson](https://github.com/swenson) for
  [keeping its sources](https://github.com/swenson/cray-blitz): real Cray code to test with.
- [fvaneijk](https://github.com/fvaneijk) for the [VT52 core](https://github.com/MiSTer-devel/VT52_MiSTer), where the fonts and the video timing come from,
  and Dimitar Zhekov for the [Terminus Font](https://terminus-font.sourceforge.net/).
- [sorgelig](https://github.com/sorgelig) and the [MiSTer project](https://github.com/MiSTer-devel) for the framework.
- [Bitsavers](http://www.bitsavers.org/pdf/cray/) and [cray-history.net](https://cray-history.net/) for the Cray manuals.
- The [Verilator](https://www.veripool.org/verilator/) and [Verible](https://github.com/chipsalliance/verible) projects for the simulator and the formatter.

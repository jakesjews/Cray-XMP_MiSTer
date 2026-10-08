# [CRAY X-MP](https://en.wikipedia.org/wiki/Cray_X-MP) for MiSTer

This core is a CRAY X-MP supercomputer running the Cray Operating System,
COS 1.17. You start the machine the way its operators did: answer the
questions on the operator's console, dead start COS, log on at the station
and submit batch jobs. You can also work with COS directly at the station:
write a program in its text editor, assemble or compile it and run it. If the
OS ever asks for a year make sure it is before 2000.

## What you need

- A [MiSTer](https://mister-devel.github.io/MkDocs_MiSTer/) with a keyboard. No SDRAM module is needed.
- 700 MB free on the SD card.

## Setup

1. Copy `Cray-XMP_YYYYMMDD.rbf` from [releases](releases) to the `_Computer` folder on the SD card.
2. Unpack `Cray-XMP_COS-1.17.zip` from [releases](releases) onto the SD card, so that its `games`
   and `_Computer` folders go into the card's.
3. Start **COS 1.17** from the Computer menu.

## Starting COS

1. Wait for `ENTER DATE [MM/DD/YY]` and type a date, such as `10/05/89`. **The year must be 80 to 99.**
2. Type a time, such as `09:30:00`.
3. Type `START COS_117 DEADSTART` and wait for `START COMPLETE`.
4. Type `STATION`, then press **F2** to see the station.
5. Type `LOGON`.
6. Type `STMSG`. COS asks for configuration changes. Type `REPLY,0,GO`.
7. Type `STMSG,I` to see how far start-up is, until the list ends with `STARTUP COMPLETE`.
8. Type `CLASS,ALL,ON`, then `LIMIT,5`, so that jobs can run.

Commands are in capital letters. Caps Lock is on when the core starts.

## Running a job

1. On the station type `SUBMIT,JFTN`. The job compiles a small FORTRAN program and runs it.
2. Press **F3** to see what the printer prints.

`SUBMIT,JCAL` does the same with a program in assembly language, and `SUBMIT,JLISP` runs a few
lines of LISP. The printout is also in `games/Cray-XMP/printer.txt` on the SD card, after what
was printed before. `FSTAT` on the operator's console lists the files the jobs come from.

## Working at the station

COS also takes statements one at a time and answers on the screen.

1. On the operator's console (**F1**) type `IAIOP LOG`.
2. On the station (**F2**) type `IAC`, then `/LOGON`.
3. Type `ACCOUNT,AC=CRAY,US=SYSTEM.`
4. Type `AUDIT.` to list the permanent datasets.
5. `/LOGOFF` ends the session, and `/BYE` brings the station's own display back.

**Statements end with a full stop.**

## Writing a program

In assembly language, the [Cray Assembly Language](http://www.bitsavers.org/pdf/cray/CAL/SR-2003_CAL_Assembler_Version_2_Feb86.pdf):

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

In FORTRAN 77, which runs as a job and prints on the printer:

1. Type `TEDI.` and name the text `SRC`. If the editor opens the last text instead of asking
   for a name, type `O` first.
2. Type `AL`, then the program below, then a line that is only a full stop.
   Statements start in column 7 and labels in columns 1 to 5.
3. Type `END`, then `SAVE,DN=SRC.` to keep the text.
4. Type `ACCESS,DN=FTN.`, then `SUBMIT,DN=FTN.` The job `FTN` compiles `SRC` and runs it.
5. Press **F3** for the printout.

```
      PROGRAM SQ
      INTEGER I
      DO 10 I = 1, 5
        PRINT 100, I, I * I
   10 CONTINUE
  100 FORMAT(1X, I4, I6)
      END
```

The first character of a printed line is not printed: it tells the printer how far to
advance, so start a `FORMAT` with `1X`. All the commands are listed in
[software/cos-tools](software/cos-tools/README.md).

## Screens

- **F1** the operator's console
- **F2** the station
- **F3** the printer

Keys go to the station while it is shown, and to the operator's console otherwise.

## Menu options

- Reset starts the machine again; COS has to be dead started again.
- Aspect ratio (with the custom ratios of `MiSTer.ini`), Scale and Scandoubler Fx work as in
  other cores. The scandoubler is for the 8x8 font.
- Text color: white, green, amber or cyan
- Font: 8x16 (31 kHz) or 8x8 (15 kHz)
- Disk drives: Fast, or As a DD-29 with the seek and rotation times of the real drive

## Troubleshooting

- `Load a boot file from the menu to start it.`: start **COS 1.17** from the menu, not **Cray-XMP**.
- `INVALID COMMAND`: the command was typed in small letters. Press Caps Lock.
- `ENTER DATE` comes back: the year must be 80 to 99.
- `PD009 - DATASET NOT FOUND` for a dataset you saved in a job: put `US=SYSTEM` into the job's
  `ACCOUNT` statement, as above.
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
  the assembler, the loader, the FORTRAN compiler and the other commands and libraries on the disk.
- [davidgiven](https://github.com/davidgiven) and the Vrije Universiteit for the [Amsterdam Compiler Kit](https://github.com/davidgiven/ack), which those are compiled with.
- Mats Nordström and his colleagues at Uppsala for LISP F4, and [blakemcbride](https://github.com/blakemcbride) for [keeping it](https://github.com/blakemcbride/LISPF4).
- [Zorislav-Shoyat](https://github.com/Zorislav-Shoyat) for [Cray-on-FPGA](https://github.com/Zorislav-Shoyat/Cray-on-FPGA) and the
  [CAL translator](https://github.com/Zorislav-Shoyat/CAL-Cray-Assembly-Language-Translator), a second opinion on the CPU and on the assembler.
- Robert Hyatt, Albert Gower and Harry Nelson for Cray Blitz, and [swenson](https://github.com/swenson) for
  [keeping its sources](https://github.com/swenson/cray-blitz): real Cray code to test with.
- [fvaneijk](https://github.com/fvaneijk) for the [VT52 core](https://github.com/MiSTer-devel/VT52_MiSTer), where the fonts and the video timing come from,
  and Dimitar Zhekov for the [Terminus Font](https://terminus-font.sourceforge.net/).
- [sorgelig](https://github.com/sorgelig) and the [MiSTer project](https://github.com/MiSTer-devel) for the framework.
- [Bitsavers](http://www.bitsavers.org/pdf/cray/) and [cray-history.net](https://cray-history.net/) for the Cray manuals.
- The [Verilator](https://www.veripool.org/verilator/) and [Verible](https://github.com/chipsalliance/verible) projects for the simulator and the formatter.
// weave: run 'weave explain Readme.md' for per-hunk detail, 'weave check' to verify your resolution

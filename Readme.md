# CRAY X-MP for MiSTer

This core is a CRAY X-MP supercomputer with its I/O Subsystem, and it runs the
Cray Operating System, COS 1.17. You start the machine the way its operators
did: answer the I/O Subsystem at the operator's console, dead start COS, log
on at the station and submit batch jobs. What the jobs print goes to a text
file on the SD card.

## What you need

- A [MiSTer](https://mister-devel.github.io/MkDocs_MiSTer/) (DE10-Nano) with a keyboard. No SDRAM module is needed.
- The COS 1.17 system as the [cray-sim](https://github.com/andrastantos/cray-sim) project runs it: a folder with
  `boot_tape.tap`, `exp_disk.img`, nine `biop_dk*.img` files and `target/cos_117/iop_kern.bin`.
- Python 3 on your computer.
- 6 GB free where your MiSTer keeps its games, on a file system other than FAT32.

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
6. Type `STMSG`. When COS asks for configuration changes, type `REPLY,0,GO`.
7. Type `STMSG,I` after a few seconds to see how far start-up is. It ends with `STARTUP COMPLETE`.
   Type `+` for the next page of the list.
8. If the list stops at `RE-READING $EFT` for device `BMR-0-20`, type `STMSG`. It shows
   `NO LABEL WAS FOUND ON DEVICE BMR-0-20`; type `REPLY,10,CONTINUE`.

**Commands are in capital letters.** Caps Lock is on when the core starts.

## Running a job

1. On the station type `CLASS,ALL,ON`, then `LIMIT,5`.
2. Type `SUBMIT,JTEST30`. It is a test job that comes with the system and ends in an abort.
3. Press **F3** to see what the printer prints.

The printout is also in `games/CrayXMP/printer.txt` on the SD card. New
printing goes behind what is already there.

`FSTAT` on the operator's console lists the files on the system's disk, `JTEST30` among them.

## Screens

- **F1** the operator's console
- **F2** the station
- **F3** the printer

Keys go to the station while it is shown, and to the operator's console otherwise.

## Menu options

- Reset starts the machine again; COS has to be dead started again.
- Aspect ratio
- Text color: white, green, amber or cyan
- Font: 8x16 on a 31 kHz raster, or 8x8 on a 15 kHz raster

## Troubleshooting

- `Load a boot file from the menu to start it.`: the core was started without its files. Start **COS 1.17**, not **CrayXMP**.
- `INVALID COMMAND`: the command was typed in small letters. Press Caps Lock.
- `Concentrator ordinal 3  VAX interface select error. Command aborted.` a while after `START`:
  there is no front-end computer. COS runs without one.

What the machine is made of, where it differs from a real one and how to work
on the core are in [docs](docs).

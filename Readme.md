# CRAY X-MP for MiSTer

This core is a CRAY X-MP supercomputer with its I/O Subsystem, and it runs the
Cray Operating System, COS 1.17. You start the machine the way its operators
did: answer the I/O Subsystem at the operator's console, dead start COS, and
log on at the station.

## What you need

- A [MiSTer](https://mister-devel.github.io/MkDocs_MiSTer/) (DE10-Nano) with a keyboard. No SDRAM module is needed.
- The COS 1.17 system as the [cray-sim](https://github.com/andrastantos/cray-sim) project runs it: a folder with
  `boot_tape.tap`, `exp_disk.img`, nine `biop_dk*.img` files and `target/cos_117/iop_kern.bin`.
- Python 3 on your computer.
- 6 GB free where your MiSTer keeps its games, on a file system other than FAT32.

## Setup

1. Build the core and copy `output_files/CrayXMP.rbf` to `_Computer` on the SD card: see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
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
7. Type `STMSG` again. If it shows `NO LABEL WAS FOUND ON DEVICE BMR-0-20`, type `REPLY,10,CONTINUE`.
8. Type `STMSG,I` to watch start-up finish.

**F1** shows the operator's console again.

**Commands are in capital letters.** Caps Lock is on when the core starts.

## Menu options

- Aspect ratio
- Text color: white, green, amber or cyan
- Font: 8x16 on a 31 kHz raster, or 8x8 on a 15 kHz raster

## Troubleshooting

- `INVALID COMMAND`: the command was typed in small letters. Press Caps Lock.
- `Concentrator ordinal 3  VAX interface select error. Command aborted.` a while after `START`:
  there is no front-end computer. COS runs without one.

What the machine is made of, where it differs from a real one and how to work
on the core are in [docs](docs).

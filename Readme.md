# Cray-1 for MiSTer

This core is a Cray-1 supercomputer: the CPU with its vector registers, a
million words of memory, and a text console on your display and keyboard. It
starts in a small monitor where you can look at memory, run the built-in
demonstration programs and start programs of your own. There is no Cray
operating system for it; the machine runs one program at a time under the monitor.

## What you need

- A [MiSTer](https://mister-devel.github.io/MkDocs_MiSTer/) (DE10-Nano). No SDRAM module is needed.
- A keyboard.

## Setup

1. Copy `Cray1_YYYYMMDD.rbf` from [releases](releases) to the `_Computer` folder on the SD card.
2. Start **Cray1** from the Computer menu.
3. Type `?` and press Enter.

## The monitor

- `1` to `7` run the demonstrations: prime sieve, Mandelbrot set, SAXPY timed
  with and without the vector registers, matrix product, division, console
  echo, memory test.
- `E addr (count)` shows memory. `D addr value ...` changes it.
- `G addr` starts a program. `C` continues it. `R` shows its registers.
- **Numbers are octal**, as on the real machine.
- CTRL-C stops a running program and returns to the monitor.

## Your own programs

1. Build the assembler and write a program: see [docs/PROGRAMMING.md](docs/PROGRAMMING.md).
2. Assemble it into a memory image with the extension `.cry`.
3. Copy the image to `games/Cray1` on the SD card.
4. Choose **Load memory image** in the core's menu.

A loaded image starts at once and replaces the monitor. **Reset** in the menu
brings the monitor back.

## Menu options

- Aspect ratio
- Text color: white, green, amber or cyan
- Font: 8x16 on a 31 kHz raster, or 8x8 on a 15 kHz raster

## Troubleshooting

- `WHAT?`: the monitor did not know the command. Commands are one letter or digit.
- An address shows unexpected contents: check that the number was typed in octal.
- A program does not stop on CTRL-C: use Reset in the menu.

How the machine is built, where it differs from a real Cray-1 and how to work
on the core are in [docs](docs).

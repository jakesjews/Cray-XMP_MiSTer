# Development

How to build, simulate, test, format and lint the core. For what the machine
is made of and where it differs from a real one, see [MACHINE.md](MACHINE.md).

## Layout

- `Cray-XMP.sv`: the MiSTer `emu` wrapper. `files.qip` lists what Quartus builds.
- `rtl/xmp_machine.v`: the machine. The CPU with the X-MP features and the I/O
  Subsystem, joined by the channel pair between them.
- `rtl/cray/`: CPU modules written for this core.
- `rtl/cray/cray-1x/`: CPU modules that started in the cray-1x project. That
  project is no longer maintained, so they are maintained here like the rest.
- `rtl/ios/`: the I/O Subsystem. Three I/O Processors with their Local
  Memories, clocks, Buffer Memory channels and the channels between them; the
  consoles; the Peripheral Expander with its tape, disk and printer; the
  BIOP's disk drives and its channel into central memory; the MIOP's channel
  pair to the mainframe.
- `rtl/mister/`: what joins the machine to the MiSTer. The DDR3 port and its
  sharing, the check and copy of the boot file, the consoles on the serial
  port, the printer's file, clock domain crossings.
- `rtl/terminal/`: the screens, the keyboard and the video.
- `sys/`: the MiSTer framework, an unmodified copy of
  [Template_MiSTer](https://github.com/MiSTer-devel/Template_MiSTer).
- `sim/`: Verilator simulations. `tools/`: assembler, reference models, scripts.
- `tests/`: test programs.
- `software/cos-1.17/`: COS 1.17 as it was recovered: the I/O Subsystem's
  kernel, its boot tape and the expander disk.
- `software/cos-tools/`: the programs and libraries that are installed on the
  drive of the package for the SD card, built to run under COS.
- `docs/spec/`: the specifications the models and the hardware description
  were written from.
- `releases/`: the core and that package.
- `lint/`: Verilator file list, waivers and a PLL stub for `make lint`.

`make` lists the commands below.

## Building the core

Quartus Prime 17.0 Lite builds the core. On Windows or Linux open
`Cray-XMP.qpf` in it and compile, or run `quartus_sh --flow compile Cray-XMP`;
the result is `output_files/Cray-XMP.rbf`. On macOS `build.sh` runs Quartus
through a CrossOver bottle named `Quartus` (the header of the script has the
environment variables for another bottle or another place):

```sh
./build.sh map        # analysis and synthesis only
./build.sh compile    # full flow, writes output_files/Cray-XMP.rbf
```

`compile` exits with status 2 if timing is not met.

Everything else needs a Rust toolchain (`cargo`), Verilator 5, Python 3.9 or
later and GNU Make; nothing is fetched from outside the repository except
Rust's own crates, of which the tools use none.

**Do not edit `files.qip`, `Cray-XMP.qsf` or any RTL while a build runs.** Quartus
stops with "Settings File changed outside of the Quartus Prime software" and
rewrites `Cray-XMP.qsf` as one large flattened file. If that happens, restore
`Cray-XMP.qsf` to its 78-line form before building again.

A memory that both of its ports write must carry `(* ramstyle = "no_rw_check" *)`,
or Quartus builds it from flip-flops; the first synthesis of this core needed
75,000 logic modules for that reason. The attribute promises that nothing
reads a cell in the clock it is written in. `RW_POISON` checks the promise:
see "Simulations".

## Host tools

A Rust toolchain builds the assembler and the reference model:

```sh
make tools
tools/target/release/cray-xmp asm prog.cal -o prog.img -l prog.lst
tools/target/release/cray-xmp dis prog.img
tools/target/release/cray-xmp isa                 # the instruction table
tools/target/release/cray-xmp-run prog.img --input 'text\r' --max 1000000
```

`cray-xmp-run` is the reference model of the CPU: the CPU of a one-processor
CRAY X-MP at instruction level, written from the hardware reference manuals,
independent of the RTL. The programs it runs are test programs: they print
and stop through a page of memory that only the model and the CPU's test
bench have ([ASSEMBLER.md](ASSEMBLER.md)). It keeps track of values a program
has no right to rely on, such as registers at power-up or the real-time
clock, and stops if one decides a branch, an address or console output.

### The system model

`cray-xmp-sys` joins that CPU model to a model of the
I/O Subsystem: three I/O Processors with their channels, Buffer Memory, the
Peripheral Expander with its tape, disk and printer, nine DD-29 disk drives,
the consoles, and the two links to the mainframe. It runs the I/O Subsystem's
own software, and through it COS 1.17. The hardware description was written
from this model and is checked against it.

`cray-xmp-sys` takes the directory of the ready-to-run COS 1.17 system of the
cray-sim project (the IOP kernel, the boot tape, the expander disk and the
drive images) and never writes to it; a channel whose drive image is missing
has no drive. `--save DIR` writes the disks that were written to into another
directory at the end. What the operator types comes from a script:

```sh
tools/target/release/cray-xmp-sys DIRECTORY --script tests/sys/cos.script
```

`tests/sys/cos.script` gives the date, dead starts COS, logs the station on,
answers the start-up questions and submits a batch job. The kernel console
is copied to the terminal; a `screen` step prints a console as the 24 lines
an operator would see. `--log FILE` writes every channel function of every
I/O Processor. The header of `tools/crates/ios/src/bin/cray-xmp-sys.rs` lists
the script steps and the options, among them the timing of the devices.

The tests that run this software look for it in the directory the
environment variable `CRAY_XMP_SYSTEM` names: the ready-to-run COS 1.17 of
the cray-sim project after its install, with the nine drive images. Without
it they are left out. `cargo test` in `tools/` then boots the kernel as far
as its first question, and `make test` runs the script above.

## The package for the SD card

`releases/Cray-XMP_COS-1.17.zip` is made by the system model:

```sh
make tools
python3 tools/py/mkcos.py out --zip releases/Cray-XMP_COS-1.17.zip
```

It starts from the kernel, the boot tape and the expander disk in
[software/cos-1.17](../software/cos-1.17/README.md). The script
gives the model an empty drive and has COS install itself on it
(`START COS_117 INSTALL`), starts COS again and runs one job that saves the
programs and libraries of [software/cos-tools](../software/cos-tools/README.md)
on the drive and enters the programs as commands, then starts COS from the
finished files and runs the three example jobs, whose printout it checks. It
takes about eight minutes. The header of the script says what is changed in
the software and why: one drive instead of nine, and which user the jobs
run as.

## Simulations

With Verilator 5 on `PATH`:

```sh
make sim
sim/build/cpu/Vcray_cpu --image prog.img --mem ddr3 --input 'text\r'
sim/build/fp/Vfp_tb tests/fp/xmp_ref.vec
sim/build/core/Vemu BOOTFILE --disk 0=exp_disk.img --until 'ENTER DATE'
```

- `Vcray_cpu` is the CPU with a memory model of four million words, and with
  each output channel cabled to the input channel of its pair. `--mem` picks
  the memory timing: `0`, `fixed:N`, `rand:A-B`, `ddr3` or `slow`. `--step`
  holds each instruction until the one before has finished.
- `Vfp_tb` checks the floating-point units against vector files.
- `sim/build/iop/Viop_cpu RECORD` is the I/O Processor following a record of
  the reference model's steps; `tools/py/ioptest.py` makes the records.
- `sim/build/ios/Vios KERNEL TAPE [DISK]` is the I/O Subsystem
  (`rtl/ios/ios.v`) booting its kernel. The test bench is Buffer Memory,
  central memory, the file that is the tape, and the sectors of the expander
  disk and of the nine drives (`--drive N=FILE`), which it serves the way the
  MiSTer framework does. `--type TEXT=KEYS` presses keys on a console when it
  has shown TEXT.
- `sim/build/xmp/Vxmp_machine` takes the same arguments and is the whole
  machine (`rtl/xmp_machine.v`), CPU included, with central memory served by
  the test bench. `--quick` leaves out the tests of memory and most of the
  BIOP's test of its drives.
- `sim/build/core/Vemu` is the core as the MiSTer runs it (`Cray-XMP.sv`), with
  stand-ins for `hps_io`, the PLL, DDR3 and the framework's `video_mixer` and
  `video_freak`, which pass the picture through. It loads a boot file into memory
  that is otherwise full of junk, serves the image slots, types on the serial
  port (`--type`) or on the keyboard (`--press`, with `{f1}` to `{f3}`), reads
  the serial port and the video, presses the menu's reset, and at the end
  compares the screens in the core with what the serial port carried. Run it
  from the top of the source tree; the header of `sim/harness/core_main.cpp`
  lists the options.
- `sim/build/ampex/Vterm_ampex_tb` and `sim/build/spool/Vprint_spool` are the
  benches of the terminal and of the printer's file.

Two of the benches take about a minute of real time for a second of the
machine's; `--quick` runs, where COS is started 2.4 seconds after the reset,
are the practical ones.

To check that no block memory is read in the clock it is written in, build a
bench with `+define+RW_POISON` and run its tests: such a read then returns
the wrong value.

```sh
verilator -f sim/ios.vc +define+RW_POISON --Mdir sim/build/ios_poison -o Vios
CRAY_IOS_SIM=sim/build/ios_poison/Vios python3 tools/py/ioptest.py selftest
```

## Tests

```sh
make test-quick    # about a minute
make test          # the same, then the long runs
```

`tools/py/runtests.py` lists what each runs. The long runs include COS 1.17 on
the system model and on the hardware description if the software is there
(see "The system model").

### The CPU

A test passes when the RTL and the reference model end with the same memory
contents, console output and exit code. Each program runs five ways on the
RTL: fast memory, random memory delays, slow memory, one instruction at a
time, and a second random seed. All five must agree with the model.

- `tests/smoke/` are short programs for each area of the machine. Each runs
  under up to four start-ups from `tests/rt/`: direct, through an exchange, in
  user mode, and in user mode at a non-zero base address.
- `tests/directed/` are the bring-up tests for exchange, floating point and
  vectors; `vload.cal`, vector loads at every alignment, step and length
  (written by `gen_vload.py`); the instruction buffer tests `ibuf.cal`,
  `ibufhalf.cal` and `ibufhop.cal` (written by `gen_ibuf.py`); and the tests
  of what the X-MP added: `xpkg.cal` for the exchange package and the two
  fields, `shared.cal` for the shared registers, semaphores and status
  register, `channels.cal` for the 6 Mbyte channels, `vlah.cal` for the VL
  register and the 24-bit constant, `imm.cal` for interrupt monitor mode,
  `vnu.cal` for the VNU and ESVL bits of the package, `gather.cal` for the
  gather, the scatter and the compress index.
- `tests/rtl_only/` holds programs that check themselves, for what the model
  cannot predict: the real-time clock, the programmable clock and its
  interrupt, the console interrupt, and how many clock periods instructions
  take (`timing.cal`, and `timing_esvl.cal` with the second vector logical
  unit enabled, both written by `gen_timing.py`). They run on the RTL
  alone. A line `* NOSTEP` keeps a program out of the run that holds issue
  to one instruction at a time. A line
  `* SIM: arguments` in such a program gives the simulator more arguments;
  `--ctrl-c 30000,400000` makes the console ask for its interrupt in those
  clocks. `tests/rt/rt_user.cal` has the macros these tests use to send off
  short user programs and look at the flags they come back with.
- `tools/py/randprog.py` writes random programs, shared registers and
  semaphores included. `tools/py/difftest.py rand FIRST LAST` runs a range of
  seeds and keeps failing cases in `build/diff`. With `--chain` the programs
  are mostly vector instructions on a few V registers, so that operations
  take their operands from results, loads and stores still on their way.
- `tests/fp/xmp_ref.vec` holds 79 floating-point cases whose results come from
  the cray-sim project's test program.

Random programs do not use the exits, the monitor instructions, channel
status or the clocks, and they index memory through A0 to A5 and A7.
The smoke tests cover exits, exchanges and range errors.

### The I/O Subsystem

`rtl/ios/iop_cpu.v` is checked against the model `tools/crates/ios/src/iop.rs`
step by step. The model runs a program and writes a record: Local Memory,
then for every instruction or interrupt what its channels answered and what
the registers held afterwards (`replay.rs` has the format). The simulation
is given the same memory and the same answers, and must take the same
interrupts, send the same functions, end every step with the same registers,
and finish with the same memory, operand registers and exit stack.

```sh
python3 tools/py/ioptest.py rand 1 200     # random programs, two kinds, and a directed one
python3 tools/py/ioptest.py kernel         # the real kernel's boot on each of the three processors
```

- A random program is either all random parcels, which reaches every
  operation code, or mostly register work with functions on the processor's
  own channels and frequent interrupts. Half way through each there is a
  Master Clear and a second dead start.
- `kernel` needs the COS 1.17 software (see "The system model"). The system
  model boots it and the simulation follows one processor: 150 million steps
  for the MIOP, 55 million each for the other two.
- `python3 tools/py/ioptest.py boot` runs the three processors together as
  hardware, each with its Local Memory, real-time clock, Buffer Memory channel,
  its channels to the others and its console, and the MIOP with its
  Peripheral Expander. The MIOP is dead started from Buffer Memory, loads its
  overlays from the tape, dead starts the BIOP and the XIOP and asks for the
  date and the time, which the test types; the BIOP and the XIOP say who they
  are on their own consoles. The test then types `FSTAT`, and the kernel
  lists the files on the expander disk as the system model lists them.
  Meanwhile the BIOP runs its own test of its nine drives and must have
  nothing to report.
- `python3 tools/py/ioptest.py machine` runs the whole machine: the operator
  types `START COS_117 DEADSTART`, and the kernel checks the mainframe. It
  holds the CPU with Master Clear, loads a test program into central memory
  through the BIOP, lets the CPU go, takes a word from it over the channel
  pair and reports `MFINIT: COMPLETE`. With `--start` the run goes on: COS
  is loaded from the expander disk and started, and the kernel reports the
  linkage with it and `START COMPLETE`. That is about two minutes.
- `python3 tools/py/ioptest.py selftest` runs `tests/ios/selftest.py`, a
  program for the I/O Processors that takes the kernel's place, on the system
  model and on the same hardware. It checks what the kernel's start does not
  depend on: the real-time clock, the order in which channels that ask for an
  interrupt are reported, the expander's tape and disk, a drive of the BIOP
  and its channel into central memory, that a word from each processor
  reaches each other one, and the printer. Each processor writes its verdict
  on its console, and what was printed must be the same on both. A second
  run is on the whole machine: the program puts a few instructions into
  central memory, lets the CPU go, exchanges parcels with it over the channel
  pair and tries I/O Master Clear on its own.
- `python3 tools/py/ioptest.py bridges` checks `rtl/xmp_bridge.v` by itself,
  which carries pulses, levels and memory requests between the CPU's clock
  and the I/O Subsystem's: clocks of random periods and the machine's own,
  requests that are taken back before their acknowledge, and resets in the
  middle. `sim/harness/bridge_main.cpp` says what must hold.

### The core

`tools/py/coretest.py` runs the core-level simulation and the two benches
beside it.

- `screens`: the terminal against the model's, on random character streams.
- `spool`: the printer's file. Random printing into files of random lengths,
  part of them used by an earlier session, and what the files hold afterwards.
- `nofile`: the core without a boot file says so on the screen.
- `printer`: the self-checking program as the boot file. It reports on the
  operator's console, and what it prints is on the printer's screen and in
  the printer's file.
- `boot`: the kernel boots from a boot file; date and time are typed on the
  keyboard; the menu's reset makes it boot again.
- `start`: COS is loaded and started, and the station is logged on from the
  keyboard after F2.
- `restart`: the menu's reset while the CPU runs. The kernel comes up again
  and the CPU waits for it.

Every run fails if the CPU runs before `START` has been typed.

## Files on the expander disk

The disk on the Peripheral Expander is where the station gets jobs and
programs from, and `tools/py/expdisk.py` reads and writes its file system:

```sh
python3 tools/py/expdisk.py list exp_disk.img
python3 tools/py/expdisk.py put exp_disk.img BIN/HELLO hello.abs      # a program from COS-Tools' ldr or from ack
python3 tools/py/expdisk.py puttext exp_disk.img STATION/JHELLO job.txt   # a job: a record a line, /EOF between files
python3 tools/py/expdisk.py gettext exp_disk.img STATION/JTEST30 jtest30.txt
```

On the machine, `SUBMIT,JHELLO` at the station queues the job, and a job or
an interactive session gets a file with `FETCH,DN=HELLO,MF=AP,TEXT=BIN/HELLO.`
and runs it with `HELLO.` The script's header has the layout of the disk and
of a dataset. Taking every dataset of the COS 1.17 disk apart and putting it
together again gives the same bytes. The directory holds 34 files, of which
the disk as it was recovered uses 26 and the one in the package 29.

This is the way to bring in a program compiled elsewhere, with
[the ACK for the X-MP](https://github.com/kej715/ack) for instance, or a job
with its own deck. What a program built with that kit's runtime writes to
`$OUT` is printed when it runs in a job; in an interactive session it does
not reach the screen, which is why FORTRAN and LISP are run as jobs.

## Testing on a MiSTer

`tools/py/hil.py` drives a MiSTer over SSH (`MISTER`, default `root@mister`;
`MISTER_PW`, default `1`). It needs `sshpass`. Unpack the package for the SD
card on the MiSTer once.

```sh
python3 tools/py/hil.py deploy                  # copy the core
python3 tools/py/hil.py start cos117.ios -t 60 \
    --type 'ENTER DATE [MM/DD/YY]=10/05/89\r' \
    --type 'ENTER TIME [HH:MM:SS]=01:02:03\r' \
    --type '10/05/89  01:02:03=START COS_117 DEADSTART\r' \
    --until 'START COMPLETE'
python3 tools/py/hil.py session -t 20 --type '+500=STATION\r' --type '@0:CRAY STATION=LOGON\r' --screen 0
python3 tools/py/hil.py keys '{f2}stmsg\r'      # type on a virtual keyboard
python3 tools/py/hil.py printed                 # what the printer's file holds
```

Both consoles are on the HPS serial port, `/dev/ttyS1` at 115200 baud: bit 7
of a byte is clear for the operator's console and set for the station.
`start` and `session` wait for texts and type answers the way the benches do,
and print a console as its 24 lines with `--screen` (0 the station, 3 the
operator's). A serial BREAK resets the machine as the menu's reset does;
`session --break` sends one.

For a start as after a power cycle, stop the machine first (`hil.py session
--break`), then put a fresh `drives.img` in place and load the core again,
which clears the memories. A COS that is still running writes to the drive
again, and a start on a fresh drive with the old memory ends in `CRAY HALT`.

Screenshots need direct video off: `hil.py direct-video off`, `hil.py shot out.png`,
`hil.py direct-video on`.

## Formatting

With `verible-verilog-format`, Python 3.9+ and Git on `PATH`, run from the
repository root:

```sh
make format-check                       # read-only; fails if formatting differs
make format                             # apply formatting
make format FILES='rtl/xmp_machine.v'
```

The same operations are available through
`python3 tools/py/verible.py format|format-check [files...]`. `format-check`
returns nonzero when formatting differs; both return nonzero on tool errors.

The project scope is the `Cray-XMP.sv` wrapper and the Verilog and SystemVerilog
sources under `rtl/` and `sim/`, including new, untracked files. Ignored files
are excluded.

Vendored files are never formatted:

- `sys/`, the MiSTer framework
- `rtl/pll.v` and `rtl/pll/`, generated IP

Naming one of them is rejected: `make format FILES='sys/hps_io.sv'` changes
nothing.

The configuration is the one the Apple III core uses: tab indentation,
aligned declarations, ports and assignments, a 120-column target, settings in
`.verible-format.flags`. Verible emits spaces, so the wrapper expands leading
tabs at four-column stops before formatting and restores them afterwards. It
protects strings, comments and `verilog_format: off/on` regions. `.editorconfig`
uses the same tab width. Validated with Verible `v0.0-4219-g3275ab72`.

## Linting

With Verilator on `PATH`:

```sh
make lint
```

This writes a fixed build ID to `lint/gen/` and runs Verilator with
`--lint-only -Wall -f lint/rtl.f` on the `emu` top, as built for the MiSTer.

`lint/rtl.f` lists the sources from `files.qip`; update both when adding a
synthesis source. `.v` files are parsed as Verilog 2005, as Quartus does.

Warnings are fatal. The policy and the waivers are in `lint/exclusions.vlt`:

- File naming, shadowing, declaration initialisers and explicitly empty port
  connections are not checked anywhere.
- `sys/` and the PLL stub are loaded so that connections from project RTL are
  checked, but their own diagnostics are suppressed.
- Everything else, the cray-1x sources included, has waivers only for
  reviewed cases, each with its reason: the `hps_io` ports this core does not
  use, named one by one; bits the floating-point arithmetic forms and then
  drops; ports and instruction fields a module takes but does not need.

Lint uses a port-only PLL stub, so it does not check Intel primitives or
timing. Validated with Verilator 5.052.

The host tools are not covered by `make lint` or `make format`.

## Proving that an edit changes nothing

With Yosys on `PATH`:

```sh
python3 tools/py/equiv.py                 # every RTL file that differs from HEAD
python3 tools/py/equiv.py --rev HEAD~1 rtl/cray/cray-1x/func_top.v
```

For each module in a changed file, Yosys compares the working tree with the
named revision: the same outputs, the same next state of every register and
the same values sent to every submodule, for all inputs. Use it after
reformatting or a lint clean-up, to cover what the simulations do not reach.

It needs ports, registers and submodule instances to keep their names, and it
does not look inside submodules; their files are checked on their own.

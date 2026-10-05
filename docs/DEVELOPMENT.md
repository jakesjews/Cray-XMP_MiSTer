# Development

How to build, simulate, test, format and lint the core. For what the CPU does
and where it differs from a real CRAY-1, see [CPU.md](CPU.md). For writing
programs, see [PROGRAMMING.md](PROGRAMMING.md).

## Layout

- `Cray1.sv`: the MiSTer `emu` wrapper. `files.qip` lists what Quartus builds.
- `rtl/cray_system.sv`: the machine as the wrapper sees it. CPU, dead start, I/O page.
- `rtl/cray/`: CPU modules written for this core.
- `rtl/cray/cray-1x/`: CPU modules that started in the cray-1x project. That
  project is no longer maintained, so they are maintained here like the rest.
- `rtl/terminal/`, `rtl/console_io.v`, `rtl/mister/`: console, serial port, DDR3 memory port.
- `rtl/boot/`: the monitor ROM. `monitor.mem` is built from `software/monitor/`.
- `sys/`: the MiSTer framework, an unmodified copy of
  [Template_MiSTer](https://github.com/MiSTer-devel/Template_MiSTer).
- `sim/`: Verilator simulations. `tools/`: assembler, reference model, scripts.
- `tests/`: test programs. `software/`: the monitor, its demonstrations, an example.
- `lint/`: Verilator file list, waivers and a PLL stub for `make lint`.

`make` lists the commands below.

## Building the core

Quartus 17.0 Lite builds the core. `build.sh` runs it through a CrossOver
bottle named `Quartus` on macOS.

```sh
./build.sh map        # analysis and synthesis only
./build.sh compile    # full flow, writes output_files/Cray1.rbf
```

`compile` exits with status 2 if timing is not met.

**Do not edit `files.qip`, `Cray1.qsf` or any RTL while a build runs.** Quartus
stops with "Settings File changed outside of the Quartus Prime software" and
rewrites `Cray1.qsf` as one large flattened file. If that happens, restore
`Cray1.qsf` to its 78-line form before building again.

The monitor is part of the bitstream. After changing anything under
`software/monitor/`, run `make rom` and rebuild.

## Host tools

A Rust toolchain builds the assembler and the reference model:

```sh
make tools
tools/target/release/cray1 asm prog.cal -o prog.cry -l prog.lst
tools/target/release/cray1 dis prog.cry
tools/target/release/cray1 isa                 # the instruction table
tools/target/release/cray1-run prog.cry --input 'text\r' --max 1000000
```

`cray1-run` is the reference model: an instruction-level CRAY-1 written from
the hardware reference manual, independent of the RTL. It keeps track of
values a program has no right to rely on, such as registers at power-up or a
load from outside the program's field, and stops if one decides a branch, an
address or console output.

## Simulations

With Verilator 5 on `PATH`:

```sh
make sim
sim/build/cpu/Vcray_cpu --image prog.cry --mem ddr3 --input 'text\r'
sim/build/fp/Vfp_tb tests/fp/xmp_ref.vec
(cd rtl/terminal && ../../sim/build/emu/Vemu --type '?\r' --frame /tmp/screen.ppm)
```

- `sim/build/cpu_xmp/Vcray_cpu` is the same simulation with `XMP = 1` and
  four million words of memory, for the X-MP setting.
- `Vcray_cpu` is the CPU with a memory model. `--mem` picks the memory timing:
  `0`, `fixed:N`, `rand:A-B`, `ddr3` or `slow`. `--step` holds each instruction
  until the one before has finished.
- `Vfp_tb` checks the floating-point units against vector files.
- `Vemu` is the whole core with stand-ins for `hps_io`, the PLL and DDR3. It
  runs with no image to boot the monitor, types keys, sends serial bytes and
  saves a frame of video. Run it from `rtl/terminal` so the font files are found.
  The machine has its own clock; `--cpu-ratio R` sets how many of its cycles
  run per video clock cycle.

## Tests

```sh
make test-quick    # smoke and directed tests, reference vectors, 200 random programs
make test          # the same, then long floating-point and random program runs
```

A test passes when the RTL and the reference model end with the same memory
contents, console output and exit code. Each program runs five ways on the
RTL: fast memory, random memory delays, slow memory, one instruction at a
time, and a second random seed. All five must agree with the model.

- `tests/smoke/` are short programs for each area of the machine. Each runs
  under up to four start-ups from `tests/rt/`: direct, through an exchange, in
  user mode, and in user mode at a non-zero base address.
- `tests/directed/` are the bring-up tests for exchange, floating point and
  vectors, and `vload.cal`, vector loads at every alignment, step and length
  (written by `gen_vload.py`).
- `tests/rtl_only/` holds programs that check themselves, for what the model
  cannot predict: the real-time clock, the programmable clock and its
  interrupt, and the console interrupt. They run on the RTL alone. A line
  `* SIM: arguments` in such a program gives the simulator more arguments;
  `--ctrl-c 30000,400000` makes the console ask for its interrupt in those
  clocks. `tests/rt/rt_user.cal` has the macros these tests use to send off
  short user programs and look at the flags they come back with.
- `tests/xmp/` are programs for the X-MP setting, on the start-up
  `tests/rt/rt_xmp.cal`. A program says which machine it is for with a line
  `MACHINE XMP`; the assembler then accepts the X-MP forms, and `difftest.py`
  runs it on the model with `--machine XMP` and on the `XMP = 1` simulation.
- `tools/py/randprog.py` writes random programs. `tools/py/difftest.py rand FIRST LAST`
  runs a range of seeds and keeps failing cases in `build/diff`. With `--xmp`
  both do the same for the X-MP setting, shared registers and semaphores
  included.
- `tests/fp/xmp_ref.vec` holds 79 floating-point cases whose results come from
  the cray-sim project's test program.

Random programs do not use the exits, the monitor instructions, channel
status or the clocks, and they index memory through A0 to A5 and A7.
The smoke tests and the monitor cover exits, exchanges and range errors.

## Testing on a MiSTer

`tools/py/hil.py` drives a MiSTer over SSH (`MISTER`, default `root@mister`;
`MISTER_PW`, default `1`). It needs `sshpass`.

```sh
python3 tools/py/hil.py deploy                  # copy the core
python3 tools/py/hil.py launch -t 3             # start it and print the console
python3 tools/py/mkbatch.py build/hwbatch -I tests/rt --rand 1 300 build/smoke/*.cal
python3 tools/py/hil.py batch build/hwbatch     # run every program and compare memory
python3 tools/py/hil.py run prog.cry -t 5       # load one image and print the console
python3 tools/py/hil.py keys '3\r'              # type on a virtual keyboard
```

The console is mirrored on the HPS serial port, `/dev/ttyS1` at 115200 baud.
A serial BREAK holds the machine in reset for as long as it lasts and then
dead starts from memory as it is, without copying the monitor. `hil.py run`
and `batch` use that to load images from Linux through `/dev/mem` at
`0x30000000`, where the core's memory lives.

Screenshots need direct video off: `hil.py direct-video off`, `hil.py shot out.png`,
`hil.py direct-video on`.

## Formatting

With `verible-verilog-format`, Python 3.9+ and Git on `PATH`, run from the
repository root:

```sh
make format-check                       # read-only; fails if formatting differs
make format                             # apply formatting
make format FILES='rtl/cray_system.sv'
```

The same operations are available through
`python3 tools/py/verible.py format|format-check [files...]`. `format-check`
returns nonzero when formatting differs; both return nonzero on tool errors.

The project scope is the `Cray1.sv` wrapper and the Verilog and SystemVerilog
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

This writes a fixed build ID to `lint/gen/` and runs Verilator three times
with `--lint-only -Wall -f lint/rtl.f`:

- the `emu` top, as built for the MiSTer
- the `emu` top with `SHELL_TEST` defined, the memory self-test build
- `cray_cpu` with `XMP=1`, the X-MP setting

`lint/rtl.f` lists the sources from `files.qip`; update both when adding a
synthesis source. `.v` files are parsed as Verilog 2005, as Quartus does.

Warnings are fatal. The policy and the waivers are in `lint/exclusions.vlt`:

- File naming, shadowing, declaration initialisers and explicitly empty port
  connections are not checked anywhere.
- `sys/` and the PLL stub are loaded so that connections from project RTL are
  checked, but their own diagnostics are suppressed.
- Everything else, the cray-1x sources included, has waivers only for
  reviewed cases, each with its reason: the `hps_io` ports this core does not
  use, named one by one; signals only the X-MP build uses; bits the
  floating-point arithmetic forms and then drops; ports and instruction
  fields a module takes but does not need.

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
the same values sent to every submodule, for all inputs. A module with an
`XMP` parameter is checked for both settings. Use it after reformatting or a
lint clean-up, to cover what the simulations do not reach.

It needs ports, registers and submodule instances to keep their names, and it
does not look inside submodules; their files are checked on their own.

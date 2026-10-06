#!/usr/bin/env python3
"""Check the CRAY X-MP core as a whole (module emu of CrayXMP.sv) in simulation.

    coretest.py screens [SEED [CASES]]
    coretest.py spool [CASES]
    coretest.py nofile
    coretest.py printer
    coretest.py boot [SYSTEM]
    coretest.py start [SYSTEM]
    coretest.py restart [SYSTEM]

screens checks the core's terminal (rtl/terminal/term_ampex.v) against the
reference model's: random character streams, CASES of them (default 400), and
the screens they leave.

spool checks the printer's file (rtl/mister/print_spool.v): random printing
into files of random lengths, part of them used before, CASES of them (default
400), and what the files hold afterwards.

The others run the core with stand-ins for the MiSTer framework, the PLL and
DDR3 (sim/harness/core_main.cpp).  Memory is full of junk, as DDR3 is.

nofile starts the core with no boot file: it has to say so on the screen.

printer loads the self-checking program of tests/ios/selftest.py as the boot
file.  The program has to report OK on the operator's console, which takes
three keys that come in one burst, and what it prints has to be on the
printer's screen and in the printer's file, behind the five blocks an earlier
session is made to have left there.

boot loads a boot file made from the COS 1.17 software in SYSTEM (default:
CRAY1_SYSTEM, or the directory research/Cray 1 Disk Image from Youtube),
without the kernel's tests that only take time.  The kernel has to ask for
the date; date and time are typed on the keyboard; then the menu's reset is
pressed and the kernel has to ask again.

start goes on from the date and the time: START COS_117 DEADSTART on the serial
port, STATION when COS has been started, then F2 and LOGON on the keyboard.
The run ends when the station shows the banner of COS.  About four minutes.

restart starts COS the same way and presses the menu's reset when the kernel
reports START COMPLETE, with the CPU running.  The kernel has to come up again
and ask for the date, and the CPU has to wait.  About three minutes.

Each run also compares the two screens in the core with what the serial port
carried, and fails if the CPU runs before START has been typed.  Needs make
tools and make -C sim core ampex spool.
"""
import os
import subprocess
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
CORE = os.environ.get('CRAY_CORE_SIM', os.path.join(ROOT, 'sim/build/core/Vemu'))
AMPEX = os.environ.get('CRAY_AMPEX_SIM', os.path.join(ROOT, 'sim/build/ampex/Vterm_ampex_tb'))
SPOOL = os.environ.get('CRAY_SPOOL_SIM', os.path.join(ROOT, 'sim/build/spool/Vprint_spool'))
RANDOM = os.path.join(ROOT, 'tools/target/release/examples/screen_random')
OUT = os.path.join(ROOT, 'build/core')
# the channels of the BIOP's nine drives, in order
DRIVES = [0o20, 0o21, 0o22, 0o24, 0o25, 0o26, 0o30, 0o31, 0o32]
DATE = ['--press', 'ENTER DATE [MM/DD/YY]=10/05/89\\r', '--press', 'ENTER TIME [HH:MM:SS]=01:02:03\\r']


def report(r, wanted):
    """The last lines of a run, and whether it showed everything in `wanted`."""
    missing = [w for w in wanted if w not in r.stdout]
    for line in r.stdout.strip().splitlines()[-2:]:
        print(line[:110])
    for w in missing:
        print('FAIL: `%s` was not shown' % w)
    failed = bool(missing) or r.returncode != 0
    print('1 runs, %d failed' % failed)
    return not failed


def run(args):
    return subprocess.run([CORE] + args, cwd=ROOT, capture_output=True, text=True, errors='replace')


def screens(seed, cases):
    path = os.path.join(OUT, 'screens.bin')
    subprocess.run([RANDOM, str(seed), str(cases), path], check=True)
    r = subprocess.run([AMPEX, path], capture_output=True, text=True)
    print((r.stdout + r.stderr).strip())
    print('1 runs, %d failed' % (r.returncode != 0))
    return r.returncode == 0


def spool(cases):
    r = subprocess.run([SPOOL, str(cases)], capture_output=True, text=True)
    print((r.stdout + r.stderr).strip())
    print('1 runs, %d failed' % (r.returncode != 0))
    return r.returncode == 0


def nofile():
    return report(run(['--ms', '30']), ['CRAY X-MP', 'Load a boot file from the menu to start it.'])


def printer():
    there = os.path.join(OUT, 'selftest')
    subprocess.run([sys.executable, os.path.join(ROOT, 'tests/ios/selftest.py'), there], check=True)
    boot, printed = os.path.join(OUT, 'selftest.ios'), os.path.join(OUT, 'printer.txt')
    subprocess.run([sys.executable, os.path.join(ROOT, 'tools/py/mkboot.py'), os.path.join(there, 'target/cos_117/iop_kern.bin'),
                    os.path.join(there, 'boot_tape.tap'), boot], check=True, stdout=subprocess.DEVNULL)
    if os.path.exists(printed):
        os.remove(printed)
    # the program also wants three keys, which come in one burst on the serial port
    r = run([boot, '--ms', '330', '--screen', '2', '--printed', '5', '--printer', printed, '--type', '+1=AB\\r', '--burst'])
    # a new page is an empty line on the screen, under the line the cursor was on
    ok = report(r, ['0:OK', "---- printer's screen\n\n\nPRINT!\n XX\nPR\n"])
    text = open(printed, 'rb').read() if os.path.exists(printed) else b''
    # the earlier session's blocks as they were, then what was printed, and blanks up to the block's end
    want = text[:2560] == bytes(10 if n % 64 == 63 else 46 for n in range(2560)) and text[2560:].rstrip(b' ') == b'\x0cPRINT!\n XX \nPR\n'
    if not want:
        print('FAIL: the printer\'s file holds %r behind the earlier blocks' % text[2560:2600])
    return ok and want


def boot_file(system):
    path = os.path.join(OUT, 'quick.ios')
    subprocess.run([sys.executable, os.path.join(ROOT, 'tools/py/mkboot.py'), os.path.join(system, 'target/cos_117/iop_kern.bin'),
                    os.path.join(system, 'boot_tape.tap'), path, '--quick'], check=True, stdout=subprocess.DEVNULL)
    return path


def boot(system):
    r = run([boot_file(system), '--disk', '0=' + os.path.join(system, 'exp_disk.img')] + DATE +
            ['--reset-at', '700', '--until', 'ENTER DATE [MM/DD/YY]', '--ms', '2000'])
    return report(r, ['I/O SUBSYSTEM DEAD START', '10/05/89  01:02:03', 'the text was shown'])


def start(system):
    cmd = [boot_file(system), '--disk', '0=' + os.path.join(system, 'exp_disk.img')]
    for n, channel in enumerate(DRIVES):
        cmd += ['--drive', '%d=%s' % (n, os.path.join(system, 'biop_dk%o.img' % channel))]
    cmd += DATE + ['--type', '10/05/89  01:02:03=START COS_117 DEADSTART\\r', '--type', 'START COMPLETE=STATION\\r',
                   '--press', '@0:CRAY STATION={f2}LOGON\\r', '--until', '@0:COS 1.17', '--ms', '9000', '--screen', '0']
    return report(run(cmd), ['MFINIT: COMPLETE', 'CPU <-> MIOP LINKAGE COMPLETE', 'START COMPLETE', '>LOGON', 'the text was shown'])


def restart(system):
    cmd = [boot_file(system), '--disk', '0=' + os.path.join(system, 'exp_disk.img')] + DATE
    cmd += ['--type', '10/05/89  01:02:03=START COS_117 DEADSTART\\r', '--reset-on', 'START COMPLETE',
            '--until', 'ENTER DATE [MM/DD/YY]', '--ms', '9000']
    return report(run(cmd), ['START COMPLETE', 'the text was shown'])


def main():
    a = sys.argv[1:]
    os.makedirs(OUT, exist_ok=True)
    system = os.environ.get('CRAY1_SYSTEM', os.path.join(ROOT, 'research/Cray 1 Disk Image from Youtube'))
    if a and a[0] == 'screens':
        ok = screens(int(a[1]) if len(a) > 1 else 1, int(a[2]) if len(a) > 2 else 400)
    elif a and a[0] == 'spool':
        ok = spool(int(a[1]) if len(a) > 1 else 400)
    elif a == ['nofile']:
        ok = nofile()
    elif a == ['printer']:
        ok = printer()
    elif a and a[0] in ('boot', 'start', 'restart'):
        ok = {'boot': boot, 'start': start, 'restart': restart}[a[0]](a[1] if len(a) > 1 else system)
    else:
        sys.exit(__doc__)
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()

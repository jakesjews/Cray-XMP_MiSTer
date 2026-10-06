#!/usr/bin/env python3
"""Check the CRAY X-MP core as a whole (module emu of CrayXMP.sv) in simulation.

    coretest.py screens [SEED [CASES]]
    coretest.py nofile
    coretest.py boot [SYSTEM]
    coretest.py start [SYSTEM]

screens checks the core's terminal (rtl/terminal/term_ampex.v) against the
reference model's: random character streams, CASES of them (default 400), and
the screens they leave.

The others run the core with stand-ins for the MiSTer framework, the PLL and
DDR3 (sim/harness/core_main.cpp).  Memory is full of junk, as DDR3 is.

nofile starts the core with no boot file: it has to say so on the screen.

boot loads a boot file made from the COS 1.17 software in SYSTEM (default:
CRAY1_SYSTEM, or the directory research/Cray 1 Disk Image from Youtube),
without the kernel's tests that only take time.  The kernel has to ask for
the date; date and time are typed on the keyboard; then the menu's reset is
pressed and the kernel has to ask again.

start goes on from the date and the time: START COS_117 DEADSTART on the serial
port, STATION when COS has been started, then F2 and LOGON on the keyboard.
The run ends when the station shows the banner of COS.  About four minutes.

Each run also compares the two screens in the core with what the serial port
carried.  Needs make tools and make -C sim core ampex.
"""
import os
import subprocess
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
CORE = os.environ.get('CRAY_CORE_SIM', os.path.join(ROOT, 'sim/build/core/Vemu'))
AMPEX = os.environ.get('CRAY_AMPEX_SIM', os.path.join(ROOT, 'sim/build/ampex/Vterm_ampex_tb'))
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


def nofile():
    return report(run(['--ms', '30']), ['CRAY X-MP', 'Load a boot file from the menu to start it.'])


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


def main():
    a = sys.argv[1:]
    os.makedirs(OUT, exist_ok=True)
    system = os.environ.get('CRAY1_SYSTEM', os.path.join(ROOT, 'research/Cray 1 Disk Image from Youtube'))
    if a and a[0] == 'screens':
        ok = screens(int(a[1]) if len(a) > 1 else 1, int(a[2]) if len(a) > 2 else 400)
    elif a == ['nofile']:
        ok = nofile()
    elif a and a[0] in ('boot', 'start'):
        ok = (boot if a[0] == 'boot' else start)(a[1] if len(a) > 1 else system)
    else:
        sys.exit(__doc__)
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()

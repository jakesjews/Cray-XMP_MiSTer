#!/usr/bin/env python3
"""Check the CRAY X-MP core as a whole (module emu of Cray-XMP.sv) in simulation.

    coretest.py screens [SEED [CASES]]
    coretest.py spool [CASES]
    coretest.py nofile
    coretest.py printer
    coretest.py boot [SYSTEM]
    coretest.py start [SYSTEM]
    coretest.py start-real [SYSTEM]
    coretest.py restart [SYSTEM]
    coretest.py tape [SYSTEM]

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
the directory the environment variable CRAY_XMP_SYSTEM names),
without the kernel's tests that only take time.  The kernel has to ask for
the date; date and time are typed on the keyboard; then the menu's reset is
pressed and the kernel has to ask again.

start goes on from the date and the time: START COS_117 DEADSTART on the serial
port, STATION when COS has been started, then F2 and LOGON on the keyboard.
The run ends when the station shows the banner of COS.  About four minutes.
start-real does the same with the menu's "Device times: Real", where
seeks and sectors take the drive's times; it takes longer.

restart starts COS the same way and presses the menu's reset when the kernel
reports START COMPLETE, with the CPU running.  The kernel has to come up again
and ask for the date, and the CPU has to wait.  About three minutes.

tape boots the same way and then chooses a blank tape file in the menu.  The
kernel dumps a file of its disk to it, and the tape has to be the one the
system model writes when it is asked the same.  About three minutes.

Each run also compares the two screens in the core with what the serial port
carried, and fails if the CPU runs before START has been typed.  Needs make
tools and make -C sim core ampex spool.
"""
import os
import subprocess
import sys

import mktape

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


def start(system, real_times=False):
    """COS is started and the station logs on; with real_times the devices take the real ones' times."""
    cmd = [boot_file(system), '--disk', '0=' + os.path.join(system, 'exp_disk.img')]
    for n, channel in enumerate(DRIVES):
        cmd += ['--drive', '%d=%s' % (n, os.path.join(system, 'biop_dk%o.img' % channel))]
    cmd += DATE + ['--type', '10/05/89  01:02:03=START COS_117 DEADSTART\\r', '--type', 'START COMPLETE=STATION\\r',
                   '--press', '@0:CRAY STATION={f2}LOGON\\r', '--until', '@0:COS 1.17', '--screen', '0']
    cmd += ['--real-times', '--ms', '30000'] if real_times else ['--ms', '9000']
    return report(run(cmd), ['MFINIT: COMPLETE', 'CPU <-> MIOP LINKAGE COMPLETE', 'START COMPLETE', '>LOGON', 'the text was shown'])


def tape(system):
    """A tape file from the menu, written by the kernel; the model's tape is what it has to hold."""
    blank, by_model, by_core = (os.path.join(OUT, 'tape_%s.tap' % n) for n in ('blank', 'model', 'core'))
    with open(blank, 'wb') as f:
        f.write(b'\xff' * (64 * 512))
    script = os.path.join(OUT, 'tape.script')
    with open(script, 'w') as f:
        f.write('wait kernel ENTER DATE [MM/DD/YY]\nrun 100\ntype kernel 10/05/89\nwait kernel ENTER TIME [HH:MM:SS]\nrun 100\n'
                'type kernel 01:02:03\nrun 500\ntape %s\ntype kernel FDUMP STATION/JINSTALL @MT0:\nwait kernel FDUMP COMPLETE\n'
                'save-tape %s\n' % (blank, by_model))
    subprocess.run([os.path.join(ROOT, 'tools/target/release/cray-xmp-sys'), system, '--script', script, '--quiet'],
                   check=True, capture_output=True)
    r = run([boot_file(system), '--disk', '0=' + os.path.join(system, 'exp_disk.img')] + DATE +
            ['--mount', '10/05/89  01:02:03=' + blank, '--type', '+300=FDUMP STATION/JINSTALL @MT0:\\r',
             '--until', 'FDUMP COMPLETE', '--tape-out', by_core, '--ms', '4000'])
    ok = report(r, ['FDUMP COMPLETE', 'the text was shown'])
    a = list(mktape.items(open(by_model, 'rb').read()))
    b = list(mktape.items(open(by_core, 'rb').read())) if os.path.exists(by_core) else []
    if len(a) < 6 or a != b:
        print('FAIL: the tape the core wrote is not the model\'s (%s, %s)' % (by_model, by_core))
        return False
    return ok


def restart(system):
    cmd = [boot_file(system), '--disk', '0=' + os.path.join(system, 'exp_disk.img')] + DATE
    cmd += ['--type', '10/05/89  01:02:03=START COS_117 DEADSTART\\r', '--reset-on', 'START COMPLETE',
            '--until', 'ENTER DATE [MM/DD/YY]', '--ms', '9000']
    return report(run(cmd), ['START COMPLETE', 'the text was shown'])


def main():
    a = sys.argv[1:]
    os.makedirs(OUT, exist_ok=True)
    system = os.environ.get('CRAY_XMP_SYSTEM', '')
    if a and a[0] == 'screens':
        ok = screens(int(a[1]) if len(a) > 1 else 1, int(a[2]) if len(a) > 2 else 400)
    elif a and a[0] == 'spool':
        ok = spool(int(a[1]) if len(a) > 1 else 400)
    elif a == ['nofile']:
        ok = nofile()
    elif a == ['printer']:
        ok = printer()
    elif a and a[0] in ('boot', 'start', 'restart', 'start-real', 'tape'):
        if len(a) < 2 and not system:
            sys.exit('coretest: name the directory of the COS 1.17 software, or set CRAY_XMP_SYSTEM')
        kinds = {'boot': boot, 'start': start, 'restart': restart, 'start-real': lambda s: start(s, True), 'tape': tape}
        ok = kinds[a[0]](a[1] if len(a) > 1 else system)
    else:
        sys.exit(__doc__)
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()

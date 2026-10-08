#!/usr/bin/env python3
"""Check the I/O Processor (rtl/ios/iop_cpu.v) against the reference model.

    ioptest.py rand FIRST LAST [-n STEPS] [-j JOBS]
    ioptest.py kernel [SYSTEM]
    ioptest.py boot [SYSTEM] [--full]
    ioptest.py selftest
    ioptest.py machine [SYSTEM] [--start]
    ioptest.py bridges [CASES]
    ioptest.py disks

rand runs random programs, seeds FIRST to LAST, each in two kinds (every parcel
random; mostly register work with functions on the processor's own channels)
and a short directed one.  kernel has the system model boot the I/O Subsystem
kernel of the COS 1.17 software in SYSTEM (default: the directory the
environment variable CRAY_XMP_SYSTEM names) as far as its first
question, once for each of the three processors, and replays every step.

boot runs the I/O Subsystem in hardware description (module ios: the three
processors, their Local Memories, real-time clocks, Buffer Memory channels,
the channels between them, the consoles, the Peripheral Expander, and the
BIOP's disk drives and channel into central memory) on the same kernel.  The
MIOP loads its overlays from the tape, asks for the date and the time, is
told, and is asked to list the files on the expander disk; the BIOP tests its
nine drives.  Without --full the kernel's long tests of memory are left out.

selftest runs the self-checking program of tests/ios/selftest.py in place of
the kernel, on the system model and on the same three processors in hardware
description: the real-time clock, the order in which channels that ask for an
interrupt are reported, the expander's tape and disk, the start of one
processor by another, a drive of the BIOP and its channel into central
memory, a word from each processor to each other one, and the printer.  Both
must report OK three times and print the same.  It runs a second time on the
whole machine, where the program also starts the CPU on a program of a few
instructions and exchanges parcels with it over the channel pair, to see
that a parcel that waits is not lost and what I/O Master Clear does.  Three
keys are typed on the operator's console one behind the other, and the
program must get all three.

machine runs the whole machine in hardware description (module xmp_machine: the
CPU with the X-MP features and the I/O Subsystem) on the same software, without
the tests that only take time.  The operator gives the date and the time and
types START COS_117 DEADSTART.  The kernel then checks the mainframe: it holds
the CPU with Master Clear, loads a test program into central memory through
the BIOP's channel, lets the CPU go, takes a word from it over the channel
pair and reads memory back; the run ends when it reports MFINIT: COMPLETE.
With --start it goes on until COS has been loaded from the expander disk and
started and the kernel reports START COMPLETE, which takes much longer.

bridges checks what carries pulses, levels and memory requests between the
CPU's clock and the I/O Subsystem's (rtl/xmp_bridge.v), by themselves, with
clocks of random periods, requests that are taken back, and resets
(sim/harness/bridge_main.cpp says what must hold).

disks runs the BIOP's disk drives by themselves (rtl/ios/ios_disks.v): what
they read and write, and how long a seek and a sector take, fast and with the
DD-29's own times (sim/harness/disks_main.cpp).

The model writes a record of each step (tools/crates/ios/src/replay.rs) and
the simulation of the hardware description follows it: same interrupts, same
functions, same registers after every step, same memory at the end.
Needs make tools and make -C sim iop ios xmp bridge.  Failing records stay in
build/iop.
"""
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
SIM = os.environ.get('CRAY_IOP_SIM', os.path.join(ROOT, 'sim/build/iop/Viop_cpu'))
RANDOM = os.path.join(ROOT, 'tools/target/release/examples/iop_random')
SYS = os.path.join(ROOT, 'tools/target/release/cray-xmp-sys')
BOOT = os.environ.get('CRAY_IOS_SIM', os.path.join(ROOT, 'sim/build/ios/Vios'))
MACHINE = os.environ.get('CRAY_XMP_SIM', os.path.join(ROOT, 'sim/build/xmp/Vxmp_machine'))
BRIDGE = os.environ.get('CRAY_BRIDGE_SIM', os.path.join(ROOT, 'sim/build/bridge/Vxmp_bridge_tb'))
DISKS = os.environ.get('CRAY_DISKS_SIM', os.path.join(ROOT, 'sim/build/disks/Vios_disks'))
# the channels of the BIOP's nine drives, in order
DRIVES = [0o20, 0o21, 0o22, 0o24, 0o25, 0o26, 0o30, 0o31, 0o32]
OUT = os.path.join(ROOT, 'build/iop')


def replay(name, make):
    """Write a record with the command make (which gets its path) and follow it."""
    record = os.path.join(OUT, name + '.rec')
    r = subprocess.run(make + [record], capture_output=True, text=True)
    if r.returncode != 0:
        return name, 'no record: ' + (r.stderr.strip().splitlines() or ['?'])[-1]
    r = subprocess.run([SIM, record, '--quiet'], capture_output=True, text=True)
    if r.returncode == 0:
        os.remove(record)
        return name, None
    return name, (r.stdout.strip().splitlines() or ['no output'])[-1]


def rand(first, last, steps, jobs):
    cases = [('directed', [RANDOM, 'directed'])]
    for seed in range(first, last + 1):
        for kind in ('noise', 'work'):
            cases.append(('%s%d' % (kind, seed), [RANDOM, str(seed), str(steps), kind]))
    with ThreadPoolExecutor(jobs) as pool:
        results = list(pool.map(lambda c: replay(*c), cases))
    failed = [(name, why) for name, why in results if why]
    for name, why in failed[:10]:
        print('FAIL %s: %s' % (name, why))
    print('%d programs, %d failed' % (len(cases), len(failed)))
    return not failed


def kernel(system):
    """The boot of each processor, piped from the model to the simulation."""
    script = os.path.join(OUT, 'kernel.script')
    with open(script, 'w') as f:
        f.write('wait kernel ENTER DATE\n')

    def one(n):
        fifo = os.path.join(OUT, 'kernel%d.fifo' % n)
        if os.path.exists(fifo):
            os.remove(fifo)
        os.mkfifo(fifo)
        model = subprocess.Popen([SYS, system, '--script', script, '--quiet', '--replay', '%d,1000000000,%s' % (n, fifo)],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        r = subprocess.run([SIM, fifo], capture_output=True, text=True)
        errors = model.communicate()[1]
        os.remove(fifo)
        line = (r.stdout.strip().splitlines() or ['no output'])[-1]
        if model.returncode != 0:
            line += ' (the model: %s)' % (errors.strip().splitlines() or ['?'])[0]
        print('IOP %d: %s' % (n, line), flush=True)
        return r.returncode == 0 and model.returncode == 0

    with ThreadPoolExecutor(3) as pool:
        good = list(pool.map(one, [0, 1, 3]))
    print('3 boots, %d failed' % good.count(False))
    return all(good)


def boot(system, full):
    """The three processors in hardware description boot the kernel."""
    cmd = [BOOT, os.path.join(system, 'target/cos_117/iop_kern.bin'), os.path.join(system, 'boot_tape.tap'),
           os.path.join(system, 'exp_disk.img'),
           '--type', 'ENTER DATE [MM/DD/YY]=10/05/89\\r', '--type', 'ENTER TIME [HH:MM:SS]=01:02:03\\r',
           '--type', '10/05/89  01:02:03=FSTAT\\r', '--until', 'FSTAT COMPLETE', '--ms', '25000' if full else '8000']
    if not full:
        # the memory test returns at once and the Buffer Memory test is short
        cmd += ['--poke', '42B7=0200', '--poke', '43DA=0']
    r = subprocess.run(cmd, capture_output=True, text=True, errors='replace')
    out = r.stdout
    wanted = ['MOS TEST COMPLETE', 'IOP-0 KERNEL, VERSION 4.2.2', 'IOP1 A->A', 'IOP3 A->A', 'MOS SIZE  100K',
              'AUTODMP ON', 'ENTER DATE', 'ENTER TIME', '10/05/89  01:02:03',
              # the files on the expander disk, as the system model lists them
              'IOPKERNEL             01/01/89 01:01:01         5120', 'COS_117               01/01/89 01:01:01         279040',
              'MINSTALL              01/01/89 01:01:01         80', 'TOTAL                  ---------------          850596',
              'FSTAT COMPLETE',
              'console of the BIOP: IOP-1 KERNEL, VERSION 4.2.2',
              'console of the XIOP: IOP-3 KERNEL, VERSION 4.2.2']
    missing = [w for w in wanted if w not in out]
    # the BIOP tests its nine drives and has nothing to report
    missing += ['not ' + w for w in ('CHANNEL TIMEOUT', 'DATA ERROR', 'SELECT ERROR') if w in out]
    print((out.strip().splitlines() or ['no output'])[-3][:100])
    for w in missing:
        print('FAIL: `%s` was shown' % w[4:] if w.startswith('not ') else 'FAIL: `%s` was not shown' % w)
    print('1 boots, %d failed' % (1 if missing or r.returncode != 0 else 0))
    return not missing and r.returncode == 0


def machine(system, start):
    """The CPU and the I/O Subsystem in hardware description, through START."""
    cmd = [MACHINE, os.path.join(system, 'target/cos_117/iop_kern.bin'), os.path.join(system, 'boot_tape.tap'),
           os.path.join(system, 'exp_disk.img'), '--quick',
           '--type', 'ENTER DATE [MM/DD/YY]=10/05/89\\r', '--type', 'ENTER TIME [HH:MM:SS]=01:02:03\\r',
           '--type', '10/05/89  01:02:03=START COS_117 DEADSTART\\r']
    wanted = ['MFINIT: COMPLETE']
    if start:
        for n, channel in enumerate(DRIVES):
            cmd += ['--drive', '%d=%s' % (n, os.path.join(system, 'biop_dk%o.img' % channel))]
        cmd += ['--until', 'START COMPLETE', '--ms', '6000']
        wanted += ['CPU <-> MIOP CHANNEL INIT', 'CPU <-> MIOP LINKAGE COMPLETE', 'START COMPLETE']
    else:
        cmd += ['--until', 'MFINIT: COMPLETE', '--ms', '3000']
    r = subprocess.run(cmd, capture_output=True, text=True, errors='replace')
    missing = [w for w in wanted if w not in r.stdout]
    for line in r.stdout.strip().splitlines()[-4:-2]:
        print(line[:110])
    for w in missing:
        print('FAIL: `%s` was not shown' % w)
    print('1 runs, %d failed' % (1 if missing or r.returncode != 0 else 0))
    return not missing and r.returncode == 0


def selftest():
    """The self-checking program on the model and on the hardware description:
    the I/O Subsystem alone, and with the mainframe."""
    failed = 0
    for cpu in (False, True):
        there = os.path.join(OUT, 'selftest_cpu' if cpu else 'selftest')
        subprocess.run([sys.executable, os.path.join(ROOT, 'tests/ios/selftest.py'), there] + (['--cpu'] if cpu else []), check=True)
        script = os.path.join(OUT, 'selftest.script')
        with open(script, 'w') as f:
            f.write('type kernel AB\nrun 80\nscreen kernel\nscreen 1.1\nscreen 3.1\nprinter\n')
        ran = subprocess.run([SYS, there, '--script', script, '--quiet'], capture_output=True, text=True, errors='replace')
        model, summary = ran.stdout, ran.stderr
        said = [line.strip() for line in model.splitlines() if ':' in line and len(line.strip()) <= 5]
        # with the mainframe the program starts the CPU itself, and holds it again at its end
        bench = [MACHINE, '--cpu-may-run'] if cpu else [BOOT]
        hardware = subprocess.run(bench + [os.path.join(there, 'target/cos_117/iop_kern.bin'), os.path.join(there, 'boot_tape.tap'),
                                           '--ms', '80', '--type', '+1=AB\\r', '--burst'], capture_output=True, text=True, errors='replace').stdout
        lines = [line.strip() for line in hardware.splitlines()]
        shown = [line for line in lines if len(line) <= 5 and ':' in line]
        shown += [line.split(': ', 1)[1] for line in lines if line.startswith('console of the ') and ': ' in line]
        # what the printer printed: a new page, six characters, dots, two characters
        if '---- printer at' in model:
            said.append(model.split('---- printer at', 1)[1].split('\n', 1)[1].split('\n----', 1)[0].replace('\n', '<nl>'))
        shown += [line[len('printed: '):] for line in hardware.splitlines() if line.startswith('printed: ')]
        want = ['0:OK', '1:OK', '3:OK', '<0c>PRINT!<nl> XX <nl>PR<nl>']
        if cpu:
            said += ['held' if line.rstrip().endswith(', held') else 'running' for line in summary.splitlines() if line.strip().startswith('CPU:')]
            shown += ['held' if 'is held by Master Clear' in line else 'running' for line in lines if line.startswith('the CPU ran for')]
            want.append('held')
        for name, got in (('the model', said), ('the hardware description', shown)):
            print('%s%s: %s' % (name, ', with the mainframe' if cpu else '', ' '.join(got) or 'nothing'))
            failed += got != want
    print('4 runs, %d failed' % failed)
    return failed == 0


def bridges(cases):
    r = subprocess.run([BRIDGE, str(cases)], capture_output=True, text=True)
    print((r.stdout + r.stderr).strip())
    print('1 runs, %d failed' % (r.returncode != 0))
    return r.returncode == 0


def disks():
    r = subprocess.run([DISKS], capture_output=True, text=True)
    lines = (r.stdout + r.stderr).strip().splitlines()
    print('\n'.join([l for l in lines if 'WRONG' in l or 'OUT OF RANGE' in l or 'should be' in l] + lines[-1:]))
    print('1 runs, %d failed' % (r.returncode != 0))
    return r.returncode == 0


def main():
    a = sys.argv[1:]
    os.makedirs(OUT, exist_ok=True)
    if len(a) >= 3 and a[0] == 'rand':
        steps = int(a[a.index('-n') + 1]) if '-n' in a else 3000
        jobs = int(a[a.index('-j') + 1]) if '-j' in a else os.cpu_count() or 4
        ok = rand(int(a[1]), int(a[2]), steps, jobs)
    elif a and a[0] == 'selftest':
        ok = selftest()
    elif a and a[0] == 'bridges':
        ok = bridges(int(a[1]) if len(a) > 1 else 2000)
    elif a == ['disks']:
        ok = disks()
    elif a and a[0] in ('kernel', 'boot', 'machine'):
        rest = [x for x in a[1:] if not x.startswith('--')]
        system = rest[0] if rest else os.environ.get('CRAY_XMP_SYSTEM', '')
        if not system:
            sys.exit('ioptest: name the directory of the COS 1.17 software, or set CRAY_XMP_SYSTEM')
        if a[0] == 'kernel':
            ok = kernel(system)
        elif a[0] == 'boot':
            ok = boot(system, '--full' in a)
        else:
            ok = machine(system, '--start' in a)
    else:
        sys.exit(__doc__)
    sys.exit(0 if ok else 1)


main()

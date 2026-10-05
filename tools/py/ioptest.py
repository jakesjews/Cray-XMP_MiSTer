#!/usr/bin/env python3
"""Check the I/O Processor (rtl/ios/iop_cpu.v) against the reference model.

    ioptest.py rand FIRST LAST [-n STEPS] [-j JOBS]
    ioptest.py kernel [SYSTEM]
    ioptest.py boot [SYSTEM] [--full]
    ioptest.py selftest

rand runs random programs, seeds FIRST to LAST, each in two kinds (every parcel
random; mostly register work with functions on the processor's own channels)
and a short directed one.  kernel has the system model boot the I/O Subsystem
kernel of the COS 1.17 software in SYSTEM (default: CRAY1_SYSTEM, or the
directory research/Cray 1 Disk Image from Youtube) as far as its first
question, once for each of the three processors, and replays every step.

boot runs the I/O Subsystem in hardware description (module ios: the three
processors, their Local Memories, real-time clocks, Buffer Memory channels,
the channels between them, the consoles and the Peripheral Expander) on the
same kernel.  The MIOP loads its overlays from the tape, asks for the date and
the time, is told, and is asked to list the files on the expander disk.
Without --full the kernel's long tests of memory are left out.

selftest runs the self-checking program of tests/ios/selftest.py in place of
the kernel, on the system model and on the same three processors in hardware
description: the real-time clock, the order in which channels that ask for an
interrupt are reported, two sectors written to the expander disk and read
back, the start of one processor by another, and a word from each processor
to each other one.  Both must report OK three times.

The model writes a record of each step (tools/crates/ios/src/replay.rs) and
the simulation of the hardware description follows it: same interrupts, same
functions, same registers after every step, same memory at the end.
Needs make tools and make -C sim iop ios.  Failing records stay in build/iop.
"""
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
SIM = os.environ.get('CRAY_IOP_SIM', os.path.join(ROOT, 'sim/build/iop/Viop_cpu'))
RANDOM = os.path.join(ROOT, 'tools/target/release/examples/iop_random')
SYS = os.path.join(ROOT, 'tools/target/release/cray1-sys')
BOOT = os.environ.get('CRAY_IOS_SIM', os.path.join(ROOT, 'sim/build/ios/Vios'))
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
           '--type', '10/05/89  01:02:03=FSTAT\\r', '--until', 'FSTAT COMPLETE', '--ms', '20000' if full else '3000']
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
              'console of the BIOP: CHANNEL 20 CHANNEL TIMEOUT',
              'console of the XIOP: IOP-3 KERNEL, VERSION 4.2.2']
    missing = [w for w in wanted if w not in out]
    print((out.strip().splitlines() or ['no output'])[-3][:100])
    for w in missing:
        print('FAIL: `%s` was not shown' % w)
    print('1 boots, %d failed' % (1 if missing or r.returncode != 0 else 0))
    return not missing and r.returncode == 0


def selftest():
    """The self-checking program on the model and on the hardware description."""
    there = os.path.join(OUT, 'selftest')
    subprocess.run([sys.executable, os.path.join(ROOT, 'tests/ios/selftest.py'), there], check=True)
    script = os.path.join(OUT, 'selftest.script')
    with open(script, 'w') as f:
        f.write('run 60\nscreen kernel\nscreen 1.1\nscreen 3.1\n')
    model = subprocess.run([SYS, there, '--script', script, '--quiet'], capture_output=True, text=True, errors='replace').stdout
    said = [line.strip() for line in model.splitlines() if ':' in line and len(line.strip()) <= 5]
    hardware = subprocess.run([BOOT, os.path.join(there, 'target/cos_117/iop_kern.bin'), os.path.join(there, 'boot_tape.tap'),
                               '--ms', '60'], capture_output=True, text=True, errors='replace').stdout
    lines = [line.strip() for line in hardware.splitlines()]
    shown = [line for line in lines if len(line) <= 5 and ':' in line]
    shown += [line.split(': ', 1)[1] for line in lines if line.startswith('console of the ') and ': ' in line]
    failed = 0
    for name, got in (('the model', said), ('the hardware description', shown)):
        print('%s: %s' % (name, ' '.join(got) or 'nothing'))
        failed += got != ['0:OK', '1:OK', '3:OK']
    print('2 runs, %d failed' % failed)
    return failed == 0


def main():
    a = sys.argv[1:]
    os.makedirs(OUT, exist_ok=True)
    if len(a) >= 3 and a[0] == 'rand':
        steps = int(a[a.index('-n') + 1]) if '-n' in a else 3000
        jobs = int(a[a.index('-j') + 1]) if '-j' in a else os.cpu_count() or 4
        ok = rand(int(a[1]), int(a[2]), steps, jobs)
    elif a and a[0] == 'selftest':
        ok = selftest()
    elif a and a[0] in ('kernel', 'boot'):
        rest = [x for x in a[1:] if not x.startswith('--')]
        system = rest[0] if rest else os.environ.get('CRAY1_SYSTEM', os.path.join(ROOT, 'research/Cray 1 Disk Image from Youtube'))
        ok = kernel(system) if a[0] == 'kernel' else boot(system, '--full' in a)
    else:
        sys.exit(__doc__)
    sys.exit(0 if ok else 1)


main()

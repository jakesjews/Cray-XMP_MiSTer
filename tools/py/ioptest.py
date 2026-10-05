#!/usr/bin/env python3
"""Check the I/O Processor (rtl/ios/iop_cpu.v) against the reference model.

    ioptest.py rand FIRST LAST [-n STEPS] [-j JOBS]
    ioptest.py kernel [SYSTEM]

rand runs random programs, seeds FIRST to LAST, each in two kinds (every parcel
random; mostly register work with functions on the processor's own channels)
and a short directed one.  kernel has the system model boot the I/O Subsystem
kernel of the COS 1.17 software in SYSTEM (default: CRAY1_SYSTEM, or the
directory research/Cray 1 Disk Image from Youtube) as far as its first
question, once for each of the three processors, and replays every step.

The model writes a record of each step (tools/crates/ios/src/replay.rs) and
the simulation of the hardware description follows it: same interrupts, same
functions, same registers after every step, same memory at the end.
Needs make tools and make -C sim iop.  Failing records stay in build/iop.
"""
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
SIM = os.environ.get('CRAY_IOP_SIM', os.path.join(ROOT, 'sim/build/iop/Viop_cpu'))
RANDOM = os.path.join(ROOT, 'tools/target/release/examples/iop_random')
SYS = os.path.join(ROOT, 'tools/target/release/cray1-sys')
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


def main():
    a = sys.argv[1:]
    os.makedirs(OUT, exist_ok=True)
    if len(a) >= 3 and a[0] == 'rand':
        steps = int(a[a.index('-n') + 1]) if '-n' in a else 3000
        jobs = int(a[a.index('-j') + 1]) if '-j' in a else os.cpu_count() or 4
        ok = rand(int(a[1]), int(a[2]), steps, jobs)
    elif a and a[0] == 'kernel':
        system = a[1] if len(a) > 1 else os.environ.get('CRAY1_SYSTEM', os.path.join(ROOT, 'research/Cray 1 Disk Image from Youtube'))
        ok = kernel(system)
    else:
        sys.exit(__doc__)
    sys.exit(0 if ok else 1)


main()

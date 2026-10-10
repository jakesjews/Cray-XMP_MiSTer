#!/usr/bin/env python3
"""Clock periods of the memory loops of tests/bench/membench.cal in the CPU
simulation, for each element of a loop.

    membench.py [--mem PROFILE]... [--sim VCRAY_CPU] [--seed N]

The loops are those of tests/cos, which docs/MACHINE.md gives the times of on
a DE10-Nano.  PROFILE is what Vcray_cpu takes after --mem (default: mister);
with several, one column each.
"""
import os
import subprocess
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
ASM = os.path.join(ROOT, 'tools/target/release/cray-xmp')
NAMES = ['nothing', 'scalar load', 'scalar store', 'vector load', 'vector store',
         'vector add', 'vec ld+ld+add+st', 'sca ld+ld+add+st']
N, REP, RES = 4096, 2, 0o20000


def main():
    a = sys.argv[1:]
    profiles = [a[i + 1] for i, x in enumerate(a) if x == '--mem'] or ['mister']
    sim = a[a.index('--sim') + 1] if '--sim' in a else os.path.join(ROOT, 'sim/build/cpu/Vcray_cpu')
    seed = a[a.index('--seed') + 1] if '--seed' in a else '1'
    out = os.path.join(ROOT, 'build/bench')
    os.makedirs(out, exist_ok=True)
    img = os.path.join(out, 'membench.img')
    r = subprocess.run([ASM, 'asm', os.path.join(ROOT, 'tests/bench/membench.cal'), '-o', img, '-l', img[:-4] + '.lst',
                        '-I', os.path.join(ROOT, 'tests/rt')], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if r.returncode not in (0, 2):
        sys.exit(r.stderr.decode())
    cols = []
    for p in profiles:
        r = subprocess.run([sim, '--image', img, '--mem', p, '--seed', seed, '--quiet', '--cycles', '40000000',
                            '--dump', '%o:10' % RES], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if r.returncode != 0:
            sys.exit('%s: %s' % (p, r.stderr.decode()[-300:]))
        words = [int(line.split(':')[1], 8) for line in r.stdout.decode().splitlines() if ':' in line]
        cols.append([w / (N * REP) for w in words[:8]])
    print('%-18s' % 'CP an element' + ''.join('%12s' % p for p in profiles))
    for k, name in enumerate(NAMES):
        print('%-18s' % name + ''.join('%12.2f' % c[k] for c in cols))


if __name__ == '__main__':
    main()

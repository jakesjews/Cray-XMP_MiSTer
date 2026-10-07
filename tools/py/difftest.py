#!/usr/bin/env python3
"""Differential testing: the hardware CPU simulation against the reference model.

    difftest.py rand FIRST LAST [-n INSTRUCTIONS] [--no-vector] [--no-float] [--no-mem] [--xmp] [-j JOBS]
    difftest.py file PROG.cal [PROG.cal ...] [-I INCLUDE_DIR] [-j JOBS]

Each program is assembled, run on the model, and run on the RTL simulation in
several modes: different memory latencies and with issue serialized.  Every run
must leave the same memory and console output as the model.  Failing programs
are kept in build/diff with their states.

A program with a MACHINE XMP line (randprog.py --xmp writes one) is for the
machine with the X-MP features: it runs on the model with --machine XMP and
on the simulation built with XMP = 1 (make -C sim cpu-xmp).
"""
import os
import subprocess
import re
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
ASM = os.path.join(ROOT, 'tools/target/release/cray-xmp')
MODEL = os.path.join(ROOT, 'tools/target/release/cray-xmp-run')
RTL = os.environ.get('CRAY_RTL_SIM', os.path.join(ROOT, 'sim/build/cpu/Vcray_cpu'))
RTL_XMP = os.environ.get('CRAY_RTL_SIM_XMP', os.path.join(ROOT, 'sim/build/cpu_xmp/Vcray_cpu'))
OUT = os.path.join(ROOT, 'build/diff')
MODES = [('fast', ['--mem', 'fixed:1']), ('rand', ['--mem', 'rand:1-9']), ('slow', ['--mem', 'slow']),
         ('step', ['--mem', 'ddr3', '--step']), ('seed', ['--mem', 'rand:2-5', '--seed', '77'])]

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cmp_state   # noqa: E402


def run(cmd, **kw):
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw)


def compare(rtl_state, model_state):
    r, m = cmp_state.load(rtl_state), cmp_state.load(model_state)
    problems = []
    if r['exit'] != m['exit']:
        problems.append('exit: rtl %s, model %s' % (r['exit'], m['exit']))
    if r['console'] != m['console']:
        problems.append('console differs')
    n = 0
    for addr in sorted(set(r['mem']) | set(m['mem'])):
        mv, rv = m['mem'].get(addr), r['mem'].get(addr)
        if mv == 'undef':
            continue
        n += 1
        if mv is None or rv is None or int(mv, 16) != int(rv, 16):
            problems.append('mem %07o: rtl %s, model %s' % (addr, rv, mv))
    return n, problems


def is_xmp(cal):
    """A program for the machine with the X-MP features says so: MACHINE XMP."""
    return re.search(r'^\s+MACHINE\s+XMP\b', open(cal).read(), re.M) is not None


def one(name, cal, include):
    base = os.path.join(OUT, name)
    xmp = is_xmp(cal)
    rtl = RTL_XMP if xmp else RTL
    machine = ['--machine', 'XMP'] if xmp else []
    a = run([ASM, 'asm', cal, '-o', base + '.img', '-l', base + '.lst'] + sum((['-I', d] for d in include), []))
    if a.returncode not in (0, 2):
        return name, 'ASM', a.stderr.decode()[:300]
    m = run([MODEL, base + '.img', '--state', base + '.model.state', '--quiet'] + machine)
    if m.returncode != 0:
        return name, 'MODEL', 'exit %d: %s' % (m.returncode, m.stderr.decode()[:300])
    words = 0
    for mode, args in MODES:
        st = '%s.%s.state' % (base, mode)
        r = run([rtl, '--image', base + '.img', '--cycles', '30000000', '--quiet', '--state', st] + args)
        words, problems = compare(st, base + '.model.state')
        if problems:
            return name, 'RTL ' + mode, '; '.join(problems[:3]) + (' ... %d in all' % len(problems) if len(problems) > 3 else '')
        os.remove(st)
    return name, 'ok', '%d words' % words


def main():
    a = sys.argv[1:]
    if len(a) < 2:
        sys.exit(__doc__)
    os.makedirs(OUT, exist_ok=True)
    jobs = int(a[a.index('-j') + 1]) if '-j' in a else (os.cpu_count() or 4)
    include = [a[i + 1] for i, x in enumerate(a) if x == '-I']
    work = []
    if a[0] == 'rand':
        first, last = int(a[1]), int(a[2])
        n = a[a.index('-n') + 1] if '-n' in a else '200'
        opts = [x for x in a if x in ('--no-vector', '--no-float', '--no-mem', '--xmp')]
        for seed in range(first, last + 1):
            name = ('x%d' if '--xmp' in opts else 'r%d') % seed
            cal = os.path.join(OUT, name + '.cal')
            subprocess.run([sys.executable, os.path.join(ROOT, 'tools/py/randprog.py'), str(seed), '-n', n, '-o', cal] + opts, check=True)
            work.append((name, cal))
    else:
        skip = False
        for x in a[1:]:
            if skip: skip = False; continue
            if x in ('-I', '-j'): skip = True; continue
            work.append((os.path.splitext(os.path.basename(x))[0], x))
    bad = 0
    with ThreadPoolExecutor(max_workers=jobs) as ex:
        for name, verdict, detail in ex.map(lambda w: one(w[0], w[1], include), work):
            if verdict == 'ok':
                for ext in ('.img', '.lst', '.model.state', '.cal'):
                    f = os.path.join(OUT, name + ext)
                    if a[0] == 'rand' and os.path.exists(f):
                        os.remove(f)
            else:
                bad += 1
                print('%-10s %-9s %s' % (name, verdict, detail))
    print('%d programs, %d failed' % (len(work), bad))
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""Build a directory of test programs for the hardware batch run (hil.py batch).

    mkbatch.py OUT_DIR [-I INCLUDE_DIR] [--rand FIRST LAST [-n N]] [PROG.cal ...]

Each program is assembled and run on the reference model.  OUT_DIR gets
    NAME.img   the memory image
    NAME.exp   every memory word the model says the run leaves defined:
               octal word address, 16 hex digits
    NAME.con   the console output as hex bytes
A program the model cannot finish is reported and left out.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ASM = os.path.join(ROOT, 'tools/target/release/cray1')
MODEL = os.path.join(ROOT, 'tools/target/release/cray1-run')


def main():
    a = sys.argv[1:]
    if not a:
        sys.exit(__doc__)
    out = a[0]
    os.makedirs(out, exist_ok=True)
    include, progs, i, n = [], [], 1, '250'
    rand = None
    while i < len(a):
        if a[i] == '-I': include.append(a[i + 1]); i += 2
        elif a[i] == '-n': n = a[i + 1]; i += 2
        elif a[i] == '--rand': rand = (int(a[i + 1]), int(a[i + 2])); i += 3
        else: progs.append(a[i]); i += 1
    if rand:
        for seed in range(rand[0], rand[1] + 1):
            cal = os.path.join(out, 'rand%d.cal' % seed)
            subprocess.run([sys.executable, os.path.join(ROOT, 'tools/py/randprog.py'), str(seed), '-n', n, '-o', cal], check=True)
            progs.append(cal)
    made = 0
    for cal in progs:
        name = os.path.basename(cal)[:-4]
        img = os.path.join(out, name + '.img')
        r = subprocess.run([ASM, 'asm', cal, '-o', img] + sum((['-I', d] for d in include), []), capture_output=True, text=True)
        if r.returncode != 0:
            print('%s: does not assemble: %s' % (name, r.stdout[:200] + r.stderr[:200])); continue
        state = os.path.join(out, name + '.state')
        r = subprocess.run([MODEL, img, '--max', '5000000', '--state', state, '--quiet'], capture_output=True, text=True)
        if r.returncode != 0:
            print('%s: model exit status %d %s' % (name, r.returncode, r.stderr.strip()[:120]))
            os.remove(img)
            if os.path.exists(state): os.remove(state)
            continue
        exp, con = [], ''
        for line in open(state):
            p = line.split()
            if p and p[0] == 'mem' and p[2] != 'undef': exp.append('%s %s' % (p[1], p[2]))
            elif p and p[0] == 'console' and len(p) > 1: con = p[1]
        open(os.path.join(out, name + '.exp'), 'w').write('\n'.join(exp) + '\n')
        open(os.path.join(out, name + '.con'), 'w').write(con + '\n')
        os.remove(state)
        if cal.startswith(out): os.remove(cal)
        made += 1
    print('%d programs in %s' % (made, out))


main()

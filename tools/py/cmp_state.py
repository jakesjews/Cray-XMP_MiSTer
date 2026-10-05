#!/usr/bin/env python3
"""Compare an end state from the RTL simulation with one from the reference model.

Both files hold lines of
    exit <code or none>
    console <hex bytes>
    mem <octal address> <16 hex digits or undef>
(the model also lists registers, which are ignored here: tests dump the registers
they care about to memory).  A word the model marks undef is not compared: its
value is something the model cannot predict, such as the reset-state package the
dead start writes to words 0-17.

Exit status 0 when they agree.  usage: cmp_state.py RTL.state MODEL.state [--max N]
"""
import sys


def load(path):
    out = {'exit': None, 'console': '', 'mem': {}}
    for line in open(path):
        p = line.split()
        if not p:
            continue
        if p[0] == 'exit':
            out['exit'] = p[1]
        elif p[0] == 'console':
            out['console'] = p[1] if len(p) > 1 else ''
        elif p[0] == 'mem':
            out['mem'][int(p[1], 8)] = p[2].lower()
    return out


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    limit = int(sys.argv[sys.argv.index('--max') + 1]) if '--max' in sys.argv else 10
    rtl, model = load(sys.argv[1]), load(sys.argv[2])
    problems = []
    if rtl['exit'] != model['exit']:
        problems.append('exit: rtl %s, model %s' % (rtl['exit'], model['exit']))
    if rtl['console'] != model['console']:
        a, b = bytes.fromhex(rtl['console']), bytes.fromhex(model['console'])
        n = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), min(len(a), len(b)))
        problems.append('console differs at byte %d: rtl %r, model %r' % (n, a[n:n + 24], b[n:n + 24]))
    compared = 0
    for addr in sorted(set(rtl['mem']) | set(model['mem'])):
        m, r = model['mem'].get(addr), rtl['mem'].get(addr)
        if m == 'undef':
            continue
        compared += 1
        if m is None:
            problems.append('mem %07o: rtl stored %s, model stored nothing' % (addr, r))
        elif r is None:
            problems.append('mem %07o: model stored %s, rtl stored nothing' % (addr, m))
        elif int(m, 16) != int(r, 16):
            problems.append('mem %07o: rtl %s, model %s' % (addr, r, m))
    for p in problems[:limit]:
        print(p)
    if len(problems) > limit:
        print('... %d more' % (len(problems) - limit))
    print('%s: %d words compared, %d problems' % ('AGREE' if not problems else 'DIFFER', compared, len(problems)))
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())

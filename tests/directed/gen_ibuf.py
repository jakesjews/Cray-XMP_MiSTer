#!/usr/bin/env python3
"""Write ibufhop.cal: a program that hops between more 32-word blocks than
there are instruction buffers.

    gen_ibuf.py [SEED] > ibufhop.cal

Each block counts its visit, steps a pseudo-random number and leaves by one
of three jumps picked from its bits.  The jumps sit at the very end of the
block or a parcel or two before it, so the fetch runs ahead into the next
block while the jump is on its way, and the next block is one of the set.
A block is entered at its start, in its second half or a few parcels before
its jumps, so that either half of a buffer is the one fetched first and the
program reaches the other half while it is still on its way.
With 12 blocks and four buffers, buffers are refilled all the time, and a
buffer being refilled is often asked for the block it held before.  The
reference model has no buffers; it only has to agree at the end.
"""
import random
import sys

BLOCKS = 12
BASE = 0o1000            # a multiple of 40 octal words
WORDS = 32               # a block: what an instruction buffer holds
VISITS = 600


def main():
    r = random.Random(int(sys.argv[1]) if len(sys.argv) > 1 else 1)
    out = []
    e = lambda label, result, operand='': out.append('%-8s %-9s %s' % (label, result, operand))
    out.append('* ibufhop: hops between %d blocks of code with four instruction buffers.' % BLOCKS)
    out.append('* Written by gen_ibuf.py; see there.')
    out.append('         INCLUDE "rt_direct.cal"')
    out.append('         TBEGIN')
    e('', 'A1', '0')
    e('', 'A2', "1")
    e('', 'A3', "1234565")
    e('', 'A4', "7654321")
    e('', 'A5', "D'%d" % VISITS)
    e('', 'J', 'HB0')
    for k in range(BLOCKS):
        a, b, c = (r.randrange(BLOCKS) for _ in range(3))
        shift1, shift2 = r.randrange(41, 60), r.randrange(41, 60)
        tail = r.randrange(0, 4)                      # parcels left free at the end of the block
        body = [('A1', 'A1+1'), ('A5', 'A5-1'), ('A0', 'A5'), ('JAZ', 'HDONE'), ('A2', 'A2*A3'), ('A2', 'A2+A4'),
                ('S2', 'A2'), ('S0', "S2<D'%d" % shift1), ('JSM', 'HB%d' % a), ('S0', "S2<D'%d" % shift2),
                ('JSM', 'HB%d' % b), ('J', 'HB%d' % c)]
        used = sum(2 if x[0][0] == 'J' else 1 for x in body)
        pad = 4 * WORDS - tail - used
        # where the block is entered: its start, its second half, or close to the body
        entry = r.choice([0, 0, 2 * WORDS, 2 * WORDS + r.randrange(8), r.randrange(pad + 1)])
        out.append('         ORG       %o' % (BASE + WORDS * k))
        for _ in range(entry):
            e('', 'A6', 'A6+1')                        # never reached
        first = True
        for _ in range(pad - entry):
            e('HB%d' % k if first else '', 'PASS'); first = False
        for res, op in body:
            e('HB%d' % k if first else '', res, op); first = False
        for _ in range(tail):
            e('', 'A6', 'A6+1')                        # never reached
    out.append('         ORG       %o' % (BASE + WORDS * BLOCKS))
    e('HDONE', 'A7', '0')
    e('', 'TPASS')
    e('', 'TEND', 'DUMPAS')
    e('', 'END')
    sys.stdout.write('\n'.join(out) + '\n')


main()

#!/usr/bin/env python3
"""Write vload.cal: vector loads over every alignment, step and length that
matters to the burst reads of the memory unit.

    python3 tests/directed/gen_vload.py > tests/directed/vload.cal

Each case loads a vector from a pattern area and stores it, one word after
another, in its own 64-word slot.  The reference model predicts every slot.
Steps 1 to 9 cover burst reads (1 to 7) and single words (8 and 9), as do -1
and 0; starts 0 to 15 cover every place in a 16-word line; lengths cover one
word, a few, a line, more than a line and the full 64.
"""
SRC = 0o40000      # pattern area
OUT = 0o100000     # result slots
LENGTHS = [1, 2, 3, 4, 15, 16, 17, 33, 64]
STEPS = [1, 2, 3, 4, 5, 7, 8, 9, 0, -1]

lines = []
e = lines.append
e("* vload: vector loads at every alignment, step and length (see gen_vload.py).")
e("         ORG     0")
e("         CON     P.START*O'100000000")
e("         CON     0")
e("         CON     O'7777774100000000")
e("         CON     0")
e("         CON     0")
e("         CON     O'7777774000000000")
e("         BSSZ    D'10")
e("TEXIT    =       O'17777762")
e("         ORG     O'200")
e("START    A7      0")
slot = 0
for n in LENGTHS:
    e("         A1      D'%d" % n)
    e("         VL      A1")
    for step in STEPS:
        for start in range(16):
            if step < 0:
                first = SRC + 0o2000 + start             # runs downwards
            else:
                first = SRC + 0o20 + start
            e("         A0      O'%o" % first)
            if step == 1:
                e("         V%d      ,A0,1" % (slot % 7 + 1))
            else:
                e("         A2      %s" % (("-D'%d" % -step) if step < 0 else ("D'%d" % step)))
                e("         V%d      ,A0,A2" % (slot % 7 + 1))
            e("         A0      O'%o" % (OUT + 64 * slot))
            e("         ,A0,1   V%d" % (slot % 7 + 1))
            slot += 1
e("         S1      0")
e("         TEXIT,0 S1")
e("HANG     J       HANG")
e("         ORG     O'%o" % SRC)
for i in range(0o2100):
    e("         CON     X'%016X" % ((i * 0x9E3779B97F4A7C15 + 0x0123456789ABCDEF) & (2**64 - 1)))
e("         END")
print("\n".join(lines))

#!/usr/bin/env python3
"""Write timing.cal: how many clock periods instructions take, against the
numbers of the X-MP's manual (CSM-0111000 section 5).

    python3 tests/rtl_only/gen_timing.py > tests/rtl_only/timing.cal
    python3 tests/rtl_only/gen_timing.py esvl > tests/rtl_only/timing_esvl.cal

The second is the cases of the second vector logical unit: a program whose
exchange package has the ESVL bit set.

A case is a short sequence of instructions between two readings of the
real-time clock, which counts clock periods.  Its time is the difference of
the readings less the one clock period of the first reading.  Every case
runs from instruction buffers that are filled before the first reading, so
the times do not depend on how fast memory is.  The reference model has no
clock periods; the program checks itself.
"""
import sys

PASS = ['PASS']
# (clock periods, what, instructions).  %L is the label of the second reading.
ONE_BUFFER = [
    (0, 'nothing', []),
    (1, 'a one-parcel instruction', PASS),
    (2, 'a two-parcel instruction (020)', ['A1 12345']),
    (2, 'the 24-bit constant (01h)', ['A1 40000000']),
    # branches, X 5-19 to 5-26: 5 CPs taken with the address in a buffer, 2 CPs not taken,
    # 7 CPs for 005
    (5, '006 J', ['J %L']),
    (5, '007 R', ['R %L']),
    (7, '005 J Bjk', ['J B02']),
    (5, '010 taken', ['JAZ %L']),
    (2, '011 not taken', ['JAN %L']),
    (5, '014 taken', ['JSZ %L']),
    (2, '015 not taken', ['JSN %L']),
    # "A0 busy in any one of the previous 3 CPs" holds a branch on A0: after a 022,
    # whose result takes 1 CP, the branch starts 4 CPs after the 022 at the earliest
    (9, '022 to A0, then 010 taken', ['A0 0', 'JAZ %L']),
    (9, '022 to A0, 1 CP, 010 taken', ['A0 0'] + PASS * 1 + ['JAZ %L']),
    (9, '022 to A0, 2 CPs, 010 taken', ['A0 0'] + PASS * 2 + ['JAZ %L']),
    (9, '022 to A0, 3 CPs, 010 taken', ['A0 0'] + PASS * 3 + ['JAZ %L']),
    (10, '022 to A0, 4 CPs, 010 taken', ['A0 0'] + PASS * 4 + ['JAZ %L']),
    (6, '022 to A0, then 011 not taken', ['A0 0', 'JAN %L']),
    (6, '022 to A0, 3 CPs, 011 not taken', ['A0 0'] + PASS * 3 + ['JAN %L']),
    # after an address add of 2 CPs
    (10, '030 to A0, then 011 taken', ['A0 A3+A3', 'JAN %L']),
    (10, '030 to A0, 4 CPs, 011 taken', ['A0 A3+A3'] + PASS * 4 + ['JAN %L']),
    # the same for S0: a logical product of 1 CP, a sum of 3
    (9, '044 to S0, then 014 taken', ['S0 S4&S5', 'JSZ %L']),
    (9, '044 to S0, 3 CPs, 014 taken', ['S0 S4&S5'] + PASS * 3 + ['JSZ %L']),
    (11, '060 to S0, then 014 taken', ['S0 S4+S5', 'JSZ %L']),
    (11, '060 to S0, 5 CPs, 014 taken', ['S0 S4+S5'] + PASS * 5 + ['JSZ %L']),
    # "instruction 025 issued in the previous CP" holds 005
    (9, '025, then 005', ['B02 A4', 'J B02']),
    # Vectors, X 5-73 to 5-87.  From the issue of a vector instruction its operand registers
    # are ready after (VL) + 3 CPs, its unit after (VL) + 4 and its result register after
    # (VL) + 5 + the unit time: logical 2, add 3, shift 3 but 4 for 152, floating add 6,
    # multiply 7, reciprocal 14, population count 5.  The instruction that waits for one of
    # them issues then, and the second reading is one clock period behind it.
    (9, '141, VL 1, and its element read back', ['V2 V1&V3', 'S1 V2,A2']),
    (10, '155, VL 1, and its element read back', ['V2 V1+V3', 'S1 V2,A2']),
    (10, '150, VL 1, and its element read back', ['V2 V1<A1', 'S1 V2,A2']),
    (10, '151, VL 1, and its element read back', ['V2 V1>A1', 'S1 V2,A2']),
    (11, '152, VL 1, and its element read back', ['V2 V1,V1<A1', 'S1 V2,A2']),
    (10, '153, VL 1, and its element read back', ['V2 V1,V1>A1', 'S1 V2,A2']),
    (13, '171, VL 1, and its element read back', ['V2 V1+FV3', 'S1 V2,A2']),
    (14, '161, VL 1, and its element read back', ['V2 V1*FV3', 'S1 V2,A2']),
    (21, '174, VL 1, and its element read back', ['V2 /HV1', 'S1 V2,A2']),
    (12, '174ij1, VL 1, and its element read back', ['V2 PV1', 'S1 V2,A2']),
    (5, '155, VL 1, then its operand register written', ['V2 V1+V3', 'V1 V4&V4']),
    (6, '155, VL 1, then its unit used again', ['V2 V1+V3', 'V5 V4+V6']),
    (14, '155, VL 5, and an element read back', ['V2 V1+V3', 'S1 V2,A2'], 5),
    (9, '155, VL 5, then its operand register written', ['V2 V1+V3', 'V1 V4&V4'], 5),
    (10, '155, VL 5, then its unit used again', ['V2 V1+V3', 'V5 V4+V6'], 5),
    (15, '152, VL 5, and an element read back', ['V2 V1,V1<A1', 'S1 V2,A2'], 5),
    (16, '174ij1, VL 5, and an element read back', ['V2 PV1', 'S1 V2,A2'], 5),
    # 175, X 5-85: the mask is ready (VL) + 4 CPs after issue, for 073 a clock period later
    (6, '175, VL 1, then a merge', ['VM V1,Z', 'V2 V3!V4&VM']),
    (7, '175, VL 1, then 073', ['VM V1,Z', 'S1 VM']),
    (5, '175, VL 1, then its operand register written', ['VM V1,Z', 'V1 V4+V4']),
    (10, '175, VL 5, then a merge', ['VM V1,Z', 'V2 V3!V4&VM'], 5),
    (11, '175, VL 5, then 073', ['VM V1,Z', 'S1 VM'], 5),
    # the compress index 175ijk, k from 4 to 7: Vi is ready (VL) + 10 CPs after issue,
    # the mask as for the plain 175
    (12, '175ij4, VL 1, and an element of its Vi read back', ['V2,VM V1,Z', 'S1 V2,A2']),
    (16, '175ij5, VL 5, and an element of its Vi read back', ['V2,VM V1,N', 'S1 V2,A2'], 5),
    (11, '175ij4, VL 5, then 073', ['V2,VM V1,Z', 'S1 VM'], 5),
    (10, '175ij4, VL 5, then a merge', ['V2,VM V1,Z', 'V5 V3!V4&VM'], 5),
    # 076: Si ready in 4 CPs (X 5-63)
    (5, '076, then its result used', ['S1 V2,A2', 'S2 S1&S1']),
    # 077: Vi ready in 1 CP.  It has no S result, so nothing on its way to an S register
    # holds it but its own Sj.
    (2, '077 behind a 072, then a PASS', ['V2,A2 S1', 'PASS']),
    (2, '077, then a 076 of its register', ['V2,A2 S1', 'S1 V2,A2']),
    (2, '077, then a 155 of its register', ['V2,A2 S1', 'V5 V2+V2']),
    # An instruction holds for the register it writes and the registers it reads: not for
    # a register whose number happens to be in a field that names none (a shift count, a
    # constant).  064 has S1 reserved for 7 CPs, 032 has A1 for 4.  (The PASS keeps the
    # result of the second reading out of the clock period of the shift's.)
    (3, '064 to S1, then 054 with a count of 10, then a PASS', ['S1 S4*FS5', 'S3 S3<10', 'PASS']),
    (2, '032 to A1, then 022 with the constant 10', ['A1 A3*A3', 'A4 10']),
    # Results of different units that are due in the same clock period do not wait for
    # each other: the manual has no such hold among its conditions.  A sum of 3 CPs and a
    # shift of 2 behind it; a product of 7 CPs with a sum, a shift and a logical product
    # that all arrive with it; an address product of 4 CPs with a sum and a constant.
    (3, '060, then 054 due with it, then a PASS', ['S1 S4+S5', 'S3 S3<10', 'PASS']),
    (6, '064, 3 CPs, then 060 due with it, then a PASS', ['S1 S4*FS5'] + PASS * 3 + ['S2 S4+S5', 'PASS']),
    (8, '064, 3 CPs, then 060, 054 and 044 all due with it', ['S1 S4*FS5'] + PASS * 3 + ['S2 S4+S5', 'S3 S3<10', 'S0 S4&S5', 'PASS']),
    (9, '064, 060, 054 and 044 due together, then all four used', ['S1 S4*FS5'] + PASS * 3 + ['S2 S4+S5', 'S3 S3<10', 'S0 S4&S5', 'S4 S1&S2', 'S5 S3&S0']),
    (4, '032, 1 CP, then 030 due with it, then a PASS', ['A1 A3*A3', 'PASS', 'A2 A3+A3', 'PASS']),
    (5, '032, 1 CP, then 030 and 022 due with it', ['A1 A3*A3', 'PASS', 'A2 A3+A3', 'A4 10', 'PASS']),
    # Chaining, X 4-12.  A register that is still to receive the result of an earlier
    # instruction does not hold issue as an operand: the operation takes each element when
    # it is there, and has it at its unit 4 CPs after it arrived at the register, as it has
    # an element 4 CPs after issue.  Element 0 of a 155 arrives 8 CPs after issue.  A 141
    # that issues by then loses nothing (full chaining): its result is ready (VL) + 15 CPs
    # after the 155 issued, its operand (VL) + 11 and its unit (VL) + 12.  One that issues
    # later is as much behind (partial chaining).
    (17, '155, VL 1, a 141 chained to it, and its element read back', ['V2 V1+V3', 'V4 V2&V5', 'S1 V4,A2']),
    (21, '155, VL 5, a 141 chained to it, and an element read back', ['V2 V1+V3', 'V4 V2&V5', 'S1 V4,A2'], 5),
    (17, '155, VL 5, a 141 chained to it, then the register between them read', ['V2 V1+V3', 'V4 V2&V5', 'V6 V2+V7'], 5),
    (18, '155, VL 5, a 141 chained to it, then its unit used again', ['V2 V1+V3', 'V4 V2&V5', 'V6 V7&V7'], 5),
    (21, '155, VL 5, 4 CPs, a 141 chained to it', ['V2 V1+V3'] + PASS * 4 + ['V4 V2&V5', 'S1 V4,A2'], 5),
    (21, '155, VL 5, a 141 that issues as element 0 arrives', ['V2 V1+V3'] + PASS * 7 + ['V4 V2&V5', 'S1 V4,A2'], 5),
    (22, '155, VL 5, a 141 a CP after element 0 arrived', ['V2 V1+V3'] + PASS * 8 + ['V4 V2&V5', 'S1 V4,A2'], 5),
    (24, '155, VL 5, a 141 3 CPs after element 0 arrived', ['V2 V1+V3'] + PASS * 10 + ['V4 V2&V5', 'S1 V4,A2'], 5),
    (22, '155, VL 5, a 150 chained to it', ['V2 V1+V3', 'V4 V2<A1', 'S1 V4,A2'], 5),
    (23, '155, VL 5, a 152 chained to it', ['V2 V1+V3', 'V4 V2,V2<A1', 'S1 V4,A2'], 5),
    (22, '155, VL 5, a 153 chained to it', ['V2 V1+V3', 'V4 V2,V2>A1', 'S1 V4,A2'], 5),
    (24, '155, VL 5, a 174ij1 chained to it', ['V2 V1+V3', 'V4 PV2', 'S1 V4,A2'], 5),
    (19, '155, VL 5, a 175 chained to it, then 073', ['V2 V1+V3', 'VM V2,Z', 'S1 VM'], 5),
    (37, '174, VL 5, a 161 chained to it', ['V3 /HV2', 'V5 V1*FV3', 'S1 V5,A2'], 5),
    (32, '155, 141 and 171 in a chain, VL 5', ['V2 V1+V3', 'V4 V2&V5', 'V6 V4+FV7', 'S1 V6,A2'], 5),
    # the unit of the second 155 is busy for (VL) + 4 CPs: it issues with the result on its way
    (23, '155, VL 5, a 155 of its result', ['V2 V1+V3', 'V4 V2+V5', 'S1 V4,A2'], 5),
    # 176 and 177 issue in 1 CP (X 5-91), and the instruction behind them issues in the
    # next when a look at the first address and the step shows that the transfer stays
    # inside the field: a step of less than 2, 16 or 1,024 words from an address 64, 1,024
    # or 65,536 words below the end.  For a longer step the last address is worked out
    # first, which holds the instruction behind for two CPs more.
    (2, '176 by 1, then a PASS', ['V7 ,A0,1', 'PASS']),
    (3, '022, 176 by that step, then a PASS', ['A5 2', 'V7 ,A0,A5', 'PASS']),
    (6, '020, 176 by that step of 2000, then a PASS', ['A5 2000', 'V7 ,A0,A5', 'PASS']),
    (4, '020 to A0, 177 by 1, then a PASS', ['A0 12000', ',A0,1 V7', 'PASS']),
    # The divide of X 4-36: reciprocal, a product chained to it, the correction when the
    # multiply unit is free, and the product of the two.  3 * 64 CPs and 39 (the manual has
    # the 38 of the CRAY-1, whose chaining was another).
    (232, 'a divide of 64 elements', ['V3 /HV2', 'V5 V1*FV3', 'V4 V3*IV2', 'V6 V4*FV5', 'S1 V6,A2'], 64),
]
# Cases over two blocks X and Y, both in buffers: (clock periods, what, instructions, how it ends)
#   'fall'      the sequence ends with the last parcel of X; the second reading is the first parcel of Y
#   'straddle'  its last instruction has two parcels, the second being the first parcel of Y
#   'jump'      it is in X and goes to the second reading at the start of Y by itself
# "Second parcel in different buffer, 2-CP delay", and the same for running on into another
# buffer; a branch that is taken costs its 5 CPs whichever buffer its address is in.
TWO_BUFFERS = [
    (6, 'four one-parcel instructions, then on into another buffer', PASS * 4, 'fall'),
    (4, 'a two-parcel instruction across two buffers', ['A1 12345'], 'straddle'),
    (5, '006 to another buffer', ['J %L'], 'jump'),
    (5, '010 taken to another buffer', ['JAZ %L'], 'jump'),
    (4, '011 not taken, next instruction in another buffer', ['JAN %L'], 'fall'),
    (7, '006 across two buffers', ['J %L'], 'straddle'),
    (4, '011 across two buffers, not taken', ['JAN %L'], 'straddle'),
    (7, '010 across two buffers, taken', ['JAZ %L'], 'straddle'),
]
# With the second vector logical unit enabled (X 4-18): 140 to 145 go to it when it and
# the floating point multiply unit, which share a busy signal, are free, and to the full
# unit otherwise.  Its unit time is 4 CPs, so a result register is ready (VL) + 9 CPs
# after issue.  The merges and 175 have the full unit only.
ESVL = [
    (11, '141 in the second unit, VL 1, and its element read back', ['V2 V1&V3', 'S1 V2,A2']),
    (15, '141 in the second unit, VL 5, and an element read back', ['V2 V1&V3', 'S1 V2,A2'], 5),
    (9, '147 in the full unit, VL 1, and its element read back', ['V2 V3!V4&VM', 'S1 V2,A2']),
    # two at once: the second unit, then the full one
    (14, 'two 141, VL 5, and an element of the second read back', ['V2 V1&V3', 'V5 V4&V6', 'S1 V5,A2'], 5),
    # a third waits for the unit that is free first: the second, (VL) + 4 CPs after the first issued
    (24, 'three 141, VL 5, and an element of the third read back', ['V2 V1&V3', 'V5 V4&V6', 'V7 V0&V0', 'S1 V7,A2'], 5),
    # the multiply unit is busy while the second logical unit is, for vectors and scalars
    (27, '141 in the second unit, VL 5, then 161', ['V2 V1&V3', 'V5 V4*FV6', 'S1 V5,A2'], 5),
    (17, '141 in the second unit, VL 5, then 064', ['V2 V1&V3', 'S1 S2*FS3', 'S4 S1&S1'], 5),
    # and a 141 behind a multiply goes to the full unit
    (14, '161, VL 5, then 141 in the full unit', ['V5 V4*FV6', 'V2 V1&V3', 'S1 V2,A2'], 5),
    # chained to an add; and a 175 chained to it, which the full unit alone could not be
    (23, '155, VL 5, a 141 in the second unit chained to it', ['V2 V1+V3', 'V4 V2&V5', 'S1 V4,A2'], 5),
    (20, '141 in the second unit, VL 5, a 175 chained to it, then 073', ['V2 V1&V3', 'VM V2,Z', 'S1 VM'], 5),
]
FIRST = {'A1': 0o020100, 'J': 0o006000, 'JAN': 0o011000, 'JAZ': 0o010000}   # first parcels, for 'straddle'
BASE, OUT = 0o400, 0o10000        # the cases from BASE on, their times from OUT on
BLOCK = 0o40                      # words in an instruction buffer


def two_parcels(x):
    return x.split()[0] in ('J', 'R', 'JAZ', 'JAN', 'JSZ', 'JSN') and x != 'J B02' or x.startswith('A1 ')


def line(label, result, operand=''):
    return ('%-8s %-7s %s' % (label, result, operand)).rstrip()


def main():
    global ONE_BUFFER, TWO_BUFFERS
    esvl = sys.argv[1:] == ['esvl']
    if esvl:
        ONE_BUFFER, TWO_BUFFERS = ESVL, []
    count = len(ONE_BUFFER) + len(TWO_BUFFERS)
    src = ['* timing%s: clock periods of instructions against the X-MP manual%s.'
           % (('_esvl', ', with the second vector logical unit enabled') if esvl else ('', '')),
           '* Written by gen_timing.py; see there.  %d cases; a wrong one fails check 2' % count,
           '* and leaves its number in word %o and its clock periods in word %o.' % (OUT + 0o400, OUT + 0o401),
           '* NOSTEP: issue held to one instruction at a time changes every time measured here.',
           '         INCLUDE "rt_direct.cal"']
    if esvl:
        # the bit at the left end of word 3 of the package the dead start loads
        src += [line('', 'ORG', '3'), line('', 'CON', '1000000000000000000000')]
    src += ['         TBEGIN',
           line('', 'A1', '1'), line('', 'VL', 'A1'), line('', 'A2', '0'), line('', 'A3', '3'), line('', 'J', 'C0')]
    for n, (cps, what, seq, *vl) in enumerate(ONE_BUFFER):
        src.append('* %d clock periods: %s' % (cps, what))
        src.append(line('', 'ORG', '%o' % (BASE + BLOCK * n)))
        src += [line('C%d' % n, 'A4', "D'%d" % (vl[0] if vl else 1)), line('', 'VL', 'A4')]
        src += [line('', 'A4', 'L%d' % n), line('', 'B02', 'A4'), line('', 'A0', '0'), line('', 'S0', '0')]
        src += [line('', 'PASS')] * 16                 # what the block before left on its way is done
        src.append(line('', 'S6', 'RT'))
        for x in seq:
            res, *op = x.replace('%L', 'L%d' % n).split()
            src.append(line('', res, ' '.join(op)))
        src += [line('L%d' % n, 'S7', 'RT'), line('', 'S7', 'S7-S6'), line('', '%o,0' % (OUT + n), 'S7'),
                line('', 'J', 'C%d' % (n + 1) if n + 1 < len(ONE_BUFFER) else 'P0' if TWO_BUFFERS else 'FIN')]
    pbase = BASE + BLOCK * (len(ONE_BUFFER) + 2)
    for n, (cps, what, seq, end) in enumerate(TWO_BUFFERS):
        x, y = pbase + 2 * BLOCK * n, pbase + 2 * BLOCK * n + BLOCK
        parcels = sum(2 if two_parcels(i) else 1 for i in seq)
        # J YP; the wait of 6 parcels; A0 0, S0 0; S6 RT; the sequence
        used = 2 + 6 + 2 + 1 + parcels
        pad = {'fall': 4 * BLOCK - used, 'straddle': 4 * BLOCK + 1 - used, 'jump': 20}[end]
        src.append('* %d clock periods: %s' % (cps, what))
        src.append(line('', 'ORG', '%o' % x))
        src.append(line('P%d' % n, 'J', 'YP%d' % n))                 # into Y first, so that it is in a buffer
        # both halves of both blocks are in their buffers when the wait is over
        src += [line('XM%d' % n, 'A7', "D'400"), line('W%d' % n, 'A7', 'A7-1'), line('', 'A0', 'A7'), line('', 'JAN', 'W%d' % n)]
        src += [line('', 'A0', '0'), line('', 'S0', '0')] + [line('', 'PASS')] * pad + [line('', 'S6', 'RT')]
        for i in seq[:-1] if end == 'straddle' else seq:
            res, *op = i.replace('%L', 'Y%d' % n).split()
            src.append(line('', res, ' '.join(op)))
        if end == 'straddle':
            # the second parcel of the last instruction is the first of Y: both by hand
            name = seq[-1].split()[0]
            value = 0o12345 if name == 'A1' else y * 4 + 1
            assert value < 0x10000
            src.append(line('', 'VWD', "D'16/O'%06o" % FIRST[name]))
            src.append(line('', 'ORG', '%o' % y))
            src.append(line('', 'VWD', "D'16/O'%06o" % value))
        else:
            src.append(line('', 'ORG', '%o' % y))
        src += [line('Y%d' % n, 'S7', 'RT'), line('', 'S7', 'S7-S6'), line('', '%o,0' % (OUT + len(ONE_BUFFER) + n), 'S7'),
                line('', 'J', 'P%d' % (n + 1) if n + 1 < len(TWO_BUFFERS) else 'FIN'),
                line('YP%d' % n, 'J', 'XM%d' % n)]
    last = pbase + 2 * BLOCK * len(TWO_BUFFERS) + BLOCK
    assert last + 0o100 + count < OUT, 'the cases reach the words their times are kept in'
    src.append(line('', 'ORG', '%o' % last))
    # compare every time with what it should be
    src += [line('FIN', 'A1', '0'), line('', 'A2', "D'%d" % count),
            line('FL', 'S1', '%o,A1' % OUT), line('', 'S2', 'KEXP,A1'), line('', 'S0', 'S1\\S2'), line('', 'JSN', 'FBAD'),
            line('', 'A1', 'A1+1'), line('', 'A0', 'A2-A1'), line('', 'JAN', 'FL'),
            line('', 'TPASS'), line('', 'J', 'T$END'),
            line('FBAD', '%o,0' % (OUT + 0o400), 'A1'), line('', 'S2', '1'), line('', 'S1', 'S1-S2'),
            line('', '%o,0' % (OUT + 0o401), 'S1'), line('', 'TFAIL', '2'), line('', 'TEND')]
    for n, case in enumerate(ONE_BUFFER + TWO_BUFFERS):
        src.append(line('KEXP' if n == 0 else '', 'CON', "D'%d" % (case[0] + 1)))
    src.append(line('', 'END'))
    print('\n'.join(src))


main()

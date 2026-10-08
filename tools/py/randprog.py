#!/usr/bin/env python3
"""Constrained-random CAL test programs for differential testing.

    randprog.py SEED [-n INSTRUCTIONS] [-o OUT.cal] [--no-vector] [--no-float] [--no-mem]

A program loads every register from a seeded data pool, the shared registers
of cluster 1 among them, runs a random body and then stores all registers to
a dump area, so the whole machine state ends up in memory where the reference
model and the hardware simulation can be compared.

Constraints that keep a program meaningful:
  - it terminates: branches go forward, and loops count down a reserved register
  - memory references stay inside a sandbox that the image initialises; the
    address registers they use (A0 to A5, A7) are set immediately beforehand
  - no monitor instructions, channel status or real-time clock (the model cannot
    predict a clock), and nothing is stored into code
  - floating point interrupts are left off, so any bit pattern may be an operand
  - a test and set only ever follows the clearing of its semaphore: the
    program runs in monitor mode, where a set semaphore would hold it for good

Reserved registers: A6 loop counter, A7 sandbox base.
"""
import random
import sys

SAND = 0o40000        # 512-word sandbox, base register A7 points at its middle
SAND_WORDS = 512
DUMP = 0o50000
DATA = 0o60000
MASK64 = (1 << 64) - 1


class Gen:
    def __init__(self, seed, n, vector=True, floating=True, memory=True):
        self.r = random.Random(seed)
        self.n = n
        self.vector, self.floating, self.memory = vector, floating, memory
        self.lines = []
        self.label_n = 0
        self.pending_label = ''
        self.data = []            # data pool words

    # ---- helpers ----
    def emit(self, result, operand='', label=''):
        if self.pending_label:
            if label:
                self.lines.append('%-8s %-9s' % (self.pending_label, 'PASS'))
            else:
                label = self.pending_label
            self.pending_label = ''
        self.lines.append('%-8s %-9s %s' % (label, result, operand))

    def place(self, name):
        """The next instruction emitted carries this label."""
        if self.pending_label:
            self.lines.append('%-8s %-9s' % (self.pending_label, 'PASS'))
        self.pending_label = name

    def label(self):
        self.label_n += 1
        return 'L%d' % self.label_n

    def word(self):
        r = self.r
        k = r.randrange(10)
        if k == 0: return r.choice([0, 1, MASK64, 1 << 63, (1 << 63) - 1, 0x4001800000000000])
        if k == 1: return r.getrandbits(r.choice([1, 6, 12, 24]))
        if k in (2, 3):  # a normalised floating point number with a modest exponent
            return (r.getrandbits(1) << 63) | ((0o40000 + r.randrange(-40, 40)) << 48) | (1 << 47) | r.getrandbits(47)
        if k == 4: return (-r.getrandbits(r.choice([3, 10, 30]))) & MASK64
        return r.getrandbits(64)

    def pool(self, value=None):
        self.data.append(self.word() if value is None else value & MASK64)
        return DATA + len(self.data) - 1

    def shaped(self):
        """A 22-bit constant whose low parcel looks like an instruction.

        The second parcel of a two-parcel instruction passes through the same
        registers as real instructions; if anything decodes it by mistake, a
        constant shaped like an exit, a VL or VM load, a shift or a register
        transfer will show it.
        """
        r = self.r
        op = r.choice([0o000, 0o004, 0o002, 0o003, 0o013, 0o014, 0o025, 0o026, 0o027, 0o052, 0o053, 0o054,
                       0o055, 0o056, 0o057, 0o075, 0o076, 0o077, 0o034, 0o035, 0o176, 0o177, 0o140, 0o175])
        return (r.getrandbits(6) << 16) | (op << 9) | r.getrandbits(9)

    def imm(self, bits):
        return self.shaped() if self.r.randrange(3) == 0 else self.r.getrandbits(bits)

    def areg(self):   return self.r.randrange(0, 6)          # A0-A5 may be written
    def aany(self):   return self.r.randrange(0, 8)
    def sreg(self):   return self.r.randrange(0, 8)
    def breg(self):   return self.r.randrange(1, 64)         # B00 is the return address register
    def treg(self):   return self.r.randrange(0, 64)
    def vreg(self):   return self.r.randrange(0, 8)

    # ---- program ----
    def preamble(self):
        e = self.emit
        e('IDENT', 'RAND')
        self.lines.append("TEXIT    =         O'17777762")
        e('ORG', '0')
        e('CON', "P.START*O'100000000")
        e('CON', '0')
        e('CON', "O'7777774100000000")           # the largest ILA, monitor mode
        e('CON', "O'0000000030000000000000")
        e('CON', "O'100000000")                  # DBA 0, cluster 1
        e('CON', "O'7777774000000000")           # the largest DLA
        e('BSSZ', "D'10")
        e('ORG', "O'40")
        first = True
        # the shared registers of cluster 1, through S1 and A1
        for j in range(8):
            e('S1', "O'%o,0" % self.pool(), 'START' if first else ''); first = False
            e('ST%d' % j, 'S1')
            e('A1', "O'%o,0" % self.pool())
            e('SB%d' % j, 'A1')
        e('S1', "O'%o,0" % self.pool())
        e('SM', 'S1')
        # every register from the pool
        for i in range(8):
            e('S%d' % i, "O'%o,0" % self.pool(), 'START' if first else ''); first = False
        for i in range(8):
            e('A%d' % i, "O'%o,0" % self.pool())
        e('A6', "D'64"); e('VL', 'A6')
        for i in range(8):
            a = DATA + len(self.data)
            for _ in range(64): self.pool()
            e('A0', "O'%o" % a); e('V%d' % i, ',A0,1')
        a = DATA + len(self.data)
        for _ in range(64): self.pool(self.r.getrandbits(24))
        e('A0', "O'%o" % a); e('A6', "D'64"); e('B00,A6', '0,A0')
        a = DATA + len(self.data)
        for _ in range(64): self.pool()
        e('A0', "O'%o" % a); e('T00,A6', '0,A0')
        e('S0', "O'%o,0" % self.pool()); e('VM', 'S0')
        e('S0', "O'%o,0" % self.pool())
        e('A0', "O'%o,0" % self.pool())
        e('A7', "O'%o" % (SAND + SAND_WORDS // 2))
        e('A6', '0')

    def op_a(self):
        r, e = self.r, self.emit
        i = self.areg()
        k = r.randrange(11)
        if k == 0: e('A%d' % i, "O'%o" % self.imm(r.choice([4, 6, 16, 22])))
        elif k == 1: e('A%d' % i, "#O'%o" % self.imm(r.choice([4, 16, 22])))
        elif k == 2: e('A%d' % i, 'S%d' % self.sreg())
        elif k == 3: e('A%d' % i, 'B%02o' % self.breg())
        elif k == 4: e('B%02o' % self.breg(), 'A%d' % self.aany())
        elif k == 5: e('A%d' % i, r.choice(['PS%d', 'QS%d']) % self.sreg())
        elif k == 6: e('A%d' % i, 'ZS%d' % self.sreg())
        elif k in (7, 8): e('A%d' % i, 'A%d%sA%d' % (self.aany(), r.choice('+-'), self.aany()))
        else: e('A%d' % i, 'A%d*A%d' % (self.aany(), self.aany()))

    def op_s(self):
        r, e = self.r, self.emit
        i, j, k = self.sreg(), self.sreg(), self.sreg()
        c = r.randrange(20)
        if c == 0: e('S%d' % i, "O'%o" % self.imm(r.choice([5, 16, 22])))
        elif c == 1: e('S%d' % i, "#O'%o" % self.imm(r.choice([5, 22])))
        elif c == 2: e('S%d' % i, "%sD'%d" % (r.choice('<>'), r.randrange(1, 64)))
        elif c == 3: e('S%d' % i, 'S%d&S%d' % (j, k))
        elif c == 4: e('S%d' % i, '#S%d&S%d' % (k, j))
        elif c == 5: e('S%d' % i, 'S%d\\S%d' % (j, k))
        elif c == 6: e('S%d' % i, '#S%d\\S%d' % (j, k))
        elif c == 7: e('S%d' % i, 'S%d!S%d&S%d' % (j, i, k))
        elif c == 8: e('S%d' % i, 'S%d!S%d' % (j, k))
        elif c == 9: e('S0', "S%d%sD'%d" % (i, r.choice('<>'), r.randrange(1, 64)))
        elif c == 10: e('S%d' % i, "S%d%sD'%d" % (i, r.choice('<>'), r.randrange(1, 64)))
        elif c == 11:
            a = self.shift_count()
            if r.randrange(2): e('S%d' % i, 'S%d,S%d<A%d' % (i, j, a))
            else: e('S%d' % i, 'S%d,S%d>A%d' % (j, i, a))
        elif c in (12, 13): e('S%d' % i, 'S%d%sS%d' % (j, r.choice('+-'), k))
        elif c == 14: e('S%d' % i, r.choice(['A%d', '+A%d', '+FA%d']) % self.aany())
        elif c == 15: e('S%d' % i, r.choice(['0.6', '0.4', '1.', '2.', '4.']))
        elif c == 16: e('S%d' % i, 'T%02o' % self.treg())
        elif c == 17: e('T%02o' % self.treg(), 'S%d' % i)
        elif c == 18 and self.vector: e('S%d' % i, 'VM')
        else: e('S%d' % i, 'S%d+S%d' % (j, k))

    def shift_count(self):
        """Put a shift count in an A register and return its number."""
        a = self.r.randrange(1, 6)
        self.emit('A%d' % a, "D'%d" % self.r.choice([0, 1, 2, 7, 31, 63, 64, 65, 100, 127, 128, 200, self.r.randrange(0, 130)]))
        return a

    def op_f(self):
        r, e = self.r, self.emit
        i, j, k = self.sreg(), self.sreg(), self.sreg()
        c = r.randrange(7)
        if c == 6: e('S%d' % i, '/HS%d' % j)
        else: e('S%d' % i, 'S%d%sS%d' % (j, ['+F', '-F', '*F', '*H', '*R', '*I'][c], k))

    def op_mem(self):
        r, e = self.r, self.emit
        c = r.randrange(8)
        if c < 4:      # one word, relative to the sandbox base in A7 or absolute
            how = r.randrange(3)
            if how == 0:
                d = r.randrange(-SAND_WORDS // 2, SAND_WORDS // 2)
                addr = '%s,A7' % (("-O'%o" % -d) if d < 0 else ("O'%o" % d))
            elif how == 1:
                addr = "O'%o,0" % (SAND + r.randrange(SAND_WORDS))
            else:      # any of A1-A5 as the index, pointed into the sandbox first
                h = r.randrange(1, 6)
                base = r.randrange(SAND_WORDS)
                e('A%d' % h, "O'%o" % (SAND + base))
                d = r.choice([0, r.randrange(-base, SAND_WORDS - base)])
                addr = '%s,A%d' % (("-O'%o" % -d) if d < 0 else ("O'%o" % d), h)
            if c == 0: e('A%d' % self.areg(), addr)
            elif c == 1: e(addr, 'A%d' % self.aany())
            elif c == 2: e('S%d' % self.sreg(), addr)
            else: e(addr, 'S%d' % self.sreg())
        else:          # block transfer: count in A1-A5, address in A0
            cnt = r.choice([0, 1, 2, 5, 17, r.randrange(0, 20)])
            a = r.randrange(1, 6)
            e('A%d' % a, "D'%d" % cnt)
            e('A0', "O'%o" % (SAND + r.randrange(SAND_WORDS - 24)))
            if c == 4: e('B%02o,A%d' % (self.breg(), a), '0,A0')
            elif c == 5: e('0,A0', 'B%02o,A%d' % (r.randrange(64), a))
            elif c == 6: e('T%02o,A%d' % (self.treg(), a), '0,A0')
            else: e('0,A0', 'T%02o,A%d' % (self.treg(), a))

    def op_v(self):
        r, e = self.r, self.emit
        i, j, k, s = self.vreg(), self.vreg(), self.vreg(), self.sreg()
        c = r.randrange(24)
        if c == 0:
            a = r.randrange(1, 6)
            e('A%d' % a, "D'%d" % r.choice([0, 1, 2, 3, 5, 6, 8, 9, 16, 17, 63, 64, 65, 100, 127, r.randrange(0, 128)]))
            e('VL', 'A%d' % a)
        elif c == 1: e('VM', 'S%d' % s)
        elif c == 2: e('V%d' % i, 'S%d&V%d' % (s, k))
        elif c == 3: e('V%d' % i, 'V%d&V%d' % (j, k))
        elif c == 4: e('V%d' % i, 'S%d!V%d' % (s, k))
        elif c == 5: e('V%d' % i, 'V%d!V%d' % (j, k))
        elif c == 6: e('V%d' % i, 'S%d\\V%d' % (s, k))
        elif c == 7: e('V%d' % i, 'V%d\\V%d' % (j, k))
        elif c == 8: e('V%d' % i, 'S%d!V%d&VM' % (s, k))
        elif c == 9: e('V%d' % i, 'V%d!V%d&VM' % (j, k))
        elif c in (10, 11):
            a = self.shift_count()
            e('V%d' % i, r.choice(['V%d<A%d', 'V%d>A%d']) % (j, a) if c == 10 else r.choice(['V%d,V%d<A%d', 'V%d,V%d>A%d']) % (j, j, a))
        elif c == 12: e('V%d' % i, 'S%d%sV%d' % (s, r.choice('+-'), k))
        elif c == 13: e('V%d' % i, 'V%d%sV%d' % (j, r.choice('+-'), k))
        elif c == 14 and self.floating: e('V%d' % i, 'S%d%sV%d' % (s, r.choice(['+F', '-F', '*F', '*H', '*R', '*I']), k))
        elif c == 15 and self.floating: e('V%d' % i, 'V%d%sV%d' % (j, r.choice(['+F', '-F', '*F', '*H', '*R', '*I']), k))
        elif c == 16: e('V%d' % i, r.choice(['/HV%d', 'PV%d', 'QV%d'] if self.floating else ['PV%d', 'QV%d']) % j)
        elif c == 17: e('VM', 'V%d,%s' % (j, r.choice('ZNPM')))
        elif c in (18, 19):
            a = r.randrange(1, 6)
            e('A%d' % a, "D'%d" % r.choice([0, 1, 63, 64, 100, r.randrange(64)]))
            if c == 18: e('S%d' % s, 'V%d,A%d' % (j, a))
            else: e('V%d,A%d' % (i, a), 'S%d' % s)
        elif c in (20, 21, 22, 23) and self.memory:
            stride = r.choice([1, 1, 2, 3, 7, 0, -1, -2])
            base = r.randrange(SAND_WORDS) if stride == 0 else (r.randrange(0, SAND_WORDS - 63 * stride) if stride > 0 else r.randrange(-63 * stride, SAND_WORDS))
            a = r.randrange(1, 6)
            e('A%d' % a, ("-O'%o" % -stride) if stride < 0 else "O'%o" % stride)
            e('A0', "O'%o" % (SAND + base))
            if c < 22: e('V%d' % i, ',A0,A%d' % a)
            else: e(',A0,A%d' % a, 'V%d' % j)
        else: e('V%d' % i, 'V%d+V%d' % (j, k))

    def op_raw(self):
        """Instructions with don't-care fields, as raw parcels with those fields random."""
        r = self.r
        def parcel(op, i, j, k):
            self.emit('VWD', "D'16/O'%06o" % ((op << 9) | (i << 6) | (j << 3) | k))
        x = lambda: r.randrange(8)
        c = r.randrange(9 if self.vector else 3)
        if c == 0: parcel(0o023, self.areg(), self.sreg(), x())            # Ai Sj
        elif c == 1: parcel(0o026, self.areg(), self.sreg(), x())          # Ai PSj
        elif c == 2: parcel(0o027, self.areg(), self.sreg(), x())          # Ai ZSj
        elif c == 3: parcel(0o073, self.sreg(), x(), x())                  # Si VM
        elif c == 4:                                                       # VM Sj: not 0034, 0036, 0037
            parcel(0o003, r.choice([0, 1, 2, 3, 5]), self.sreg(), x())
        elif c == 5:                                                       # VL Ak
            a = r.randrange(1, 6)
            self.emit('A%d' % a, "D'%d" % r.choice([0, 1, 5, 63, 64, 65, r.randrange(128)]))
            parcel(0o002, 0, x(), a)
        elif c == 6 and self.floating: parcel(0o174, self.vreg(), self.vreg(), x())   # reciprocal unless k is 1 or 2
        elif c == 7: parcel(0o175, x(), self.vreg(), r.randrange(8))       # mask test: only k & 3 matters
        else: parcel(0o073, self.sreg(), x(), x())

    def op_x(self):
        """The X-MP's own instructions."""
        r, e = self.r, self.emit
        c = r.randrange(14)
        j, jk = r.randrange(8), r.randrange(32)
        if c == 0: e('SB%d' % j, 'A%d' % self.aany())
        elif c in (1, 2): e('A%d' % self.areg(), 'SB%d' % j)
        elif c == 3: e('ST%d' % j, 'S%d' % self.sreg())
        elif c in (4, 5): e('S%d' % self.sreg(), 'ST%d' % j)
        elif c == 6: e('SM', 'S%d' % self.sreg())
        elif c in (7, 8): e('S%d' % self.sreg(), 'SM')
        elif c == 9: e('SM%02o' % jk, r.choice(['0', '1']))
        elif c == 10:
            e('SM%02o' % jk, '0')
            e('SM%02o' % jk, '1,TS')
        elif c == 11: e('S%d' % self.sreg(), 'SR0')
        elif c == 12: e(r.choice(['ERI', 'DRI', 'EBM', 'DBM', 'CMR', 'EFI', 'DFI']))
        else:
            # a store read back at once
            a = self.areg()
            e('SB%d' % j, 'A%d' % self.aany())
            e('A%d' % a, 'SB%d' % j)

    def one(self):
        w = self.r.randrange(100)
        if self.r.randrange(8) == 0: return self.op_x()
        if w < 28: self.op_a()
        elif w < 58: self.op_s()
        elif w < 68 and self.floating: self.op_f()
        elif w < 80 and self.memory: self.op_mem()
        elif w < 96 and self.vector: self.op_v()
        elif w < 99: self.op_raw()
        else: self.op_s()

    def branch(self):
        """A forward conditional branch over a few instructions."""
        r, e = self.r, self.emit
        lab = self.label()
        if r.randrange(2):
            e('A0', 'A%d' % r.randrange(1, 8))
            e(r.choice(['JAZ', 'JAN', 'JAP', 'JAM']), lab)
        else:
            e('S0', 'S%d+S%d' % (self.sreg(), self.sreg()))
            e(r.choice(['JSZ', 'JSN', 'JSP', 'JSM']), lab)
        for _ in range(r.randrange(1, 5)): self.one()
        self.place(lab)

    def body(self):
        r = self.r
        count = 0
        while count < self.n:
            w = r.randrange(100)
            if w < 8:
                self.branch()
            elif w < 11:
                # counted loop on the reserved register A6
                top = self.label()
                self.emit('A6', "D'%d" % r.randrange(1, 5))
                self.place(top)
                for _ in range(r.randrange(1, 6)): self.one()
                self.emit('A6', 'A6-1')
                self.emit('A0', 'A6')
                self.emit('JAN', top)
            elif w < 13:
                # subroutine call and return through B00
                sub, after = self.label(), self.label()
                self.emit('R', sub)
                self.emit('J', after)
                self.place(sub)
                self.emit('S%d' % self.sreg(), 'S%d+S%d' % (self.sreg(), self.sreg()))
                self.emit('J', 'B00')
                self.place(after)
            else:
                self.one()
            count += 1

    def epilogue(self):
        e = self.emit
        for i in range(8): e("O'%o,0" % (DUMP + i), 'A%d' % i)
        for i in range(8): e("O'%o,0" % (DUMP + 0o10 + i), 'S%d' % i)
        e('S1', 'VM'); e("O'%o,0" % (DUMP + 0o20), 'S1')
        for j in range(8):
            e('A1', 'SB%d' % j); e("O'%o,0" % (DUMP + 0o30 + j), 'A1')
            e('S1', 'ST%d' % j); e("O'%o,0" % (DUMP + 0o40 + j), 'S1')
        e('S1', 'SM'); e("O'%o,0" % (DUMP + 0o50), 'S1')
        e('S1', 'SR0'); e("O'%o,0" % (DUMP + 0o51), 'S1')
        e('A1', "D'64")
        e('A0', "O'%o" % (DUMP + 0o100)); e('0,A0', 'B00,A1')
        e('A0', "O'%o" % (DUMP + 0o200)); e('0,A0', 'T00,A1')
        e('VL', 'A1')
        for i in range(8):
            e('A0', "O'%o" % (DUMP + 0o1000 + 0o100 * i)); e(',A0,1', 'V%d' % i)
        e('A1', '0'); e('TEXIT,0', 'A1')
        self.place('HANG'); e('J', 'HANG')
        self.lines.append("         ORG       O'%o" % SAND)
        for _ in range(SAND_WORDS): self.lines.append("         CON       O'%o" % self.word())
        self.lines.append("         ORG       O'%o" % DATA)
        for w in self.data: self.lines.append("         CON       O'%o" % w)
        self.lines.append('         END')

    def program(self):
        self.preamble()
        self.body()
        self.epilogue()
        return '\n'.join(self.lines) + '\n'


def main():
    a = sys.argv[1:]
    if not a:
        sys.exit(__doc__)
    seed = int(a[0])
    n = int(a[a.index('-n') + 1]) if '-n' in a else 200
    g = Gen(seed, n, vector='--no-vector' not in a, floating='--no-float' not in a, memory='--no-mem' not in a)
    text = g.program()
    if '-o' in a:
        open(a[a.index('-o') + 1], 'w').write(text)
    else:
        sys.stdout.write(text)


if __name__ == '__main__':
    main()

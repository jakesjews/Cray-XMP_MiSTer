#!/usr/bin/env python3
"""Write a self-checking program for the I/O Processors.

    selftest.py DIRECTORY

The program takes the place of the I/O Subsystem kernel: DIRECTORY gets the
files cray1-sys and sim/build/ios/Vios_core read (the program as the kernel,
an empty tape, an empty disk).  It checks what the kernel's own start-up does
not depend on:

  1. the real-time clock sets its Done flag
  2. of two channels that ask for an interrupt the lower numbered one is
     reported: the clock, then Buffer Memory, then none
  3. the MIOP starts the BIOP and the XIOP as the kernel does, telling each
     who it is through a parcel it changes in Buffer Memory
  4. every processor sends a word of its own to each of the others, and
     each word arrives where it should and is seen to be taken

Each processor then writes its number and OK on its console (channel 47 on
the MIOP, 43 on the others), or F and a letter: C clock, P Q R priority,
L no word came, D wrong word, T a word was not taken.
"""
import os
import sys


class Program:
    """A program for the I/O Processor: parcels, with labels for branches."""

    def __init__(self):
        self.items = []

    def label(self, name):
        self.items.append(('label', name))

    def ins(self, f, d=0):
        assert 0 <= f < 0o200 and 0 <= d < 0o1000, (f, d)
        self.items.append(('parcel', f << 9 | d))

    def word(self, value):
        self.items.append(('parcel', value))

    def ink(self, f, k, d=0):
        """A two-parcel instruction; k is a number or a label."""
        self.ins(f, d)
        self.items.append(('k', k))

    # every branch goes through operand register 0, which holds zero
    def jump(self, to):
        self.ink(0o075, to)

    def jump_if(self, condition, to):
        self.ink(0o124 + {'C=0': 0, 'C=1': 1, 'A=0': 2, 'A#0': 3}[condition], to)

    def fn(self, channel, function):
        self.ins(0o140 + function, channel)

    def a(self, value):
        """A = value."""
        if value < 0o1000:
            self.ins(0o010, value)
        else:
            self.ink(0o014, value)

    def size(self):
        return sum(1 for kind, _ in self.items if kind != 'label')

    def assemble(self):
        at, labels = 0, {}
        for kind, value in self.items:
            if kind == 'label':
                labels[value] = at
            else:
                at += 1
        out = []
        for kind, value in self.items:
            if kind == 'parcel':
                out.append(value)
            elif kind == 'k':
                out.append(labels[value] if isinstance(value, str) else value)
        return out


WHO = 4          # the parcel that tells a processor its number; word 1 of Buffer Memory
R_WHO, R_AT, R_CONSOLE, R_COUNT, R_WORD, R_TABLE, R_EXPECT, R_SLOT = 1, 2, 3, 4, 5, 6, 7, 8
CLOCK, MOS = 4, 5


def program():
    p = Program()
    n = [0]

    def fresh(stem):
        n[0] += 1
        return '%s%d' % (stem, n[0])

    def wait_done(channel, failure):
        """Wait for Done of a channel; after 65,536 looks it is a failure."""
        loop, done = fresh('wait'), fresh('done')
        p.a(0)
        p.ins(0o024, R_COUNT)
        p.label(loop)
        p.ins(0o040, channel)
        p.jump_if('C=1', done)
        p.ins(0o027, R_COUNT)
        p.jump_if('A#0', loop)
        p.a(ord(failure))
        p.jump('fail')
        p.label(done)

    def mos(local, buffer, function):
        """Copy one word between Local Memory and Buffer Memory and wait for it."""
        p.fn(MOS, 0)
        p.a(buffer)
        p.fn(MOS, 3)
        p.a(0)
        p.fn(MOS, 2)
        p.a(local)
        p.fn(MOS, 1)
        p.a(1)
        p.fn(MOS, function)
        wait_done(MOS, 'M')

    p.jump('start')                      # parcels 0 and 1
    while p.size() < WHO:
        p.word(0)
    for _ in range(4):
        p.word(0)                        # WHO and the rest of its word
    p.label('start')
    p.a(0)
    p.ins(0o024, 0)
    p.a(WHO)
    p.ins(0o024, R_AT)
    p.ins(0o030, R_AT)                   # A = the parcel WHO
    p.ins(0o024, R_WHO)
    # the console: channel 47 on the MIOP, 43 on the others; B selects it
    p.jump_if('A#0', 'other_console')
    p.a(0o47)
    p.jump('console')
    p.label('other_console')
    p.a(0o43)
    p.label('console')
    p.ins(0o024, R_CONSOLE)
    p.ins(0o054)                         # B = A
    p.ins(0o020, R_WHO)
    p.jump_if('A#0', 'links')

    # ---- 1. the clock (the MIOP only)
    wait_done(CLOCK, 'C')
    # ---- 2. the clock and Buffer Memory both ask
    p.fn(CLOCK, 7)
    mos(0x1000, 0, 4)                    # well behind the program
    p.fn(MOS, 7)
    for channel, failure in ((CLOCK, 'P'), (MOS, 'Q'), (0, 'R')):
        p.fn(0, 0o10)                    # IOR : 10
        if channel:
            p.ins(0o013, channel)
        ok = fresh('ok')
        p.jump_if('A=0', ok)
        p.a(ord(failure))
        p.jump('fail')
        p.label(ok)
        if channel:
            p.fn(channel, 0)
            p.fn(channel, 6)
    # ---- 3. start the BIOP (output channel 7) and the XIOP (output channel 13)
    for target, channel in ((1, 0o7), (3, 0o13)):
        p.a(target)
        p.ins(0o034, R_AT)               # the parcel WHO = the number of the target
        mos(WHO, 1, 5)
        p.a(3)
        p.fn(channel, 1)                 # Master Clear and dead start
        for _ in range(8):
            p.ins(0)
        p.a(0)
        p.fn(channel, 1)                 # it loads and starts
        delay = fresh('delay')
        p.a(2000)
        p.label(delay)
        p.ins(0o013, 1)
        p.jump_if('A#0', delay)
        p.a(0)
        p.ins(0o034, R_AT)
        mos(WHO, 1, 5)
    p.fn(MOS, 0)

    # ---- 4. a word to each of the others: 120000 + 20 * number + pair (octal)
    p.label('links')
    p.ins(0o020, R_WHO)
    p.ins(0o005, 4)                      # A = A < 4
    p.ink(0o016, 0xA000)
    p.ins(0o024, R_WORD)
    for slot in range(3):
        p.ins(0o020, R_WORD)
        p.ins(0o012, slot)
        p.fn(7 + 2 * slot, 0o14)
    # what should arrive on each input channel: the table has a row for each number
    p.ins(0o020, R_WHO)
    p.ins(0o005, 1)
    p.ins(0o022, R_WHO)                  # A = 3 * number
    p.ink(0o016, 'table')
    p.ins(0o024, R_TABLE)
    for slot in range(3):
        skip = fresh('skip')
        p.ins(0o030, R_TABLE)
        p.ins(0o024, R_EXPECT)
        p.ins(0o026, R_TABLE)
        p.ins(0o020, R_EXPECT)
        p.jump_if('A=0', skip)           # nobody on that pair
        wait_done(6 + 2 * slot, 'L')
        p.fn(6 + 2 * slot, 0o10)
        p.ins(0o023, R_EXPECT)
        good = fresh('good')
        p.jump_if('A=0', good)
        p.a(ord('D'))
        p.jump('fail')
        p.label(good)
        wait_done(7 + 2 * slot, 'T')     # and the word we sent that way was taken
        p.label(skip)

    def put():
        """Send the character in A to the console and wait until it is out."""
        wait = fresh('out')
        p.ins(0o174)                     # IOB : 14
        p.label(wait)
        p.ins(0o043)                     # C = Busy of channel B
        p.jump_if('C=1', wait)

    def verdict(text):
        p.ins(0o020, R_WHO)
        p.ins(0o012, ord('0'))
        put()
        for c in text:
            p.a(ord(c))
            put()

    verdict(':OK')
    p.label('halt')
    p.jump('halt')
    p.label('fail')
    p.ins(0o024, R_SLOT)
    verdict(':F')
    p.ins(0o020, R_SLOT)
    put()
    p.jump('halt')
    # rows for the MIOP, the BIOP, IOP 2 (there is none) and the XIOP
    p.label('table')
    for row in ((0xA010, 0, 0xA030), (0xA000, 0, 0xA031), (0, 0, 0), (0xA002, 0xA012, 0)):
        for value in row:
            p.word(value)
    return p.assemble()


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(os.path.join(out, 'target/cos_117'), exist_ok=True)
    with open(os.path.join(out, 'target/cos_117/iop_kern.bin'), 'wb') as f:
        for parcel in program():
            f.write(bytes([parcel >> 8, parcel & 0xFF]))
    # a tape with one file mark, and a disk with nothing on it
    with open(os.path.join(out, 'boot_tape.tap'), 'wb') as f:
        f.write(bytes(4))
    open(os.path.join(out, 'exp_disk.img'), 'wb').close()


main()

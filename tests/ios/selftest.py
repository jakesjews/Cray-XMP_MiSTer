#!/usr/bin/env python3
"""Write a self-checking program for the I/O Processors.

    selftest.py DIRECTORY [--cpu]

The program takes the place of the I/O Subsystem kernel: DIRECTORY gets the
files cray-xmp-sys and sim/build/ios/Vios_core read (the program as the kernel,
an empty tape, an empty disk).  It checks what the kernel's own start-up does
not depend on:

  1. the real-time clock sets its Done flag
  2. of two channels that ask for an interrupt the lower numbered one is
     reported: the clock, then Buffer Memory, then none
  3. the MIOP reads the tape on the Peripheral Expander, which has no write
     ring: a record, a short record of an odd number of bytes, a file mark,
     the end of the tape; it rewinds, spaces over a record and reads the one
     behind it, spaces back to the load point, is refused a write, reads
     part of a record and finds the rest passed over
  4. it writes two sectors to the expander's disk, reads them back to another
     place under another name for the same sectors, and finds them the same;
     the drive's interrupt request obeys the mask and the interrupt mode;
     and it prints a new page, six characters and a new line, then two
     parcels of dots in graphics mode and two characters in text mode again
  5. the MIOP starts the BIOP and the XIOP as the kernel does, telling each
     who it is through a parcel it changes in Buffer Memory
  6. the BIOP tries its first disk drive: the buffer echo, the Status
     Response register, reserving, seeking, a sector written and read back
     under another name; and it writes four words to central memory and
     reads them back
  7. every processor sends a word of its own to each of the others, and
     each word arrives where it should and is seen to be taken
  8. the MIOP reads the three keys A, B and RETURN from its console, which
     whoever runs the program types one behind the other, and clears the
     keyboard channel after each as the kernel does; none may be lost
  9. with --cpu, for a machine that has the mainframe: the BIOP puts a
     program into central memory and the MIOP lets the CPU go.  The program
     offers sixteen parcels on the CPU's output channel 11 and takes parcels
     on its input channel 10.  The MIOP takes eight, the first of which has
     waited for it, and sends four without a Disconnect, so that the CPU's
     channel goes on waiting; then it raises I/O Master Clear alone, after
     which nothing may pass either way; and it holds the CPU again

Each processor then writes its number and OK on its console (channel 47 on
the MIOP, 43 on the others), or F and a letter: C clock, P Q R priority,
a to m, o, S and Y the tape, n the Done flag of the expander, W a drive did
not finish,
B wrong address after the read, X what was read is not what was written,
E the printer did not finish,
I J K M N the interrupt request of the disk, p to z the BIOP's drive and
its channel into central memory, L no word came, D wrong word, T a word was
not taken, G no parcels from the mainframe, H not the parcels it sent, O it
took none, U V parcels passed after I/O Master Clear, 1 no key came, 2 not
the key that was typed.
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
R_WHO, R_AT, R_CONSOLE, R_COUNT, R_WORD, R_TABLE, R_EXPECT, R_SLOT, R_FROM, R_TO = 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
CLOCK, MOS, EXB = 4, 5, 0o17
HIA, HOA, DRIVE = 0o14, 0o15, 0o20      # channels of the BIOP
CIA, COA = 0o20, 0o21                   # the MIOP's channels to the mainframe
KEYBOARD = 0o46                         # of the MIOP's operator's console
KEYS = b'AB\r'                          # what is typed there
# The mainframe's program, words 0 to 43 octal of central memory: the exchange
# package of a CPU with the X-MP features (P = parcel 100, monitor mode, the
# largest fields), at word 20
#          A1  10       the input channel takes words 50 to 57
#          A2  50
#          A3  60
#          CL,A1 A3
#          CA,A1 A2
#          A1  11       the output channel sends words 40 to 43
#          A2  40
#          A3  44
#          CL,A1 A3
#          CA,A1 A2
#   HERE   J   HERE
# and at word 40 what it sends.
MAINFRAME = {0o00: 0x0000000040000000, 0o02: 0x0000FFFFE1000000, 0o05: 0x0000FFFFE0000000,
             0o20: 0x244824A824F0024B, 0o21: 0x020A244924A024E4, 0o22: 0x024B020A0C00004A,
             0o40: 0x14E5DC14E5DC14E5, 0o41: 0x924A4926DB72492D, 0o42: 0x71C71C71C71C71C7, 0o43: 0xFFFF00007FFF8000}
MAINFRAME_WORDS = 0o44
TAKEN = 0x5000                          # where the MIOP puts the mainframe's parcels
PRINTER, TAPE, DISK = 0o17, 0o22, 0o60          # addresses on the Peripheral Expander
PRINTED = b'\x0cPRINT!\n XX \nPR\n'   # what the printer prints: a page, a line, four times eight dots, two characters
DOTS = (0x00FF, 0x1200)                 # the dots: none, some, some, none
# the tape: a record of 300 bytes, one of 5, a file mark
RECORD = bytes((7 * n + 3) & 0xFF for n in range(300))
SHORT = bytes([1, 2, 3, 4, 5])
WRITTEN, READ, PARCELS = 0x2000, 0x3000, 512


def program(cpu):
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
    p.jump_if('A#0', 'others')

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
    # ---- 3 and 4. the tape and the disk of the Peripheral Expander
    def exb(function, value=None):
        """A function of the expander, and the wait for the channel a delayed one needs."""
        if value is not None:
            p.a(value)
        p.fn(EXB, function)
        busy = fresh('busy')
        p.label(busy)
        p.ins(0o041, EXB)
        p.jump_if('C=1', busy)

    def check(expected, failure):
        """The accumulator must hold this."""
        ok = fresh('is')
        if expected:
            p.ink(0o017, expected)
        p.jump_if('A=0', ok)
        p.a(ord(failure))
        p.jump('fail')
        p.label(ok)

    def register(n, expected, failure):
        """Register A (1), B (2) or C (3) of the selected device."""
        exb(n)
        exb(0o10)
        check(expected, failure)

    def parcel(address, expected, failure):
        p.a(address)
        p.ins(0o024, R_FROM)
        p.ins(0o030, R_FROM)
        check(expected, failure)

    def start():
        """Start the device and wait for it."""
        exb(0o17, 1)
        finished('W')

    def finished(failure):
        """Wait for the device as the kernel does: bit 15 of status 1."""
        loop, done = fresh('drive'), fresh('driven')
        p.a(0)
        p.ins(0o024, R_COUNT)
        p.label(loop)
        exb(4)
        exb(0o11)
        p.ins(0o005, 1)                  # the Done flag of the drive into the carry
        p.jump_if('C=1', done)
        p.ins(0o027, R_COUNT)
        p.jump_if('A#0', loop)
        p.a(ord(failure))
        p.jump('fail')
        p.label(done)

    def tape(command, address=0, count=0):
        exb(0o15, address)
        exb(0o16, (0x10000 - count) & 0xFFFF)
        exb(0o14, command << 3)
        start()
        exb(0o17, 2)                     # Clear

    def disk(command, address):
        exb(0o15, address)
        exb(0o16, command)
        start()

    def place(cylinder, head, sector):
        for value, name in ((cylinder, 5), (head, 1), (sector, 2), (PARCELS // 256, 3)):
            exb(0o15, value)
            exb(0o16, name)

    def asks(expected, failure):
        """The channel that asks for an interrupt."""
        p.fn(0, 0o10)
        check(expected, failure)

    exb(6, 0)
    # a delayed function leaves the channel Done, and function 0 clears that
    p.ins(0o040, EXB)
    p.jump_if('C=1', 'expander_done')
    p.a(ord('n'))
    p.jump('fail')
    p.label('expander_done')
    exb(0)
    p.ins(0o040, EXB)
    p.jump_if('C=0', 'expander_idle')
    p.a(ord('n'))
    p.jump('fail')
    p.label('expander_idle')

    exb(5, TAPE)
    register(1, 0x0084, 'a')             # at the load point, without a write ring
    exb(0o15, 0x4000)
    exb(0o16, 0x10000 - 200)
    exb(0o14, 0)
    start()                              # read: the record is shorter than the count
    exb(4)
    exb(0o11)
    p.ins(0o011, 0o77)                   # the device that asks
    check(TAPE, 'b')
    exb(0o17, 2)
    register(1, 0x0005, 'c')
    register(2, 0x4000 + 150, 'd')
    register(3, 0x10000 - 50, 'e')
    parcel(0x4000, RECORD[0] << 8 | RECORD[1], 'f')
    parcel(0x4000 + 149, RECORD[298] << 8 | RECORD[299], 'f')
    tape(0, 0x4100, 200)                 # five bytes: the last parcel is half filled
    register(2, 0x4103, 'g')
    parcel(0x4102, 0x0500, 'g')
    tape(0, 0x4400, 200)
    register(1, 0x8105, 'h')             # a file mark
    tape(0, 0x4400, 200)
    register(1, 0x8205, 'i')             # the end of the tape
    tape(1)
    register(1, 0x0085, 'j')             # rewound
    tape(3, 0, 1)                        # forward over the first record
    register(1, 0x0005, 'S')
    tape(0, 0x4500, 200)                 # the one behind it
    parcel(0x4502, 0x0500, 'S')
    tape(4, 0, 2)                        # back over both
    register(1, 0x0085, 'Y')
    register(3, 0, 'Y')                  # the count of records is used up
    tape(5, 0x4000, 4)                   # no write ring: illegal
    register(1, 0x9085, 'o')
    tape(0, 0x4200, 2)                   # two parcels of the record; the rest is passed over
    register(2, 0x4202, 'k')
    parcel(0x4202, 0, 'k')
    tape(0, 0x4300, 200)
    parcel(0x4300, 0x0102, 'm')

    exb(5, DISK)
    place(1, 2, 8)                       # cylinder 1, head 2, sector 8: 5 heads, 35 sectors
    # something different in every parcel: three times its address
    p.a(WRITTEN)
    p.ins(0o024, R_FROM)
    fill = fresh('fill')
    p.label(fill)
    p.ins(0o020, R_FROM)
    p.ins(0o005, 1)
    p.ins(0o022, R_FROM)
    p.ins(0o034, R_FROM)
    p.ins(0o026, R_FROM)
    p.ink(0o017, WRITTEN + PARCELS)
    p.jump_if('A#0', fill)
    disk(0o10, WRITTEN)
    exb(4)
    exb(0o11)
    p.ins(0o011, 0o77)                   # the device that asks
    check(DISK, 'M')
    # the drive asks for an interrupt: only with interrupts from the devices
    # on, and only when the mask lets it
    asks(0, 'I')
    exb(7, 2)
    asks(EXB, 'J')
    exb(6, 1 << 6)
    asks(0, 'K')
    exb(6, 0)
    exb(0o17, 2)                         # Clear
    asks(0, 'N')
    exb(7, 0)
    place(0, 6, 35 + 8)                  # the same two sectors, named another way
    disk(0, READ)
    exb(0o17, 2)
    register(2, READ + PARCELS, 'B')     # the address behind what was read
    p.a(WRITTEN)
    p.ins(0o024, R_FROM)
    p.a(READ)
    p.ins(0o024, R_TO)
    compare, equal = fresh('compare'), fresh('equal')
    p.label(compare)
    p.ins(0o030, R_FROM)
    p.ins(0o033, R_TO)
    p.jump_if('A=0', equal)
    p.a(ord('X'))
    p.jump('fail')
    p.label(equal)
    p.ins(0o026, R_TO)
    p.ins(0o026, R_FROM)
    p.ink(0o017, WRITTEN + PARCELS)
    p.jump_if('A#0', compare)

    # the printer: a new page, three parcels of this program, a new line
    p.jump('text_end')
    p.label('text')
    for at in range(1, 7, 2):
        p.word(PRINTED[at] << 8 | PRINTED[at + 1])
    p.label('dots')
    for parcel in DOTS:
        p.word(parcel)
    p.label('text_end')
    exb(5, PRINTER)
    exb(0o14, 0)
    exb(0o17, 4)                         # Pulse
    finished('E')
    exb(0o17, 2)
    exb(0o15, 0x10000 - 3)
    p.ink(0o014, 'text')
    exb(0o16)
    finished('E')
    exb(0o17, 2)
    exb(0o14, 3)
    exb(0o17, 4)
    finished('E')
    exb(0o17, 2)
    for mode, data, parcels in ((1, 'dots', len(DOTS)), (4, 'text', 1)):
        exb(0o14, mode)                  # graphics mode, then text mode again
        exb(0o17, 4)
        exb(0o15, 0x10000 - parcels)
        p.ink(0o014, data)
        exb(0o16)
        finished('E')
        exb(0o17, 2)
        exb(0o14, 3)
        exb(0o17, 4)
        finished('E')
        exb(0o17, 2)
    exb(0)

    # ---- 5. start the BIOP (output channel 7) and the XIOP (output channel 13)
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

    p.jump('links')

    def check(expected, failure):
        """The accumulator must hold this."""
        ok = fresh('is')
        if expected:
            p.ink(0o017, expected)
        p.jump_if('A=0', ok)
        p.a(ord(failure))
        p.jump('fail')
        p.label(ok)

    def fill(first, parcels):
        """Something different in every parcel: three times its address."""
        p.a(first)
        p.ins(0o024, R_FROM)
        loop = fresh('fill')
        p.label(loop)
        p.ins(0o020, R_FROM)
        p.ins(0o005, 1)
        p.ins(0o022, R_FROM)
        p.ins(0o034, R_FROM)
        p.ins(0o026, R_FROM)
        p.ink(0o017, first + parcels)
        p.jump_if('A#0', loop)

    def compare(one, other, parcels, failure):
        p.a(one)
        p.ins(0o024, R_FROM)
        p.a(other)
        p.ins(0o024, R_TO)
        loop, equal = fresh('compare'), fresh('equal')
        p.label(loop)
        p.ins(0o030, R_FROM)
        p.ins(0o033, R_TO)
        p.jump_if('A=0', equal)
        p.a(ord(failure))
        p.jump('fail')
        p.label(equal)
        p.ins(0o026, R_TO)
        p.ins(0o026, R_FROM)
        p.ink(0o017, one + parcels)
        p.jump_if('A#0', loop)

    def dk(function, value=None, drive=DRIVE):
        if value is not None:
            p.a(value)
        p.fn(drive, function)

    def reads(function, expected, failure):
        p.fn(DRIVE, function)
        check(expected, failure)

    def central(channel, high, local, function, failure):
        """Four words between Local Memory and central memory at high * 512 + 0x123."""
        p.fn(channel, 0)
        p.a(0x123)
        p.fn(channel, 3)
        p.a(high)
        p.fn(channel, 2)
        p.a(local)
        p.fn(channel, 1)
        p.a(4)
        p.fn(channel, function)
        wait_done(channel, failure)
        p.fn(channel, 0)

    # ---- 6. the BIOP: its first drive, and central memory
    p.label('others')
    p.ins(0o013, 1)
    p.jump_if('A#0', 'links')
    # the buffer echo: 512 parcels out and back, with no disk involved; the
    # second drive is given other parcels in between and keeps them to itself
    fill(WRITTEN, 2048)
    dk(0)
    dk(0o14, WRITTEN)
    dk(3, 0)
    wait_done(DRIVE, 'p')
    reads(0o10, WRITTEN + 512, 'p')
    dk(0, drive=DRIVE + 1)
    dk(0o14, WRITTEN + 1024, drive=DRIVE + 1)
    dk(3, 0, drive=DRIVE + 1)
    wait_done(DRIVE + 1, 'p')
    dk(0o14, READ)
    dk(2, 0)
    wait_done(DRIVE, 'p')
    reads(0o10, READ + 512, 'p')
    compare(WRITTEN, READ, 512, 'q')
    dk(0o15, 0xBEEF)                     # the Status Response register keeps what it is given
    reads(0o11, 0xBEEF, 'r')
    dk(0)
    dk(1, 0o7001)                        # the head register of a drive that is not reserved
    wait_done(DRIVE, 's')
    reads(0o11, 0, 's')
    dk(1, 0o1000)                        # reserve it
    wait_done(DRIVE, 's')
    dk(4, 3)                             # head group 3
    dk(1, 0o7001)
    wait_done(DRIVE, 's')
    reads(0o11, 0o143, 's')
    dk(5, 5)                             # cylinder 5
    wait_done(DRIVE, 't')
    reads(0o11, 5 << 5, 't')
    dk(0)
    p.ins(0o040, DRIVE)
    p.jump_if('C=0', 'drive_idle')
    p.a(ord('t'))
    p.jump('fail')
    p.label('drive_idle')
    dk(0o14, WRITTEN)                    # sector 7 of that track: 10 head groups, 18 sectors
    dk(3, 7)
    wait_done(DRIVE, 'u')
    reads(0o10, WRITTEN + 2048, 'u')
    dk(5, 4)                             # the same sector, named another way:
    wait_done(DRIVE, 'u')                # cylinder 4, head group 12, sector 25
    dk(0)
    dk(4, 12)
    dk(0o14, READ)
    dk(2, 18 + 7)
    wait_done(DRIVE, 'u')
    reads(0o10, READ + 2048, 'u')
    compare(WRITTEN, READ, 2048, 'v')
    dk(0)
    # four words to central memory, four others to an address that differs
    # in its upper part only, and the first four back to another place
    central(HOA, 2, WRITTEN, 5, 'w')
    central(HOA, 0, WRITTEN + 16, 5, 'w')
    central(HIA, 2, READ + 0x800, 4, 'x')
    compare(WRITTEN, READ + 0x800, 16, 'z')
    if cpu:
        # the mainframe's program, to the start of central memory
        p.fn(HOA, 0)
        p.a(0)
        p.fn(HOA, 3)
        p.a(0)
        p.fn(HOA, 2)
        p.ink(0o014, 'mainframe')
        p.fn(HOA, 1)
        p.a(MAINFRAME_WORDS)
        p.fn(HOA, 5)
        wait_done(HOA, 'w')
        p.fn(HOA, 0)
    # the output channel does not read, and the input channel does not write
    for channel, function in ((HOA, 4), (HIA, 5)):
        p.a(4)
        p.fn(channel, function)
        idle = fresh('idle')
        p.ins(0o041, channel)
        p.jump_if('C=0', idle)
        p.a(ord('y'))
        p.jump('fail')
        p.label(idle)

    # ---- 7. a word to each of the others: 120000 + 20 * number + pair (octal)
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

    # ---- 8. three keys (the MIOP).  They were typed long ago: the first waits
    # in the channel and the others come as soon as there is room.
    p.ins(0o020, R_WHO)
    p.jump_if('A#0', 'no_keys')
    for key in KEYS:
        wait_done(KEYBOARD, '1')
        p.fn(KEYBOARD, 0o10)
        check(key, '2')
        p.fn(KEYBOARD, 0)
    p.label('no_keys')

    # ---- 9. the mainframe, from the MIOP.  The BIOP's word has come, so its
    # program is in central memory.
    def pause():
        loop = fresh('pause')
        p.a(4000)
        p.label(loop)
        p.ins(0o013, 1)
        p.jump_if('A#0', loop)

    def transfer(channel, address, parcels):
        p.fn(channel, 0)
        p.a(parcels)
        p.fn(channel, 2)
        if isinstance(address, str):
            p.ink(0o014, address)
        else:
            p.a(address)
        p.fn(channel, 1)

    def quiet(channel, failure):
        still = fresh('still')
        p.ins(0o040, channel)
        p.jump_if('C=0', still)
        p.a(ord(failure))
        p.jump('fail')
        p.label(still)

    if cpu:
        p.ins(0o020, R_WHO)
        p.jump_if('A#0', 'no_mainframe')
        # both Master Clears, the CPU's alone, none: it starts.  COA is to hold its
        # Disconnect, or the CPU's input channel would stop after the first parcels.
        for lines in (0xC000, 0x8000, 0x0200):
            p.a(lines)
            p.fn(COA, 4)
        pause()                              # by now its first parcel waits at CIA
        transfer(CIA, TAKEN, 8)
        wait_done(CIA, 'G')
        sent = [MAINFRAME[0o40 + k // 4] >> (48 - 16 * (k % 4)) & 0xFFFF for k in range(16)]
        for k in range(8):
            p.a(TAKEN + k)
            p.ins(0o024, R_FROM)
            p.ins(0o030, R_FROM)
            if sent[k]:
                p.ink(0o017, sent[k])
            ok = fresh('sent')
            p.jump_if('A=0', ok)
            p.a(ord('H'))
            p.jump('fail')
            p.label(ok)
        transfer(COA, 'mainframe', 4)
        wait_done(COA, 'O')
        p.a(0x4200)                          # I/O Master Clear alone
        p.fn(COA, 4)
        p.a(0x0200)
        p.fn(COA, 4)
        transfer(COA, 'mainframe', 4)
        transfer(CIA, TAKEN + 16, 8)
        pause()
        quiet(COA, 'U')
        quiet(CIA, 'V')
        p.fn(COA, 0)
        p.fn(CIA, 0)
        p.a(0x8000)                          # the CPU is held again
        p.fn(COA, 4)
        p.label('no_mainframe')

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
    if cpu:
        # the 100 Mbyte channel takes whole words from an address that is a multiple of four
        while p.size() % 4:
            p.word(0)
        p.label('mainframe')
        for word in range(MAINFRAME_WORDS):
            for k in range(4):
                p.word(MAINFRAME.get(word, 0) >> (48 - 16 * k) & 0xFFFF)
    return p.assemble()


def main():
    cpu = '--cpu' in sys.argv
    names = [a for a in sys.argv[1:] if a != '--cpu']
    if len(names) != 1:
        sys.exit(__doc__)
    out = names[0]
    os.makedirs(os.path.join(out, 'target/cos_117'), exist_ok=True)
    with open(os.path.join(out, 'target/cos_117/iop_kern.bin'), 'wb') as f:
        for parcel in program(cpu):
            f.write(bytes([parcel >> 8, parcel & 0xFF]))
    # the tape, and a disk with nothing on it
    with open(os.path.join(out, 'boot_tape.tap'), 'wb') as f:
        for record in (RECORD, SHORT):
            f.write(len(record).to_bytes(4, 'little') + record + len(record).to_bytes(4, 'little'))
        f.write(bytes(4))
    open(os.path.join(out, 'exp_disk.img'), 'wb').close()
    for drive in (0o20, 0o21):
        open(os.path.join(out, 'biop_dk%o.img' % drive), 'wb').close()


main()

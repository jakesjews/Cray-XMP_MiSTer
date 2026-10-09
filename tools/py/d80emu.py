#!/usr/bin/env python3
"""Run the firmware of an Ampex Dialogue 80 terminal on a model of its hardware.

    d80emu.py [--rom FILE] SCRIPT

The firmware is the terminal's six program ROMs joined in address order
(12,288 bytes).  It is not part of this repository: name the file with --rom
or in the environment variable CRAY_XMP_D80_ROM.

This is the oracle that the terminal of the reference model
(tools/crates/ios/src/screen.rs) is checked against by d80test.py.  SCRIPT is
a file of events, the same that the model's screen_dump example reads, and the
output is the same canonical text, so the two can be compared line by line:

    config pages=2 duplex=full wrap=1 bell=0 hz=60 lock=0 fk=0 printer=0
    h 1b 3d 21 21 41       bytes from the host (hexadecimal)
    K 2 2 0 0              a key: matrix row, bit, SHIFT held, CTRL held
    k 61                   a key by the code it sends (hexadecimal)
    P a 0 1                PROG A (or b) held, and the key at row 0 bit 1
    d                      print the state

Every event is run to the end before the next: the firmware's main loop turns
until nothing changes any more.  That is the terminal as a host sees it that
never sends faster than the terminal works.  `printer=1` connects a printer
that is always ready; without it the printer never is.

The state is these lines; numbers of two and four digits are hexadecimal:

    page P shown S addressed A of N   the firmware's page, the page on the
                                      screen, the page it addresses, pages
    bottom B ...                      for each page, the memory row that is
                                      its last line
    cursor ROW COLUMN status S        memory row; S: shown in the status line
    mode M latch L shadow S port0 P readback R
                                      the modes; the attribute latch as it is
                                      and as the firmware keeps it; address
                                      5840 as written; cells read as attributes
    tabs T0 ... T9                    tab stops, a bit a column, column 0 in
                                      bit 0 of T0 (T9 is also the erase flag)
    lock L caps C                     keyboard locked by ESC #; caps lock
    hung H lost L                     what only the model can say: the
                                      firmware is in a loop for good; the
                                      model cannot follow it any more
    bells N                           bells rung since switching on
    sent BYTES                        sent to the host since the last print
    work ... send ... print ... keys ...
                                      the firmware's working variables: what
                                      the next event can depend on
    status CELLS                      the status line, 80 cells of three
                                      digits: attributes (1 reverse, 2 blank,
                                      4 flash, 8 underline), then protect bit
                                      and character
    pP rRR CELLS                      memory row RR of page P
    end

The hardware model (what a memory address does) is the one the firmware
itself implies; the firmware's power-on test of the video memory, of the
serial port and of its own checksum passes on it:

    0000-2FFF  program ROM              4000-43FF  RAM
    5400-540C  keyboard matrix rows     5C00-5C0F  CRT controller
    5840  write: pages shown and addressed, program mode, caps lock lamp
          read: switches (bit 7 full duplex, bits 6-0 inverted bell column)
    5841  write: control (bit 1 printer, bit 5 attribute read-back, bit 6
          bell, bit 7 loop-back)    read: serial port status
    5842  write: transmit            read: lines (bit 1 clear to send, bit 2
          keyboard lock, bit 3 wrap, bit 4 50 Hz, bit 5 four pages)
    5843  write: attribute latch     read: received byte
    8000-97FF  the addressed page, 8000h + row*100h + column
    C000-C04F  the status line

A screen cell is a 7-bit character, a protect bit and four attribute bits.  A
write stores the character if latch bit 2 is set, the protect bit (latch bit
7) if latch bit 1 is set, the attributes (latch bits 6-3) if latch bit 0 is
set.  A read returns protect and character, or the attributes in bits 6-3
while control bit 5 is set.

Only the instructions the firmware uses are implemented (the 8080's, without
decimal adjust, parity and input/output).  The vertical interrupt is not
given: it only moves the cursor of the CRT controller, times the bell and the
blinking, and repeats keys.  The receive interrupt is given at the top of the
main loop, and so is the transmit interrupt, as long as a byte waits to be
sent: the host line is always free.
"""
import os
import sys

ROM_BYTES = 0x3000
MAIN_LOOP = 0x0115          # the top of the firmware's main loop
AFTER_SCAN = 0x0118         # the main loop behind its keyboard scan
RX_VECTOR = 0x38
TX_VECTOR = 0x28
PORT_SET = 0x2833           # the routine every bell goes through, with C = 40h
SEND_WAIT, SEND_WAIT_END = 0x1421, 0x1433   # the loop that waits for room to send

# RAM variables of the firmware that the state print reads
V_KEYQ_OUT, V_KEYQ_IN = 0x4120, 0x4121
V_TXQ_OUT, V_TXQ_IN = 0x4122, 0x4123
V_DISPQ_OUT, V_DISPQ_IN = 0x4124, 0x4125
V_SHADOW, V_MODE, V_STATUS_REQ = 0x4126, 0x4129, 0x412B
V_ESC_CNT, V_ESC_LEN, V_ADVANCE = 0x412D, 0x412E, 0x4130
V_PAGE, V_PAGE_MAX, V_COL, V_ROW = 0x4135, 0x4136, 0x4137, 0x4138
V_LOCKED, V_PRINTING, V_TRANSPARENT, V_IN_STATUS = 0x4142, 0x4143, 0x4144, 0x4145
V_BOTTOM, V_PTR, V_CAPS = 0x4146, 0x414A, 0x4163
V_QFULL = 0x418B
V_SEND_ATTR, V_SEND_PROT = 0x4193, 0x4194
V_PRN_PROT, V_PRN_PGM, V_PRN_PAGE = 0x4196, 0x4197, 0x4198
V_SEND_PTR, V_SEND_END, V_PRN_PTR, V_PRN_END = 0x419B, 0x419D, 0x419F, 0x41A1
V_SENDING, V_SEND_ALL, V_PRN_ACK, V_PRN_DONE = 0x41A3, 0x41A4, 0x41A5, 0x41A6
V_STEP, V_PRNQ_IN, V_PRNQ_OUT, V_LIMIT = 0x41A8, 0x41A9, 0x41AA, 0x41B3
V_WRAPPED, V_TABS, V_FILL, V_SEEK = 0x41E6, 0x41E8, 0x41F2, 0x41F3
V_FK_RECORDING, V_FK_TO_HOST, V_FK_INDEX = 0x420C, 0x420D, 0x420E
V_FK_PTR, V_FK_LAST, V_FK_FREE = 0x420F, 0x4211, 0x4213
V_FK_TABLE, V_FK_END = 0x4295, 0x4400
KEY_TABLES = 0x113E         # plain, CTRL, SHIFT, caps lock: 104 codes each


def rom_path(argument=None):
    path = argument or os.environ.get('CRAY_XMP_D80_ROM', '')
    if not path:
        sys.exit('d80emu: name the firmware with --rom or in CRAY_XMP_D80_ROM')
    return path


def load_rom(path):
    with open(path, 'rb') as f:
        rom = f.read()
    if len(rom) != ROM_BYTES:
        sys.exit('d80emu: %s: %d bytes, not the %d of the six program ROMs' % (path, len(rom), ROM_BYTES))
    return rom


class Stuck(Exception):
    """The firmware did not come round its main loop in thirty million
    instructions (the longest it takes for anything else is about three)."""


class Terminal:
    def __init__(self, rom, pages=2, half_duplex=False, wrap=True, bell=0, hz50=False,
                 lock=False, printer=False):
        self.mem = bytearray(0x10000)
        self.mem[:len(rom)] = rom
        # video memory: per page the character with the protect bit, and the attributes
        self.cc = [bytearray(0x1800) for _ in range(4)]
        self.at = [bytearray(0x1800) for _ in range(4)]
        self.status_cc = bytearray(0x100)
        self.status_at = bytearray(0x100)
        self.port0 = 0
        self.port1 = 0
        self.latch = 0
        self.switch = (0 if half_duplex else 0x80) | (~bell & 0x7F)
        self.lines = 0x02 | (0x04 if lock else 0) | (0x08 if wrap else 0) | \
            (0x10 if hz50 else 0) | (0x20 if pages == 4 else 0)
        self.printer_ready = printer
        self.vtac = bytearray(16)
        self.keys = bytearray(13)
        self.rxdata = 0
        self.dav = 0
        self.sent = bytearray()         # to the host
        self.printed = bytearray()      # to the printer
        self.tx_done = 0                # a byte has left: the transmitter interrupts
        self.bells = 0
        self.writes = 0                 # changes of video memory and status line, bytes sent
        self.R = [0] * 8                # B C D E H L - A
        self.sp = 0
        self.pc = 0
        self.fz = self.fs = self.fc = 0
        self.iff = 0
        self.eip = 0                    # EI counts down two instructions
        self.steps = 0

    # ------------------------------------------------------------ memory
    def rd_io(self, a):
        if 0x8000 <= a < 0x9800:
            page = (self.port0 >> 4) & 3
            if self.port1 & 0x20:
                return self.at[page][a - 0x8000] << 3
            return self.cc[page][a - 0x8000]
        if a == 0x5841:
            return 0x06 | (0x08 if self.dav else 0) | (0 if self.printer_ready else 0x01)
        if a == 0x5843:
            self.dav = 0
            return self.rxdata
        if a == 0x5842:
            return self.lines
        if a == 0x5840:
            return self.switch
        if 0x5400 <= a <= 0x540C:
            return self.keys[a - 0x5400]
        if 0xC000 <= a < 0xC100:
            if self.port1 & 0x20:
                return self.status_at[a - 0xC000] << 3
            return self.status_cc[a - 0xC000]
        return 0xFF

    def wr_io(self, a, v):
        if 0x8000 <= a < 0x9800:
            page = (self.port0 >> 4) & 3
            self.put_cell(self.cc[page], self.at[page], a - 0x8000, v)
        elif a == 0x5843:
            self.latch = v
        elif a == 0x5841:
            self.port1 = v
        elif a == 0x5840:
            self.port0 = v
        elif a == 0x5842:
            self.writes += 1
            if self.port1 & 0x80:               # loop-back
                self.rxdata = v
                self.dav = 1
            elif self.port1 & 0x02:
                self.printed.append(v)
                self.tx_done = 1
            else:
                self.sent.append(v)
                self.tx_done = 1
        elif 0xC000 <= a < 0xC100:
            self.put_cell(self.status_cc, self.status_at, a - 0xC000, v)
        elif 0x5C00 <= a <= 0x5C0F:
            self.vtac[a & 15] = v

    def put_cell(self, cc, at, i, v):
        latch = self.latch
        before = (cc[i], at[i])
        if latch & 4:
            cc[i] = (cc[i] & 0x80) | (v & 0x7F)
        if latch & 2:
            cc[i] = (cc[i] & 0x7F) | (latch & 0x80)
        if latch & 1:
            at[i] = (latch >> 3) & 15
        if before != (cc[i], at[i]):
            self.writes += 1

    # ------------------------------------------------------------ processor
    def run(self, steps, stop=MAIN_LOOP):
        """Execute until the program counter is `stop` or `steps` instructions
        have run.  True when it stopped at `stop`."""
        mem = self.mem
        R = self.R
        rd = self.rd_io
        wr = self.wr_io
        pc = self.pc
        sp = self.sp
        fz = self.fz
        fs = self.fs
        fc = self.fc
        iff = self.iff
        eip = self.eip
        n = steps
        hit = False
        while n:
            n -= 1
            if eip:
                eip -= 1
                if not eip:
                    iff = 1
            op = mem[pc]
            pc += 1
            if op == 0x21:                      # LD HL,nn
                R[5] = mem[pc]
                R[4] = mem[pc + 1]
                pc += 2
            elif op == 0x36:                    # LD (HL),n
                a = (R[4] << 8) | R[5]
                if 0x4000 <= a < 0x4400:
                    mem[a] = mem[pc]
                else:
                    wr(a, mem[pc])
                pc += 1
            elif op == 0x3A:                    # LD A,(nn)
                a = mem[pc] | (mem[pc + 1] << 8)
                pc += 2
                R[7] = mem[a] if a < 0x4400 else rd(a)
            elif op == 0x32:                    # LD (nn),A
                a = mem[pc] | (mem[pc + 1] << 8)
                pc += 2
                if 0x4000 <= a < 0x4400:
                    mem[a] = R[7]
                else:
                    wr(a, R[7])
            elif op == 0xCD:                    # CALL nn
                a = mem[pc] | (mem[pc + 1] << 8)
                pc += 2
                if a == PORT_SET and R[1] == 0x40 and R[3] == 1:
                    self.bells += 1
                sp = (sp - 2) & 0xFFFF
                mem[sp] = pc & 255
                mem[sp + 1] = pc >> 8
                pc = a
            elif op == 0xC9:                    # RET
                pc = mem[sp] | (mem[sp + 1] << 8)
                sp = (sp + 2) & 0xFFFF
            elif op == 0xC3:                    # JP nn
                pc = mem[pc] | (mem[pc + 1] << 8)
            elif op == 0xFE:                    # CP n
                r = R[7] - mem[pc]
                pc += 1
                fc = 1 if r < 0 else 0
                r &= 255
                fz = 1 if r == 0 else 0
                fs = r >> 7
            elif 0x40 <= op < 0x80:             # LD r,r'
                z = op & 7
                y = (op >> 3) & 7
                if z == 6:
                    a = (R[4] << 8) | R[5]
                    v = mem[a] if a < 0x4400 else rd(a)
                else:
                    v = R[z]
                if y == 6:
                    if op == 0x76:
                        raise Stuck('HALT at %04X' % (pc - 1))
                    a = (R[4] << 8) | R[5]
                    if 0x4000 <= a < 0x4400:
                        mem[a] = v
                    else:
                        wr(a, v)
                else:
                    R[y] = v
            elif 0x80 <= op < 0xC0 or (op & 0xC7) == 0xC6:   # arithmetic and logic
                if op < 0xC0:
                    z = op & 7
                    if z == 6:
                        a = (R[4] << 8) | R[5]
                        v = mem[a] if a < 0x4400 else rd(a)
                    else:
                        v = R[z]
                else:
                    v = mem[pc]
                    pc += 1
                y = (op >> 3) & 7
                if y == 7:                      # CP
                    r = R[7] - v
                    fc = 1 if r < 0 else 0
                    r &= 255
                elif y == 0:                    # ADD
                    r = R[7] + v
                    fc = r >> 8
                    r &= 255
                    R[7] = r
                elif y == 2:                    # SUB
                    r = R[7] - v
                    fc = 1 if r < 0 else 0
                    r &= 255
                    R[7] = r
                elif y == 4:                    # AND
                    r = R[7] & v
                    fc = 0
                    R[7] = r
                elif y == 6:                    # OR
                    r = R[7] | v
                    fc = 0
                    R[7] = r
                elif y == 5:                    # XOR
                    r = R[7] ^ v
                    fc = 0
                    R[7] = r
                elif y == 3:                    # SBC
                    r = R[7] - v - fc
                    fc = 1 if r < 0 else 0
                    r &= 255
                    R[7] = r
                else:                           # ADC
                    r = R[7] + v + fc
                    fc = r >> 8
                    r &= 255
                    R[7] = r
                fz = 1 if r == 0 else 0
                fs = r >> 7
            elif op >= 0xC0:
                z = op & 7
                y = (op >> 3) & 7
                if z == 2 or z == 4 or z == 0:  # JP cc / CALL cc / RET cc
                    if y == 0:
                        c = not fz
                    elif y == 1:
                        c = fz
                    elif y == 2:
                        c = not fc
                    elif y == 3:
                        c = fc
                    elif y == 6:
                        c = not fs
                    elif y == 7:
                        c = fs
                    else:
                        raise Stuck('parity condition at %04X' % (pc - 1))
                    if z == 2:
                        if c:
                            pc = mem[pc] | (mem[pc + 1] << 8)
                        else:
                            pc += 2
                    elif z == 0:
                        if c:
                            pc = mem[sp] | (mem[sp + 1] << 8)
                            sp = (sp + 2) & 0xFFFF
                    else:
                        if c:
                            a = mem[pc] | (mem[pc + 1] << 8)
                            pc += 2
                            sp = (sp - 2) & 0xFFFF
                            mem[sp] = pc & 255
                            mem[sp + 1] = pc >> 8
                            pc = a
                        else:
                            pc += 2
                elif z == 5:                    # PUSH
                    p = y >> 1
                    sp = (sp - 2) & 0xFFFF
                    if p == 3:
                        mem[sp] = (fs << 7) | (fz << 6) | 2 | fc
                        mem[sp + 1] = R[7]
                    else:
                        mem[sp] = R[2 * p + 1]
                        mem[sp + 1] = R[2 * p]
                elif z == 1:
                    p = y >> 1
                    if not y & 1:               # POP
                        if p == 3:
                            v = mem[sp]
                            fs = v >> 7
                            fz = (v >> 6) & 1
                            fc = v & 1
                            R[7] = mem[sp + 1]
                        else:
                            R[2 * p + 1] = mem[sp]
                            R[2 * p] = mem[sp + 1]
                        sp = (sp + 2) & 0xFFFF
                    elif p == 2:                # JP (HL)
                        pc = (R[4] << 8) | R[5]
                    elif p == 3:                # LD SP,HL
                        sp = (R[4] << 8) | R[5]
                    else:
                        raise Stuck('instruction %02X at %04X' % (op, pc - 1))
                elif op == 0xEB:                # EX DE,HL
                    R[2], R[4] = R[4], R[2]
                    R[3], R[5] = R[5], R[3]
                elif op == 0xE3:                # EX (SP),HL
                    v = mem[sp]
                    mem[sp] = R[5]
                    R[5] = v
                    v = mem[sp + 1]
                    mem[sp + 1] = R[4]
                    R[4] = v
                elif op == 0xF3:                # DI
                    iff = 0
                    eip = 0
                elif op == 0xFB:                # EI
                    eip = 2
                elif z == 7:                    # RST
                    sp = (sp - 2) & 0xFFFF
                    mem[sp] = pc & 255
                    mem[sp + 1] = pc >> 8
                    pc = y * 8
                else:
                    raise Stuck('instruction %02X at %04X' % (op, pc - 1))
            else:                               # 00-3F
                z = op & 7
                y = (op >> 3) & 7
                if z == 1:
                    p = y >> 1
                    if not y & 1:               # LD rr,nn
                        if p == 3:
                            sp = mem[pc] | (mem[pc + 1] << 8)
                        else:
                            R[2 * p + 1] = mem[pc]
                            R[2 * p] = mem[pc + 1]
                        pc += 2
                    else:                       # ADD HL,rr
                        v = sp if p == 3 else (R[2 * p] << 8) | R[2 * p + 1]
                        r = ((R[4] << 8) | R[5]) + v
                        fc = r >> 16
                        R[4] = (r >> 8) & 255
                        R[5] = r & 255
                elif z == 6:                    # LD r,n
                    R[y] = mem[pc]
                    pc += 1
                elif z == 4 or z == 5:          # INC r / DEC r
                    if y == 6:
                        a = (R[4] << 8) | R[5]
                        v = mem[a] if a < 0x4400 else rd(a)
                    else:
                        v = R[y]
                    v = (v + 1 if z == 4 else v - 1) & 255
                    fz = 1 if v == 0 else 0
                    fs = v >> 7
                    if y == 6:
                        if 0x4000 <= a < 0x4400:
                            mem[a] = v
                        else:
                            wr(a, v)
                    else:
                        R[y] = v
                elif z == 3:                    # INC rr / DEC rr
                    p = y >> 1
                    if p == 3:
                        sp = (sp + (-1 if y & 1 else 1)) & 0xFFFF
                    else:
                        v = (((R[2 * p] << 8) | R[2 * p + 1]) + (-1 if y & 1 else 1)) & 0xFFFF
                        R[2 * p] = v >> 8
                        R[2 * p + 1] = v & 255
                elif z == 2:
                    if y == 4:                  # LD (nn),HL
                        a = mem[pc] | (mem[pc + 1] << 8)
                        pc += 2
                        if 0x4000 <= a < 0x43FF:
                            mem[a] = R[5]
                            mem[a + 1] = R[4]
                        else:
                            wr(a, R[5])
                            wr(a + 1, R[4])
                    elif y == 5:                # LD HL,(nn)
                        a = mem[pc] | (mem[pc + 1] << 8)
                        pc += 2
                        if a < 0x43FF:
                            R[5] = mem[a]
                            R[4] = mem[a + 1]
                        else:
                            R[5] = rd(a)
                            R[4] = rd(a + 1)
                    elif y < 4:                 # LD (BC),A  LD A,(BC)  LD (DE),A  LD A,(DE)
                        p = y >> 1
                        a = (R[2 * p] << 8) | R[2 * p + 1]
                        if y & 1:
                            R[7] = mem[a] if a < 0x4400 else rd(a)
                        elif 0x4000 <= a < 0x4400:
                            mem[a] = R[7]
                        else:
                            wr(a, R[7])
                    else:
                        raise Stuck('instruction %02X at %04X' % (op, pc - 1))
                elif z == 7:
                    v = R[7]
                    if y == 3:                  # RRA
                        R[7] = (v >> 1) | (fc << 7)
                        fc = v & 1
                    elif y == 5:                # CPL
                        R[7] = v ^ 255
                    elif y == 0:                # RLCA
                        fc = v >> 7
                        R[7] = ((v << 1) | fc) & 255
                    elif y == 2:                # RLA
                        R[7] = ((v << 1) | fc) & 255
                        fc = v >> 7
                    elif y == 1:                # RRCA
                        fc = v & 1
                        R[7] = (v >> 1) | (fc << 7)
                    elif y == 6:                # SCF
                        fc = 1
                    elif y == 7:                # CCF
                        fc ^= 1
                    else:
                        raise Stuck('DAA at %04X' % (pc - 1))
                elif op != 0:
                    raise Stuck('instruction %02X at %04X' % (op, pc - 1))
            if pc == stop:
                hit = True
                break
        self.steps += steps - n
        self.pc = pc
        self.sp = sp
        self.fz = fz
        self.fs = fs
        self.fc = fc
        self.iff = iff
        self.eip = eip
        return hit

    def interrupt(self, vector):
        mem = self.mem
        self.iff = 0
        self.eip = 0
        self.sp = (self.sp - 2) & 0xFFFF
        mem[self.sp] = self.pc & 255
        mem[self.sp + 1] = self.pc >> 8
        self.pc = vector

    # ------------------------------------------------------------ driving
    def transmit(self):
        """Transmit interrupts when a byte has left and while bytes for the
        host wait: the transmitter asks for the next byte.  They are given at the top of the main loop
        only (the firmware moves the pointer of its transmit queue before it
        stores the byte, and an interrupt between the two sends what was in
        the place before; the real transmitter can hit that window too, but
        only by chance)."""
        mem = self.mem
        for _ in range(64):
            if not self.iff or not (self.tx_done or mem[V_TXQ_IN] != mem[V_TXQ_OUT]):
                return
            self.tx_done = 0
            self.interrupt(TX_VECTOR)
            if not self.run(100000):
                raise Stuck('the transmit interrupt does not return')

    def turn(self, scan=None):
        """One turn of the main loop, from its top to its top.  Without keys
        down and with nothing left in the scan's memory of the keyboard the
        scan would change nothing, and is left out."""
        mem = self.mem
        if self.pc != MAIN_LOOP:
            raise Stuck('not at the top of the main loop but at %04X' % self.pc)
        self.transmit()
        if scan is None:
            scan = any(self.keys) or any(mem[0x4168:0x4175]) or any(mem[0x4178:0x4185])
        if not scan:
            self.pc = AFTER_SCAN
        for _ in range(600000):
            try:
                if self.run(50):
                    return
            except IndexError:
                raise Stuck('the firmware has left its program, at %04X' % (self.pc & 0xFFFF))
            # the firmware waits in a loop for room in its transmit queue
            # (1421-1432): there the transmitter must take a byte
            if SEND_WAIT <= self.pc < SEND_WAIT_END and self.iff and mem[V_QFULL] & 1:
                self.interrupt(TX_VECTOR)
        raise Stuck('the main loop does not come round, at %04X' % self.pc)

    def watch(self):
        mem = self.mem
        # the queues, modes, cursor, pages, the send and print and function key
        # work; not what the transmit service flips at every call while the
        # printer does not answer (412A, 4128, 41A7)
        return (bytes(mem[0x4120:0x4128]), mem[V_MODE], mem[V_STATUS_REQ], mem[V_ESC_CNT],
                bytes(mem[0x4135:0x4139]), bytes(mem[0x4142:0x414A]), bytes(mem[0x4193:0x41A7]),
                bytes(mem[0x41A8:0x41AB]), bytes(mem[0x420C:0x4211]), mem[0x4153], self.writes,
                self.port0)

    def settle(self):
        """Turn the main loop until a turn changes nothing."""
        for _ in range(100000):
            before = self.watch()
            self.turn()
            if self.watch() == before:
                return
        raise Stuck('the firmware does not come to rest')

    def boot(self):
        self.pc = 0
        if not self.run(3000000):
            raise Stuck('the firmware does not reach its main loop')
        self.settle()
        self.bells = 0
        self.sent = bytearray()
        self.printed = bytearray()
        self.tx_done = 0

    def host(self, byte):
        """One byte from the host: the receive interrupt, then to rest."""
        if not self.iff:
            raise Stuck('interrupts are off at the top of the main loop')
        self.rxdata = byte
        self.dav = 1
        self.interrupt(RX_VECTOR)
        if not self.run(100000):
            raise Stuck('the receive interrupt does not return')
        self.settle()

    def key_code(self, row, bit, shift, ctrl):
        """The code the firmware's tables give this key now."""
        table = 1 if ctrl else 2 if shift else 3 if self.mem[V_CAPS] & 1 else 0
        return self.mem[KEY_TABLES + 104 * table + 8 * row + bit]

    def find_key(self, code):
        for ctrl in (0, 1):
            for shift in (0, 1):
                if ctrl and shift:
                    continue
                for row in range(13):
                    for bit in range(8):
                        if (row, bit) in ((2, 1), (3, 1), (4, 2), (4, 5), (4, 7)):
                            continue
                        if self.key_code(row, bit, shift, ctrl) == code:
                            return row, bit, shift, ctrl
        raise ValueError('no key sends %02X' % code)

    def key(self, row, bit, shift=False, ctrl=False, prog=None):
        """A key pressed and let go, with SHIFT, CTRL or a PROG key held."""
        held = []
        if shift:
            held.append((3, 1))
        if ctrl:
            held.append((2, 1))
        if prog:
            held.append((4, 2) if prog == 'a' else (4, 7))
        for r, b in held:
            self.keys[r] |= 1 << b
        if held:
            self.turn(True)
            self.turn(True)
        self.keys[row] |= 1 << bit
        self.turn(True)
        self.turn(True)
        self.settle()
        self.keys[row] &= ~(1 << bit)
        self.turn(True)
        self.turn(True)
        for r, b in held:
            self.keys[r] &= ~(1 << b)
        if held:
            self.turn(True)
            self.turn(True)
        self.settle()

    # ------------------------------------------------------------ copies
    def snapshot(self):
        return (bytes(self.mem[0x4000:0x4400]), [bytes(p) for p in self.cc], [bytes(p) for p in self.at],
                bytes(self.status_cc), bytes(self.status_at), self.port0, self.port1, self.latch,
                bytes(self.vtac), bytes(self.keys), self.rxdata, self.dav, bytes(self.sent),
                bytes(self.printed), self.bells, self.writes, list(self.R), self.sp, self.pc,
                self.fz, self.fs, self.fc, self.iff, self.eip, self.tx_done)

    def restore(self, s):
        self.mem[0x4000:0x4400] = s[0]
        self.cc = [bytearray(p) for p in s[1]]
        self.at = [bytearray(p) for p in s[2]]
        self.status_cc = bytearray(s[3])
        self.status_at = bytearray(s[4])
        self.port0, self.port1, self.latch = s[5:8]
        self.vtac = bytearray(s[8])
        self.keys = bytearray(s[9])
        self.rxdata, self.dav = s[10:12]
        self.sent = bytearray(s[12])
        self.printed = bytearray(s[13])
        self.bells, self.writes = s[14:16]
        self.R = list(s[16])
        (self.sp, self.pc, self.fz, self.fs, self.fc, self.iff, self.eip, self.tx_done) = s[17:25]

    # ------------------------------------------------------------ the state
    def word(self, a):
        return self.mem[a] | (self.mem[a + 1] << 8)

    def dump(self):
        """The state as lines of text, the same that screen_dump prints for
        the model.  The bytes sent to the host since the last print are part
        of it and are taken away."""
        mem = self.mem
        pages = mem[V_PAGE_MAX] + 1
        out = []
        out.append('page %d shown %d addressed %d of %d' % (
            mem[V_PAGE], self.port0 & 3, (self.port0 >> 4) & 3, pages))
        out.append('bottom ' + ' '.join('%d' % mem[V_BOTTOM + p] for p in range(pages)))
        out.append('cursor %d %d status %d' % (mem[V_ROW], mem[V_COL], mem[V_IN_STATUS] & 1))
        out.append('mode %02X latch %02X shadow %02X port0 %02X readback %d' % (
            mem[V_MODE], self.latch, mem[V_SHADOW], self.port0, (self.port1 >> 5) & 1))
        out.append('tabs ' + ' '.join('%02X' % mem[V_TABS + i] for i in range(10)))
        out.append('lock %d caps %d' % (mem[V_LOCKED] & 1, mem[V_CAPS] & 1))
        out.append('hung 0 lost 0')
        out.append('bells %d' % self.bells)
        out.append('sent ' + self.sent.hex())
        self.sent = bytearray()
        out.append('work step %02X limit %04X ptr %04X seek %02X wrapped %02X advance %02X fill %02X '
                   'esc %02X %02X request %02X full %02X queue %d keys %d' % (
                       mem[V_STEP], self.word(V_LIMIT), self.word(V_PTR), mem[V_SEEK], mem[V_WRAPPED],
                       mem[V_ADVANCE], mem[V_FILL], mem[V_ESC_CNT], mem[V_ESC_LEN], mem[V_STATUS_REQ],
                       mem[V_QFULL], (mem[V_DISPQ_IN] - mem[V_DISPQ_OUT]) & 255,
                       (mem[V_KEYQ_IN] - mem[V_KEYQ_OUT]) & 15))
        out.append('send %02X ptr %04X end %04X attr %02X prot %02X all %02X' % (
            mem[V_SENDING], self.word(V_SEND_PTR), self.word(V_SEND_END), mem[V_SEND_ATTR],
            mem[V_SEND_PROT], mem[V_SEND_ALL]))
        out.append('print %02X transparent %02X done %02X ack %02X queued %d ptr %04X end %04X '
                   'page %d prot %02X pgm %02X' % (
                       mem[V_PRINTING], mem[V_TRANSPARENT], mem[V_PRN_DONE], mem[V_PRN_ACK],
                       (mem[V_PRNQ_IN] - mem[V_PRNQ_OUT]) & 7, self.word(V_PRN_PTR),
                       self.word(V_PRN_END), mem[V_PRN_PAGE], mem[V_PRN_PROT], mem[V_PRN_PGM]))
        out.append('keys recording %02X host %02X index %02X ptr %04X free %04X memory %s' % (
            mem[V_FK_RECORDING], mem[V_FK_TO_HOST], mem[V_FK_INDEX], self.word(V_FK_PTR),
            self.word(V_FK_FREE), bytes(mem[V_FK_TABLE:V_FK_END]).hex()))
        out.append('status ' + ''.join('%X%02X' % (self.status_at[i], self.status_cc[i]) for i in range(80)))
        for p in range(pages):
            cc, at = self.cc[p], self.at[p]
            for row in range(24):
                base = row * 256
                out.append('p%d r%02d %s' % (p, row, ''.join(
                    '%X%02X' % (at[base + i], cc[base + i]) for i in range(80))))
        out.append('end')
        return out


def parse_config(words):
    opts = {'pages': 2, 'half_duplex': False, 'wrap': True, 'bell': 0, 'hz50': False,
            'lock': False, 'printer': False, 'fk': False}
    for w in words:
        name, _, value = w.partition('=')
        if name == 'pages':
            opts['pages'] = int(value)
        elif name == 'duplex':
            opts['half_duplex'] = value == 'half'
        elif name == 'wrap':
            opts['wrap'] = value == '1'
        elif name == 'bell':
            opts['bell'] = int(value)
        elif name == 'hz':
            opts['hz50'] = value == '50'
        elif name == 'lock':
            opts['lock'] = value == '1'
        elif name == 'printer':
            opts['printer'] = value == '1'
        elif name == 'fk':
            opts['fk'] = value == '1'
        else:
            raise ValueError('config: %s' % w)
    return opts


def power_on(rom, opts):
    """A terminal that has been switched on and has come to rest.  (`fk` only
    tells the model whether to have function keys; the firmware always has.)"""
    t = Terminal(rom, pages=opts['pages'], half_duplex=opts['half_duplex'], wrap=opts['wrap'],
                 bell=opts['bell'], hz50=opts['hz50'], lock=opts['lock'], printer=opts['printer'])
    t.boot()
    return t


def event(t, words):
    """One line of a script, but config and d."""
    if words[0] == 'h':
        for w in words[1:]:
            t.host(int(w, 16))
    elif words[0] == 'K':
        t.key(int(words[1]), int(words[2]), words[3] == '1', words[4] == '1')
    elif words[0] == 'k':
        t.key(*t.find_key(int(words[1], 16)))
    elif words[0] == 'P':
        t.key(int(words[2]), int(words[3]), prog=words[1])
    else:
        raise ValueError('event: %s' % ' '.join(words))


def main():
    args = sys.argv[1:]
    rom = None
    if args[:1] == ['--rom']:
        rom = args[1]
        args = args[2:]
    if len(args) != 1:
        sys.exit(__doc__)
    rom = load_rom(rom_path(rom))
    t = None
    for number, line in enumerate(open(args[0])):
        words = line.split('#')[0].split()
        if not words:
            continue
        if words[0] == 'config':
            t = power_on(rom, parse_config(words[1:]))
            continue
        if t is None:
            t = power_on(rom, parse_config([]))
        if words[0] == 'd':
            print('\n'.join(t.dump()))
            continue
        try:
            event(t, words)
        except Stuck as e:
            # nothing can be printed of a terminal that never comes to rest
            sys.exit('d80emu: the firmware hangs in line %d: %s' % (number + 1, e))


if __name__ == '__main__':
    main()

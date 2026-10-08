#!/usr/bin/env python3
"""Runs on the MiSTer (Python 3.9, standard library only).

Gives the test scripts what the CRAY X-MP core exposes to Linux:
  - the machine's memories, which are in DDR3 from physical 0x30000000 and
    visible through /dev/mem: central memory from word 0, the Buffer Memory
    of the I/O Subsystem from word 0o20000000, the boot file from word
    0o40000000.  Word n is 8 bytes at offset 8*n, most significant first.
  - the two consoles on the HPS serial port /dev/ttyS1 (115200 8N1): bit 7 of
    a byte is clear for the operator's console and set for the station.  A
    BREAK resets the machine, as the menu's reset does.
  - a keyboard: keys typed here reach the core as a USB keyboard's would.

Commands:
  peek WORD [COUNT]            print words in hex (WORD and COUNT accept 0o / 0x)
  poke WORD HEX [HEX ...]      write words
  fill WORD COUNT HEX          write one value to a range
  dump FILE WORD COUNT         save a range of memory to a file
  uart SECONDS [--break] [--send TEXT] [--until TEXT]
                               copy what the serial port carries to stdout
  keys TEXT [--gap SEC]        type on the keyboard
  session SECONDS [--break] [--type WAIT=KEYS]... [--until TEXT] [--screen C]...
                               work the two consoles
"""
import fcntl
import mmap
import os
import select
import struct
import sys
import termios
import time

BASE = 0x30000000
SPAN = 80 << 20           # central memory, Buffer Memory and the boot file
TTY = '/dev/ttyS1'
KEPT = '/tmp/xmp_console.raw'     # what the serial port has carried since the machine was started


def mem():
    fd = os.open('/dev/mem', os.O_RDWR | os.O_SYNC)
    return mmap.mmap(fd, SPAN, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE, offset=BASE)


def num(s):
    return int(s, 0)


def cmd_peek(args):
    word = num(args[0])
    count = num(args[1]) if len(args) > 1 else 1
    m = mem()
    for i in range(count):
        print('%07o: %s' % (word + i, m[(word + i) * 8:(word + i) * 8 + 8].hex()))


def cmd_poke(args):
    word = num(args[0])
    m = mem()
    for i, h in enumerate(args[1:]):
        m[(word + i) * 8:(word + i) * 8 + 8] = int(h, 16).to_bytes(8, 'big')


def cmd_fill(args):
    word, count, val = num(args[0]), num(args[1]), int(args[2], 16).to_bytes(8, 'big')
    m = mem()
    m[word * 8:(word + count) * 8] = val * count


def cmd_dump(args):
    word, count = num(args[1]), num(args[2])
    m = mem()
    open(args[0], 'wb').write(m[word * 8:(word + count) * 8])


def open_tty():
    fd = os.open(TTY, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    a = termios.tcgetattr(fd)
    a[0] = 0                                             # iflag: raw
    a[1] = 0                                             # oflag
    a[2] = termios.CS8 | termios.CREAD | termios.CLOCAL  # cflag
    a[3] = 0                                             # lflag
    a[4] = a[5] = termios.B115200
    a[6][termios.VMIN] = 0
    a[6][termios.VTIME] = 0
    termios.tcsetattr(fd, termios.TCSANOW, a)
    termios.tcflush(fd, termios.TCIOFLUSH)
    return fd


def cmd_uart(args):
    seconds = float(args[0])
    do_break = '--break' in args
    send = args[args.index('--send') + 1] if '--send' in args else None
    until = args[args.index('--until') + 1].encode() if '--until' in args else None
    fd = open_tty()
    if do_break:
        termios.tcsendbreak(fd, 0)
    got = b''
    end = time.time() + seconds
    sent = send is None
    out = sys.stdout.buffer
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            chunk = os.read(fd, 4096)
            got += chunk
            out.write(chunk)
            out.flush()
            if until and until in got:
                break
        elif not sent and time.time() > end - seconds + 0.5:
            os.write(fd, send.encode().decode('unicode_escape').encode('latin1'))
            sent = True
    os.close(fd)


TIOCSBRK = 0x5427
TIOCCBRK = 0x5428
UI_SET_EVBIT, UI_SET_KEYBIT, UI_DEV_CREATE, UI_DEV_DESTROY = 0x40045564, 0x40045565, 0x5501, 0x5502
EV_SYN, EV_KEY = 0, 1
KEY_CTRL, KEY_SHIFT = 29, 42
NAMED = {'enter': 28, 'esc': 1, 'tab': 15, 'backspace': 14, 'space': 57, 'up': 103, 'down': 108, 'left': 105,
         'right': 106, 'f1': 59, 'f2': 60, 'f3': 61, 'f12': 88, 'delete': 111}
PLAIN = dict(zip('1234567890-=', range(2, 14)))
PLAIN.update(zip('qwertyuiop[]', range(16, 28)))
PLAIN.update(zip('asdfghjkl;\'`', range(30, 42)))
PLAIN.update(zip('\\zxcvbnm,./', range(43, 54)))
PLAIN.update({' ': 57, '\r': 28, '\n': 28, '\t': 15, '\x08': 14})
SHIFTED = dict(zip('!@#$%^&*()_+', range(2, 14)))
SHIFTED.update(zip('{}', (26, 27)))
SHIFTED.update(zip(':"~', (39, 40, 41)))
SHIFTED.update(zip('|', (43,)))
SHIFTED.update(zip('<>?', (51, 52, 53)))


def key_steps(text):
    """(key code, shift, ctrl) for each key press named by `text`: characters,
    \\r \\n \\t \\xHH (control characters are typed with CTRL) and {name} for a
    special key such as {f12}, {enter}, {esc}, {up}, {down}."""
    text = text.encode().decode('unicode_escape')
    out, i = [], 0
    while i < len(text):
        c = text[i]
        if c == '{' and '}' in text[i:] and text[i + 1:text.index('}', i)].lower() in NAMED:
            j = text.index('}', i)
            out.append((NAMED[text[i + 1:j].lower()], False, False))
            i = j + 1
            continue
        i += 1
        if c in PLAIN: out.append((PLAIN[c], False, False))
        elif c in SHIFTED: out.append((SHIFTED[c], True, False))
        elif c.lower() in PLAIN and c.isupper(): out.append((PLAIN[c.lower()], True, False))
        elif 1 <= ord(c) <= 26: out.append((PLAIN[chr(ord(c) + 96)], False, True))
        else: sys.exit('no key for character %r' % c)
    return out


def cmd_keys(args):
    """keys TEXT [--gap SEC]: type TEXT on a virtual keyboard."""
    gap = float(args[args.index('--gap') + 1]) if '--gap' in args else 0.06
    steps = key_steps(args[0])
    fd = os.open('/dev/uinput', os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    for code in range(1, 120):
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)
    # struct uinput_user_dev: name, bus/vendor/product/version, ff effects, 4 x 64 axis limits
    os.write(fd, struct.pack('80sHHHHi', b'Cray-XMP test keyboard', 3, 0x1209, 0xC1A1, 1, 0) + bytes(4 * 64 * 4))
    fcntl.ioctl(fd, UI_DEV_CREATE)
    time.sleep(5.0)                           # Main_MiSTer needs a while to open the new device

    def emit(code, value):
        os.write(fd, struct.pack('llHHi', 0, 0, EV_KEY, code, value) + struct.pack('llHHi', 0, 0, EV_SYN, 0, 0))
        time.sleep(gap / 2)

    for code, shift, ctrl in steps:
        if ctrl: emit(KEY_CTRL, 1)
        if shift: emit(KEY_SHIFT, 1)
        emit(code, 1)
        emit(code, 0)
        if shift: emit(KEY_SHIFT, 0)
        if ctrl: emit(KEY_CTRL, 0)
        time.sleep(gap)
    time.sleep(0.3)
    fcntl.ioctl(fd, UI_DEV_DESTROY)
    os.close(fd)


def squeeze(raw):
    """What a console was sent, without blanks, control characters and the
    escape sequences of an Ampex Dialogue 80: for finding a text whatever moved
    the cursor between its words."""
    out, n = [], 0
    while n < len(raw):
        c = raw[n]
        if c == 0x1B:
            n += 4 if raw[n + 1:n + 2] == b'=' else 3 if raw[n + 1:n + 2] == b'G' else 2
            continue
        if 0x20 < c < 0x7F:
            out.append(c)
        n += 1
    return bytes(out)


class Ampex:
    """The screen of an Ampex Dialogue 80, a character at a time: 24 lines of
    80 characters (the sequences are those of rtl/terminal/term_ampex.v)."""

    def __init__(self):
        self.lines = [bytearray(b' ' * 80) for _ in range(24)]
        self.line = self.column = 0
        self.escape = 0        # 1 after ESC, 2 the row follows, 3 the column, 4 one character to drop
        self.row = 0

    def feed(self):
        if self.line < 23:
            self.line += 1
        else:
            self.lines.pop(0)
            self.lines.append(bytearray(b' ' * 80))

    def put(self, c):
        c &= 0x7F
        if self.escape == 1:
            self.escape = 0
            if c == 0x3D:
                self.escape = 2
            elif c == 0x47:
                self.escape = 4
            elif c == 0x2A:
                self.lines = [bytearray(b' ' * 80) for _ in range(24)]
                self.line = self.column = 0
            elif c == 0x54:
                self.lines[self.line][self.column:] = b' ' * (80 - self.column)
            elif c == 0x52:
                self.lines.pop(self.line)
                self.lines.append(bytearray(b' ' * 80))
        elif self.escape == 2:
            self.row, self.escape = c, 3
        elif self.escape == 3:
            self.line = min(max(self.row - 0x20, 0), 23)
            self.column = min(max(c - 0x20, 0), 79)
            self.escape = 0
        elif self.escape == 4:
            self.escape = 0
        elif c == 0x1B:
            self.escape = 1
        elif c == 0x08:
            self.column = max(self.column - 1, 0)
        elif c == 0x0A:
            self.feed()
        elif c == 0x0C:
            self.column = min(self.column + 1, 79)
        elif c == 0x0D:
            self.column = 0
        elif 0x20 <= c < 0x7F:
            self.lines[self.line][self.column] = c
            if self.column < 79:
                self.column += 1
            else:
                self.column = 0
                self.feed()

    def text(self):
        text = [l.decode().rstrip() for l in self.lines]
        while text and not text[-1]:
            text.pop()
        return '\n'.join(text)


def render(raw):
    """The 24 lines a console shows after what it was sent."""
    screen = Ampex()
    for c in raw:
        screen.put(c)
    return screen.text()


def cmd_session(args):
    """session SECONDS [--break] [--type WAIT=KEYS]... [--until TEXT] [--gap SEC]
               [--screen C]... [--raw FILE]

    Work the two consoles of the CRAY X-MP core through the serial port, where
    bit 7 of a byte names the console: clear is the operator's console, set
    the station.  --type sends KEYS (\\r is RETURN) once a console has shown
    WAIT; several are taken in order.  WAIT is found whatever blanks and cursor
    movements lie between its characters, or when the screen shows it and did
    not at the last KEYS (the station sends only what differs from what is
    there); +N waits N milliseconds instead.  WAIT
    is looked for on the operator's console, or on the station if it begins
    with @0:, and KEYS go to the same console.  --until ends the session when
    its text has been shown after the last KEYS; --break first holds a serial
    BREAK, which starts the machine again.  What the operator's console prints
    is copied to the output as it comes; --screen prints a console's 24 lines
    at the end (0 the station, 3 the operator's).  Exit status 1 if the text of
    --until did not come.

    The station sends only the characters of a display that differ from what
    is on the screen, so a session has to know what the screens showed before
    it: everything the port carries is kept in /tmp/xmp_console.raw from one
    session to the next.  --fresh (and --break) starts from empty screens."""
    seconds = float(args[0])
    typing, screens, until, gap, raw_file, do_break, fresh = [], [], None, 0.004, None, False, False

    def on_console(text):
        if text[:1] == '@' and text[2:3] == ':':
            return (1 if text[1] == '0' else 0), text[3:]
        return 0, text

    i = 1
    while i < len(args):
        a = args[i]
        i += 1
        if a == '--break':
            do_break = True
        elif a == '--fresh':
            fresh = True
        elif a == '--type':
            wait, keys = args[i].split('=', 1)
            c, wait = on_console(wait)
            delay = float(wait[1:]) / 1000 if wait[:1] == '+' and wait[1:].isdigit() else None
            typing.append((c, squeeze(wait.encode()), keys.replace('\\r', '\r').encode(), delay))
            i += 1
        elif a == '--until':
            until = on_console(args[i])
            until = (until[0], squeeze(until[1].encode()))
            i += 1
        elif a == '--gap':
            gap = float(args[i])
            i += 1
        elif a == '--screen':
            screens.append(1 if args[i] == '0' else 0)
            i += 1
        elif a == '--raw':
            raw_file = args[i]
            i += 1
        else:
            sys.exit('session: unknown argument %s' % a)

    fd = open_tty()
    if do_break:
        fcntl.ioctl(fd, TIOCSBRK)
        time.sleep(0.1)
        termios.tcflush(fd, termios.TCIOFLUSH)
        fcntl.ioctl(fd, TIOCCBRK)
    console = [bytearray(), bytearray()]
    screen = [Ampex(), Ampex()]
    # what the screens showed when the session before this one ended
    if do_break or fresh:
        open(KEPT, 'wb').close()
    elif os.path.exists(KEPT):
        for b in open(KEPT, 'rb').read():
            screen[b >> 7].put(b)
    # the screens at the last keys, without blanks: what is on them now is not news
    before = [squeeze(screen[n].text().encode()) for n in range(2)]
    raw = bytearray()
    typed_from = [0, 0]

    def has(c, want):
        if want in squeeze(bytes(console[c][typed_from[c]:])):
            return True
        return want not in before[c] and want in squeeze(screen[c].text().encode())
    said, shown = 0, False
    out = sys.stdout
    start = time.time()
    waiting_since = start
    while time.time() < start + seconds:
        r, _, _ = select.select([fd], [], [], 0.02)
        if r:
            chunk = os.read(fd, 4096)
            raw += chunk
            for b in chunk:
                console[b >> 7].append(b & 0x7F)
                screen[b >> 7].put(b)
                if not b >> 7 and (b == 0x0A or 0x20 <= b < 0x7F):
                    out.write(chr(b))
            out.flush()
        if said < len(typing):
            c, wait, keys, delay = typing[said]
            there = time.time() - waiting_since >= delay if delay is not None else has(c, wait)
            if there:
                # the kernel drops a key that comes before it has finished its question
                time.sleep(0.05)
                for k in keys:
                    os.write(fd, bytes([k | (0x80 if c else 0)]))
                    time.sleep(gap)
                said += 1
                typed_from = [len(console[0]), len(console[1])]
                before = [squeeze(screen[n].text().encode()) for n in range(2)]
                waiting_since = time.time()
        elif until and has(until[0], until[1]):
            shown = True
            break
    # what is still on its way
    end = time.time() + 0.3
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            chunk = os.read(fd, 4096)
            raw += chunk
            for b in chunk:
                console[b >> 7].append(b & 0x7F)
                screen[b >> 7].put(b)
    os.close(fd)
    with open(KEPT, 'ab') as kept:
        kept.write(raw)
    if raw_file:
        open(raw_file, 'wb').write(raw)
    for c in screens:
        print('\n---- %s\n%s\n----' % ('station' if c else "operator's console", screen[c].text()))
    print('\n%.1f s; %d characters on the operator\'s console, %d on the station' % (time.time() - start, len(console[0]), len(console[1])))
    if until:
        print('the text was shown' if shown else 'the text was NOT shown')
        sys.exit(0 if shown else 1)


COMMANDS = {'keys': cmd_keys, 'peek': cmd_peek, 'poke': cmd_poke, 'fill': cmd_fill, 'dump': cmd_dump, 'uart': cmd_uart,
            'session': cmd_session}

if __name__ == '__main__':
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        sys.exit(__doc__)
    COMMANDS[sys.argv[1]](sys.argv[2:])

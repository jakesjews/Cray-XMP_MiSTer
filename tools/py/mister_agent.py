#!/usr/bin/env python3
"""Runs on the MiSTer (Python 3.9, standard library only).

Gives the test scripts two things the core exposes to Linux:
  - Cray main memory, which lives in DDR3 at physical 0x30000000 and is visible
    through /dev/mem.  Word n is 8 bytes at offset 8*n, most significant first.
  - the console mirror on the HPS serial port /dev/ttyS1 (115200 8N1), where a
    BREAK asks the core to dead start from whatever is in memory.

Commands:
  peek WORD [COUNT]            print words in hex (WORD and COUNT accept 0o / 0x)
  poke WORD HEX [HEX ...]      write words
  fill WORD COUNT HEX          write one value to a range
  load FILE [WORD]             write a raw image into memory
  dump FILE WORD COUNT         save a range of memory to a file
  uart SECONDS [--break] [--send TEXT] [--until TEXT]
                               capture console output to stdout
  run FILE SECONDS [--until TEXT] [--poison WORD:COUNT ...]
                               hold BREAK (the core's master clear), write the
                               image, release BREAK to dead start, capture output
  session SECONDS [--break] [--type WAIT=KEYS]... [--until TEXT] [--screen C]...
                               work the two consoles of the CRAY X-MP core
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
SPAN = 8 << 20            # one million 64-bit words
TTY = '/dev/ttyS1'


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


def cmd_load(args):
    data = open(args[0], 'rb').read()
    word = num(args[1]) if len(args) > 1 else 0
    m = mem()
    m[word * 8:word * 8 + len(data)] = data
    print('loaded %d bytes at word %o' % (len(data), word))


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
POISON = bytes.fromhex('badc0ffee0ddf00d')


def cmd_run(args):
    path, seconds = args[0], float(args[1])
    until = args[args.index('--until') + 1].encode() if '--until' in args else None
    data = open(path, 'rb').read()
    fd = open_tty()
    fcntl.ioctl(fd, TIOCSBRK)             # master clear: the CPU stops
    time.sleep(0.05)
    m = mem()
    i = 2
    while i < len(args):
        if args[i] == '--poison':         # stale results must not look like fresh ones
            word, count = (num(x) for x in args[i + 1].split(':'))
            m[word * 8:(word + count) * 8] = POISON * count
            i += 1
        i += 1
    m[0:len(data)] = data
    termios.tcflush(fd, termios.TCIFLUSH)
    fcntl.ioctl(fd, TIOCCBRK)             # dead start
    got = b''
    end = time.time() + seconds
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
    os.close(fd)


def cmd_batch(args):
    """Run every NAME.img in a directory and compare memory with NAME.exp
    (lines of: octal word address, 16 hex digits) and the console output with
    NAME.con (hex bytes), as written by mkbatch.py."""
    d = args[0]
    timeout = float(args[args.index('--timeout') + 1]) if '--timeout' in args else 3.0
    names = sorted(f[:-4] for f in os.listdir(d) if f.endswith('.img'))
    fd = open_tty()
    m = mem()
    failed = []
    for n in names:
        data = open(os.path.join(d, n + '.img'), 'rb').read()
        exp = []
        for line in open(os.path.join(d, n + '.exp')):
            a, v = line.split()
            exp.append((int(a, 8), bytes.fromhex(v)))
        con = b''
        if os.path.exists(os.path.join(d, n + '.con')):
            con = bytes.fromhex(open(os.path.join(d, n + '.con')).read().strip())
        fcntl.ioctl(fd, TIOCSBRK)             # master clear
        time.sleep(0.02)
        for a, _ in exp:                      # stale results must not look like fresh ones
            m[a * 8:a * 8 + 8] = POISON
        m[0:len(data)] = data
        termios.tcflush(fd, termios.TCIFLUSH)
        fcntl.ioctl(fd, TIOCCBRK)             # dead start
        got = b''
        end = time.time() + timeout
        ok = False
        while time.time() < end and not ok:
            r, _, _ = select.select([fd], [], [], 0.005)
            if r:
                got += os.read(fd, 4096)
            ok = len(got) >= len(con) and all(m[a * 8:a * 8 + 8] == v for a, v in exp)
        if ok and got[:len(con)] != con:
            ok = False
        if not ok:
            failed.append(n)
            bad = [(a, v, m[a * 8:a * 8 + 8]) for a, v in exp if m[a * 8:a * 8 + 8] != v]
            print('FAIL %s: %d of %d words differ' % (n, len(bad), len(exp)))
            for a, v, w in bad[:4]:
                print('   %07o expected %s found %s' % (a, v.hex(), w.hex()))
            if got[:len(con)] != con:
                print('   console expected %r found %r' % (con[:40], got[:40]))
            sys.stdout.flush()
    os.close(fd)
    print('%d programs, %d failed' % (len(names), len(failed)))
    if failed:
        sys.exit(1)


# ---- key presses through a virtual keyboard (Linux uinput) ----
# Main_MiSTer picks the device up like any USB keyboard, so the keys take the
# same road into the core as a real one: Main, hps_io, ps2_key.
UI_SET_EVBIT, UI_SET_KEYBIT, UI_DEV_CREATE, UI_DEV_DESTROY = 0x40045564, 0x40045565, 0x5501, 0x5502
EV_SYN, EV_KEY = 0, 1
KEY_CTRL, KEY_SHIFT = 29, 42
NAMED = {'enter': 28, 'esc': 1, 'tab': 15, 'backspace': 14, 'space': 57, 'up': 103, 'down': 108, 'left': 105,
         'right': 106, 'f1': 59, 'f2': 60, 'f12': 88, 'delete': 111}
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
    os.write(fd, struct.pack('80sHHHHi', b'Cray1 test keyboard', 3, 0x1209, 0xC1A1, 1, 0) + bytes(4 * 64 * 4))
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


def render(raw):
    """The 24 lines of 80 characters a console shows after what it was sent
    (the sequences are those of rtl/terminal/term_ampex.v)."""
    lines = [bytearray(b' ' * 80) for _ in range(24)]
    line = column = n = 0

    def feed():
        nonlocal line
        if line < 23:
            line += 1
        else:
            lines.pop(0)
            lines.append(bytearray(b' ' * 80))

    while n < len(raw):
        c = raw[n]
        n += 1
        if c == 0x1B and n < len(raw):
            e = raw[n]
            n += 1
            if e == 0x3D and n + 1 < len(raw):
                line = min(max(raw[n] - 0x20, 0), 23)
                column = min(max(raw[n + 1] - 0x20, 0), 79)
                n += 2
            elif e == 0x47:
                n += 1
            elif e == 0x2A:
                lines = [bytearray(b' ' * 80) for _ in range(24)]
                line = column = 0
            elif e == 0x54:
                lines[line][column:] = b' ' * (80 - column)
            elif e == 0x52:
                lines.pop(line)
                lines.append(bytearray(b' ' * 80))
        elif c == 0x08:
            column = max(column - 1, 0)
        elif c == 0x0A:
            feed()
        elif c == 0x0C:
            column = min(column + 1, 79)
        elif c == 0x0D:
            column = 0
        elif 0x20 <= c < 0x7F:
            lines[line][column] = c
            if column < 79:
                column += 1
            else:
                column = 0
                feed()
    text = [l.decode().rstrip() for l in lines]
    while text and not text[-1]:
        text.pop()
    return '\n'.join(text)


def cmd_session(args):
    """session SECONDS [--break] [--type WAIT=KEYS]... [--until TEXT] [--gap SEC]
               [--screen C]... [--raw FILE]

    Work the two consoles of the CRAY X-MP core through the serial port, where
    bit 7 of a byte names the console: clear is the operator's console, set
    the station.  --type sends KEYS (\\r is RETURN) once a console has shown
    WAIT; several are taken in order.  WAIT is found whatever blanks and cursor
    movements lie between its characters; +N waits N milliseconds instead.  WAIT
    is looked for on the operator's console, or on the station if it begins
    with @0:, and KEYS go to the same console.  --until ends the session when
    its text has been shown after the last KEYS; --break first holds a serial
    BREAK, which starts the machine again.  What the operator's console prints
    is copied to the output as it comes; --screen prints a console's 24 lines
    at the end (0 the station, 3 the operator's).  Exit status 1 if the text of
    --until did not come."""
    seconds = float(args[0])
    typing, screens, until, gap, raw_file, do_break = [], [], None, 0.004, None, False

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
    raw = bytearray()
    typed_from = [0, 0]
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
                if not b >> 7 and (b == 0x0A or 0x20 <= b < 0x7F):
                    out.write(chr(b))
            out.flush()
        if said < len(typing):
            c, wait, keys, delay = typing[said]
            there = time.time() - waiting_since >= delay if delay is not None else wait in squeeze(bytes(console[c][typed_from[c]:]))
            if there:
                # the kernel drops a key that comes before it has finished its question
                time.sleep(0.05)
                for k in keys:
                    os.write(fd, bytes([k | (0x80 if c else 0)]))
                    time.sleep(gap)
                said += 1
                typed_from = [len(console[0]), len(console[1])]
                waiting_since = time.time()
        elif until and until[1] in squeeze(bytes(console[until[0]][typed_from[until[0]]:])):
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
    os.close(fd)
    if raw_file:
        open(raw_file, 'wb').write(raw)
    for c in screens:
        print('\n---- %s\n%s\n----' % ('station' if c else "operator's console", render(bytes(console[c]))))
    print('\n%.1f s; %d characters on the operator\'s console, %d on the station' % (time.time() - start, len(console[0]), len(console[1])))
    if until:
        print('the text was shown' if shown else 'the text was NOT shown')
        sys.exit(0 if shown else 1)


COMMANDS = {'keys': cmd_keys, 'batch': cmd_batch, 'run': cmd_run, 'peek': cmd_peek, 'poke': cmd_poke, 'fill': cmd_fill, 'load': cmd_load,
            'dump': cmd_dump, 'uart': cmd_uart, 'session': cmd_session}

if __name__ == '__main__':
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        sys.exit(__doc__)
    COMMANDS[sys.argv[1]](sys.argv[2:])

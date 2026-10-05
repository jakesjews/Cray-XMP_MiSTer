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
         'right': 106, 'f12': 88, 'delete': 111}
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


COMMANDS = {'keys': cmd_keys, 'batch': cmd_batch, 'run': cmd_run, 'peek': cmd_peek, 'poke': cmd_poke, 'fill': cmd_fill, 'load': cmd_load,
            'dump': cmd_dump, 'uart': cmd_uart}

if __name__ == '__main__':
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        sys.exit(__doc__)
    COMMANDS[sys.argv[1]](sys.argv[2:])

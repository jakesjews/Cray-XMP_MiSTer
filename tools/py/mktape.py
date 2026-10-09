#!/usr/bin/env python3
"""Tape files for the tape drive on the Peripheral Expander (the menu's Tape).

    mktape.py blank FILE [MBYTES]
    mktape.py list FILE

blank makes a blank tape of MBYTES megabytes (default 8, at most 16): a file
of bytes of all ones.  list says what a tape holds.

A tape file is in .tap form: a record is its length in four bytes, low byte
first, its bytes and the length again; a length of zero alone is a file mark.
The whole blocks of 512 bytes of the file are the tape, and what is on it ends
at the first length whose fourth byte is not zero, or that leads past the end
(docs/MACHINE.md, Tapes).
"""
import sys


def items(data):
    """The records (bytes) and file marks (None) of a tape, and where each begins."""
    room = len(data) // 512 * 512
    at = 0
    while at + 4 <= room:
        length = int.from_bytes(data[at:at + 4], 'little')
        if length >> 24 or (length and at + length + 8 > room):
            break
        yield at, data[at + 4:at + 4 + length] if length else None
        at += length + 8 if length else 4


def main():
    a = sys.argv[1:]
    if len(a) in (2, 3) and a[0] == 'blank':
        mbytes = int(a[2]) if len(a) == 3 else 8
        if not 1 <= mbytes <= 16:
            sys.exit('mktape: a tape is 1 to 16 megabytes')
        with open(a[1], 'wb') as f:
            f.write(b'\xff' * (mbytes << 20))
    elif len(a) == 2 and a[0] == 'list':
        data = open(a[1], 'rb').read()
        used = 0
        for at, record in items(data):
            if record is None:
                print('%8d  file mark' % at)
            else:
                text = ''.join(chr(c) if 32 <= c < 127 else '.' for c in record[:48])
                print('%8d  record of %5d bytes  %s' % (at, len(record), text))
            used = at + (len(record) + 8 if record is not None else 4)
        print('%8d  the end; the tape has room for %d bytes' % (used, len(data) // 512 * 512))
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main()

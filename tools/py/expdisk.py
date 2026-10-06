#!/usr/bin/env python3
"""The files on the disk of the Peripheral Expander (exp_disk.img).

  expdisk.py list IMAGE
  expdisk.py get IMAGE [DIR/]NAME FILE
  expdisk.py put IMAGE [DIR/]NAME FILE [--date MM/DD/YY] [--time HH:MM:SS]
  expdisk.py rm IMAGE [DIR/]NAME
  expdisk.py gettext IMAGE [DIR/]NAME FILE
  expdisk.py puttext IMAGE [DIR/]NAME FILE [--date MM/DD/YY] [--time HH:MM:SS]

The I/O Subsystem keeps the kernel, COS, parameter files, jobs and programs
on this disk, in a file system of its own.  The station reads them from it:
SUBMIT,NAME queues a job from directory STATION, and a job gets a file with
FETCH,DN=X,MF=AP,TEXT=DIR/NAME.  put adds a file so that this works; get
copies one out; rm takes one off and gives its room back.

put stores a file as it is, so what COS is to read as a dataset must already
be one: a program made by COS-Tools' ldr or by ack is.  puttext makes a
dataset of a text file, a record a line, as a job has to be; a line that is
only /EOF ends a file of the dataset, as it does in a card deck.  gettext
writes a dataset of text out the same way.

A dataset is blocks of 512 words.  Each block begins with a block control
word, and a record, a file and the dataset end with a record control word;
every control word counts the words to the next one.  A record control word
also holds the bits not used in the record's last word and how many blocks
back its record and its file began (COS reference manual SR-0011, blocked
format; the fields are as COS-Tools' cosdataset.h names them).

How the disk is laid out (worked out from the disk of the COS 1.17 set and
from the notes in cray-sim's exp_disk_create, whose code this does not use):

  - 823 cylinders, 5 heads, 35 sectors of 512 bytes.  The software uses the
    first 32 sectors of a track, in blocks of 8: a block is 4,096 bytes, or
    512 words, and there are four to a track.  Parcels are stored high byte
    first.
  - Block 3 is the label: 0xFFFF, the volume's name, dates.
  - Block 4 lists the free room: at parcel 7 the number of entries, then for
    each the first block and the number of blocks.
  - Block 5 is the directory: from parcel 8, 34 entries of 60 parcels.  An
    entry has bit 15 of its first parcel set when in use, then the directory
    name and the file name (16 characters each), date and time (8 each), the
    length in words (two parcels), three parcels not used here, the number of
    blocks, the number of pieces, and for each piece its first block and its
    number of blocks.

Only what that one directory block holds is handled: 34 files, each in one
piece when written here.
"""
import struct
import sys

BLOCK = 4096
LABEL, FREE, DIRECTORY = 3, 4, 5
ENTRIES, ENTRY = 34, 60


def offset(block):
    """Where a block is in the image: four to a track of 35 sectors."""
    return ((block // 4) * 35 + (block % 4) * 8) * 512


def parcels(image, block):
    at = offset(block)
    return list(struct.unpack('>2048H', image[at:at + BLOCK]))


def store(image, block, values):
    at = offset(block)
    image[at:at + BLOCK] = struct.pack('>2048H', *values)


def text(values):
    return struct.pack('>%dH' % len(values), *values).rstrip(b'\0 ').decode('latin-1')


def packed(string, count):
    raw = string.encode('ascii').ljust(2 * count, b'\0')
    return list(struct.unpack('>%dH' % count, raw))


def entries(image):
    """The directory: (slot, directory, name, date, time, words, pieces) for each file."""
    d = parcels(image, DIRECTORY)
    out = []
    for slot in range(ENTRIES):
        e = d[8 + ENTRY * slot:8 + ENTRY * (slot + 1)]
        if e[0] & 0x8000:
            pieces = [(e[32 + 2 * k], e[33 + 2 * k]) for k in range(e[31])]
            out.append((slot, text(e[1:9]), text(e[9:17]), text(e[17:21]), text(e[21:25]), e[25] << 16 | e[26], pieces))
    return out


def split(path):
    directory, _, name = path.upper().rpartition('/')
    if not name or len(name) > 15 or len(directory) > 15:
        sys.exit('expdisk: a name has 1 to 15 characters, with a directory of up to 15 in front: %s' % path)
    return directory, name


def find(image, path):
    directory, name = split(path)
    for e in entries(image):
        if (e[1], e[2]) == (directory, name):
            return e
    return None


def check(image):
    if len(image) != 823 * 5 * 35 * 512 or parcels(image, LABEL)[0] != 0xFFFF:
        sys.exit('expdisk: this is not a disk of the Peripheral Expander with a label')


def listing(image):
    label = parcels(image, LABEL)
    free = parcels(image, FREE)
    print('volume %s' % text(label[1:5]))
    for _, directory, name, date, time, words, _ in entries(image):
        print('%-31s %8d words  %s %s' % ((directory + '/' if directory else '') + name, words, date, time))
    room = sum(free[9 + 2 * k] for k in range(free[7]))
    print('%d of %d files; %d blocks of 512 words free' % (len(entries(image)), ENTRIES, room))


def get(image, path):
    e = find(image, path)
    if not e:
        sys.exit('expdisk: no file %s' % path)
    data = b''.join(bytes(image[offset(b):offset(b) + BLOCK]) for first, count in e[6] for b in range(first, first + count))
    return data[:8 * e[5]]


def put(image, path, data, date, time):
    directory, name = split(path)
    if find(image, path):
        sys.exit('expdisk: there is a file %s already; rm it first' % path)
    used = {e[0] for e in entries(image)}
    slots = [s for s in range(ENTRIES) if s not in used]
    if not slots:
        sys.exit('expdisk: the directory is full (%d files)' % ENTRIES)
    words = (len(data) + 7) // 8
    blocks = max(1, (words + 511) // 512)
    free = parcels(image, FREE)
    for k in range(free[7]):
        first, count = free[8 + 2 * k], free[9 + 2 * k]
        if count >= blocks:
            break
    else:
        sys.exit('expdisk: no room for %d blocks in one piece' % blocks)
    free[8 + 2 * k], free[9 + 2 * k] = first + blocks, count - blocks
    store(image, FREE, free)
    data = data.ljust(blocks * BLOCK, b'\0')
    for n in range(blocks):
        at = offset(first + n)
        image[at:at + BLOCK] = data[n * BLOCK:(n + 1) * BLOCK]
    d = parcels(image, DIRECTORY)
    entry = [0x8000] + packed(directory, 8) + packed(name, 8) + packed(date, 4) + packed(time, 4)
    entry += [words >> 16, words & 0xFFFF, 0, 0, 0, blocks, 1, first, blocks]
    entry += [0] * (ENTRY - len(entry))
    at = 8 + ENTRY * slots[0]
    d[at:at + ENTRY] = entry
    store(image, DIRECTORY, d)
    return blocks


def rewrite(image, path, data):
    """Give a file new contents in the room it has; False if they do not fit there."""
    e = find(image, path)
    if not e:
        sys.exit('expdisk: no file %s' % path)
    blocks = [b for first, count in e[6] for b in range(first, first + count)]
    if len(data) > len(blocks) * BLOCK:
        return False
    data = data.ljust(len(blocks) * BLOCK, b'\0')
    for n, b in enumerate(blocks):
        image[offset(b):offset(b) + BLOCK] = data[n * BLOCK:(n + 1) * BLOCK]
    words = (len(data.rstrip(b'\0')) + 7) // 8
    d = parcels(image, DIRECTORY)
    at = 8 + ENTRY * e[0]
    d[at + 25], d[at + 26] = words >> 16, words & 0xFFFF
    store(image, DIRECTORY, d)
    return True


def remove(image, path):
    e = find(image, path)
    if not e:
        sys.exit('expdisk: no file %s' % path)
    free = parcels(image, FREE)
    for first, count in e[6]:
        # room that lies right before a free piece joins it; other room gets an entry of its own
        for k in range(free[7]):
            if free[8 + 2 * k] == first + count:
                free[8 + 2 * k], free[9 + 2 * k] = first, free[9 + 2 * k] + count
                break
        else:
            k = free[7]
            free[8 + 2 * k], free[9 + 2 * k] = first, count
            free[7] += 1
    store(image, FREE, free)
    d = parcels(image, DIRECTORY)
    at = 8 + ENTRY * e[0]
    d[at:at + ENTRY] = [0] * ENTRY
    store(image, DIRECTORY, d)


EOR, EOF, EOD = 0o10, 0o16, 0o17


def unblock(data):
    """The files of a blocked dataset, each a list of records (bytes)."""
    words = struct.unpack('>%dQ' % (len(data) // 8), data[:len(data) // 8 * 8])
    files, records, record, at = [], [], b'', 0
    while at < len(words):
        control = words[at]
        kind, ahead = control >> 60, control & 0o777
        if kind == EOR:
            unused = control >> 54 & 0o77
            records.append(record[:len(record) - unused // 8])
            record = b''
        elif kind == EOF:
            files.append(records)
            records = []
        elif kind == EOD:
            return files
        elif kind != 0:
            sys.exit('expdisk: not a blocked dataset (word %d)' % at)
        record += data[8 * (at + 1):8 * (at + 1 + ahead)]
        at += 1 + ahead
    sys.exit('expdisk: the dataset has no end')


def block(files):
    """A blocked dataset of files, each a list of records (bytes)."""
    words = []       # control words as [kind, unused bits, file's block, record's block], data as bytes
    last = [None]    # the control word whose count of words ahead is still open

    def control(kind, unused=0, began_file=0, began_record=0):
        if len(words) % 512 == 0 and kind != 0:
            control(0)
        here = len(words) // 512
        if last[0] is not None:
            words[last[0]][4] = len(words) - last[0] - 1
        last[0] = len(words)
        words.append([kind, unused, here - began_file, here - began_record, 0, here])

    def data(chunk):
        for n in range(0, len(chunk), 8):
            if len(words) % 512 == 0:
                control(0)
            words.append(chunk[n:n + 8])

    control(0)
    for records in files:
        file_block = len(words) // 512
        for record in records:
            record_block = len(words) // 512
            unused = -len(record) % 8
            data(record + bytes(unused))
            control(EOR, 8 * unused, file_block, record_block)
        control(EOF, 0, file_block, len(words) // 512)
    control(EOD)
    words[-1][2] = words[-1][3] = 0
    out = bytearray()
    for w in words:
        if isinstance(w, bytes):
            out += w
        elif w[0] == 0:
            out += struct.pack('>Q', w[5] << 9 | w[4])
        else:
            out += struct.pack('>Q', w[0] << 60 | w[1] << 54 | (w[2] & 0xFFFFF) << 24 | (w[3] & 0x7FFF) << 9 | w[4])
    return bytes(out)


def from_text(text_bytes):
    files, records = [], []
    for line in text_bytes.decode('ascii').splitlines():
        if line.strip() == '/EOF':
            files.append(records)
            records = []
        else:
            records.append(line.rstrip().encode('ascii'))
    if records or not files:
        files.append(records)
    return block(files)


def to_text(data):
    out = []
    for n, records in enumerate(unblock(data)):
        if n:
            out.append(b'/EOF')
        out += records
    return b'\n'.join(out) + b'\n'


def main(argv):
    options = {'--date': '01/01/89', '--time': '01:01:01'}
    args = []
    while argv:
        a = argv.pop(0)
        if a in options and argv:
            options[a] = argv.pop(0)
        else:
            args.append(a)
    if len(args) < 2 or args[0] not in ('list', 'get', 'put', 'rm', 'gettext', 'puttext'):
        sys.exit(__doc__)
    image = bytearray(open(args[1], 'rb').read())
    check(image)
    if args[0] == 'list' and len(args) == 2:
        listing(image)
    elif args[0] in ('get', 'gettext') and len(args) == 4:
        data = get(image, args[2])
        open(args[3], 'wb').write(to_text(data) if args[0] == 'gettext' else data)
    elif args[0] in ('put', 'puttext') and len(args) == 4:
        if len(options['--date']) != 8 or len(options['--time']) != 8:
            sys.exit('expdisk: a date is MM/DD/YY and a time HH:MM:SS')
        data = open(args[3], 'rb').read()
        blocks = put(image, args[2], from_text(data) if args[0] == 'puttext' else data, options['--date'], options['--time'])
        open(args[1], 'wb').write(image)
        print('%s: %d blocks' % (args[2].upper(), blocks))
    elif args[0] == 'rm' and len(args) == 3:
        remove(image, args[2])
        open(args[1], 'wb').write(image)
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main(sys.argv[1:])

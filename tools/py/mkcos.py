#!/usr/bin/env python3
"""Make what the CRAY X-MP core needs on the MiSTer's SD card to run COS 1.17,
from the COS 1.17 set as the cray-sim project distributes it.

  mkcos.py SYSTEM_DIR OUT_DIR

SYSTEM_DIR holds boot_tape.tap, exp_disk.img, the nine drive images
biop_dk20.img to biop_dk32.img and target/cos_117/iop_kern.bin.  Written are

  OUT_DIR/games/CrayXMP/cos117.ios    the boot file: kernel and boot tape
  OUT_DIR/games/CrayXMP/exp_disk.img  the disk of the Peripheral Expander, with
                                      the striped group of drives switched off
                                      and the programs of software/cos-tools
                                      added
  OUT_DIR/games/CrayXMP/drives.img    the nine drives, one after another
  OUT_DIR/games/CrayXMP/printer.txt   takes what the printer prints: 8 MB of
                                      empty lines, which the core fills from
                                      the top
  OUT_DIR/_Computer/COS 1.17.mgl      starts the core with all four in place

Copy the two folders onto the SD card.  drives.img is 5.5 GB, so the card (or
the folder the MiSTer looks for games in) must not be FAT32.

The one change to the software is in DEADSTART, the list of parameters COS is
started with.  In older copies of the set it makes drives 24 to 26 one striped
group, STRIPE-1.  What COS writes to that group it does not always read back:
when a read runs past the last sector of a track, the I/O Subsystem's
software hands back other sectors than the ones COS wrote at the start of the
next track, and a dataset that happens to land on the group ends in BLOCK
NUMBER ERROR.  The group is switched off the way cray-sim's own later copy of
the file has it: STRIPE-1 not available, and 29-1-22A named in its place.  The
six other drives remain.

Added to the disk are the assembler, the loader and the copy command of
software/cos-tools, as BIN/CAL, BIN/LDR and BIN/COPYF, and a job, JTOOLS.
SUBMIT,JTOOLS at the station saves them, and the text editor and the dataset
lister that are on the disk already, as permanent datasets and enters them as
commands, so that a later job or session can say CAL or TEDI without fetching
anything.
"""
import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkboot import boot_file  # noqa: E402
import expdisk  # noqa: E402

DRIVES = (0o20, 0o21, 0o22, 0o24, 0o25, 0o26, 0o30, 0o31, 0o32)   # the BIOP's disk channels
DRIVE_BYTES = 823 * 10 * 18 * 4096      # cylinders, head groups, sectors, bytes
PRINTER_BYTES = 8 << 20

MGL = '''<mistergamedescription>
    <rbf>_Computer/CrayXMP</rbf>
    <file delay="1" type="s" index="0" path="exp_disk.img"/>
    <file delay="1" type="s" index="1" path="drives.img"/>
    <file delay="1" type="s" index="2" path="printer.txt"/>
    <file delay="1" type="f" index="1" path="cos117.ios"/>
</mistergamedescription>
'''


def stripe_off(disk):
    """Switch the striped group off in the DEADSTART parameters of an expander disk; True if changed."""
    if not expdisk.find(disk, 'STATION/DEADSTART'):
        return False
    text = expdisk.get(disk, 'STATION/DEADSTART')
    end = text.find(b'*END\r') + 5
    if end < 5:
        return False
    new = text[:end].replace(b'*CONFIG,DVN=STRIPE-1,AVAIL', b'*CONFIG,DVN=STRIPE-1,NAVAIL')
    new = new.replace(b'*DEVICE,LDV=STRIPE-1\r', b'*DEVICE,LDV=29-1-22A\r')
    if new == text[:end]:
        return False
    # the file's length is kept in words: the kernel hands COS that many, and COS takes
    # a last line that is cut short for a directive it does not know
    return expdisk.rewrite(disk, 'STATION/DEADSTART', new)


TOOLS = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'software', 'cos-tools'))
OURS = ('CAL', 'LDR', 'COPYF')      # of software/cos-tools
THEIRS = ('TEDI', 'AUDIT')          # on the disk as it comes


def add_tools(disk):
    """Put the programs of software/cos-tools on an expander disk, and the job that installs them; the names added."""
    added = []
    for name in OURS:
        path = 'BIN/' + name
        if not expdisk.find(disk, path):
            expdisk.put(disk, path, open(os.path.join(TOOLS, name), 'rb').read(), '01/01/89', '01:01:01')
            added.append(path)
    job = ['JOB,JN=JTOOLS,T=60.', 'ACCOUNT,AC=CRAY,APW=XYZZY,UPW=QUASAR.']
    for name in OURS + THEIRS:
        job += ['FETCH,DN=%s,MF=AP,TEXT=BIN/%s.' % (name, name), 'SAVE,DN=%s,EXO=ON.' % name,
                'RELEASE,DN=%s.' % name, 'ACCESS,DN=%s,ENTER.' % name]
    if not expdisk.find(disk, 'STATION/JTOOLS'):
        text = ('\n'.join(job) + '\n').encode('ascii')
        expdisk.put(disk, 'STATION/JTOOLS', expdisk.from_text(text), '01/01/89', '01:01:01')
        added.append('STATION/JTOOLS')
    return added


def main(argv):
    if len(argv) != 2:
        sys.exit(__doc__)
    system, out = argv
    games = os.path.join(out, 'games', 'CrayXMP')
    computer = os.path.join(out, '_Computer')
    names = ['boot_tape.tap', 'exp_disk.img', os.path.join('target', 'cos_117', 'iop_kern.bin')]
    names += ['biop_dk%o.img' % channel for channel in DRIVES]
    missing = [n for n in names if not os.path.isfile(os.path.join(system, n))]
    if missing:
        sys.exit('mkcos: %s does not have %s' % (system, ', '.join(missing)))
    for n in names[3:]:
        if os.path.getsize(os.path.join(system, n)) > DRIVE_BYTES:
            sys.exit('mkcos: %s is larger than a DD-29 drive' % n)
    os.makedirs(games, exist_ok=True)
    os.makedirs(computer, exist_ok=True)

    kernel = open(os.path.join(system, names[2]), 'rb').read()
    tape = open(os.path.join(system, names[0]), 'rb').read()
    try:
        open(os.path.join(games, 'cos117.ios'), 'wb').write(boot_file(kernel, tape))
    except ValueError as e:
        sys.exit('mkcos: %s' % e)
    print('cos117.ios')

    disk = bytearray(open(os.path.join(system, 'exp_disk.img'), 'rb').read())
    changed = stripe_off(disk)
    added = add_tools(disk)
    open(os.path.join(games, 'exp_disk.img'), 'wb').write(disk)
    print('exp_disk.img' + (': STRIPE-1 switched off in DEADSTART' if changed else ''))
    if added:
        print('exp_disk.img: added ' + ', '.join(added))

    with open(os.path.join(games, 'drives.img'), 'wb') as joined:
        for n in names[3:]:
            with open(os.path.join(system, n), 'rb') as image:
                shutil.copyfileobj(image, joined, 16 << 20)
            # a short image is filled up, so that the next drive begins where the core looks for it
            joined.truncate((names[3:].index(n) + 1) * DRIVE_BYTES)
            joined.seek(0, os.SEEK_END)
            print('drives.img: %s' % n)

    # what was printed before stays: the core goes on behind it
    printer = os.path.join(games, 'printer.txt')
    if not os.path.exists(printer):
        open(printer, 'wb').write(b'\n' * PRINTER_BYTES)
        print('printer.txt')

    open(os.path.join(computer, 'COS 1.17.mgl'), 'w').write(MGL)
    print('COS 1.17.mgl')


if __name__ == '__main__':
    main(sys.argv[1:])

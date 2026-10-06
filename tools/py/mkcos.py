#!/usr/bin/env python3
"""Make what the CRAY X-MP core needs on the MiSTer's SD card to run COS 1.17,
from the COS 1.17 set as the cray-sim project distributes it.

  mkcos.py SYSTEM_DIR OUT_DIR

SYSTEM_DIR holds boot_tape.tap, exp_disk.img, the nine drive images
biop_dk20.img to biop_dk32.img and target/cos_117/iop_kern.bin.  Written are

  OUT_DIR/games/CrayXMP/cos117.ios    the boot file: kernel and boot tape
  OUT_DIR/games/CrayXMP/exp_disk.img  the disk of the Peripheral Expander, as it is
  OUT_DIR/games/CrayXMP/drives.img    the nine drives, one after another
  OUT_DIR/_Computer/COS 1.17.mgl      starts the core with all three in place

Copy the two folders onto the SD card.  drives.img is 5.5 GB, so the card (or
the folder the MiSTer looks for games in) must not be FAT32.
"""
import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkboot import boot_file  # noqa: E402

DRIVES = (0o20, 0o21, 0o22, 0o24, 0o25, 0o26, 0o30, 0o31, 0o32)   # the BIOP's disk channels
DRIVE_BYTES = 823 * 10 * 18 * 4096      # cylinders, head groups, sectors, bytes

MGL = '''<mistergamedescription>
    <rbf>_Computer/CrayXMP</rbf>
    <file delay="1" type="s" index="0" path="exp_disk.img"/>
    <file delay="1" type="s" index="1" path="drives.img"/>
    <file delay="1" type="f" index="1" path="cos117.ios"/>
</mistergamedescription>
'''


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

    shutil.copyfile(os.path.join(system, 'exp_disk.img'), os.path.join(games, 'exp_disk.img'))
    print('exp_disk.img')

    with open(os.path.join(games, 'drives.img'), 'wb') as joined:
        for n in names[3:]:
            with open(os.path.join(system, n), 'rb') as image:
                shutil.copyfileobj(image, joined, 16 << 20)
            # a short image is filled up, so that the next drive begins where the core looks for it
            joined.truncate((names[3:].index(n) + 1) * DRIVE_BYTES)
            joined.seek(0, os.SEEK_END)
            print('drives.img: %s' % n)

    open(os.path.join(computer, 'COS 1.17.mgl'), 'w').write(MGL)
    print('COS 1.17.mgl')


if __name__ == '__main__':
    main(sys.argv[1:])

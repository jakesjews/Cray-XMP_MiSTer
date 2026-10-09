#!/usr/bin/env python3
"""Build what the CRAY X-MP core needs on the MiSTer's SD card to run COS 1.17:
COS installed on one disk drive, with the programs of software/cos-tools as
its commands.

  mkcos.py OUT_DIR [--zip FILE] [--no-check]

It starts from COS 1.17 as it was recovered, in software/cos-1.17: the kernel
of the I/O Subsystem, its boot tape and the expander disk.  The system model
does the work (tools/target/release/cray-xmp-sys; `make tools` builds it).
Written are

  OUT_DIR/games/Cray-XMP/cos117.ios    the boot file: kernel and boot tape
  OUT_DIR/games/Cray-XMP/exp_disk.img  the disk of the Peripheral Expander
  OUT_DIR/games/Cray-XMP/drives.img    the disk drive, 607 MB, COS and the
                                       programs on it
  OUT_DIR/games/Cray-XMP/printer.txt   takes what the printer prints: 8 MB of
                                       empty lines, which the core fills from
                                       the top
  OUT_DIR/games/Cray-XMP/tape.tap      a blank tape for the menu's Tape: 8 MB
                                       of bytes of all ones (tools/py/mktape.py)
  OUT_DIR/_Computer/COS 1.17.mgl       starts the core with the first four in
                                       place
  OUT_DIR/games/Cray-XMP/licenses/     the notices of what is in the files

and with --zip all of that in one file, which is what a release carries.

How the drive is made.  The model is given an empty drive and the expander
disk with a list of parameters that names one drive only.  COS is started
with INSTALL, which writes its labels and catalogs to the drive.  Then COS is
started again as it is every day, with DEADSTART, and one job, JSETUP, fetches
every program and library from the expander disk and saves it; the programs
it also enters as commands.  Last, unless --no-check is given, COS is started from the finished
files and three jobs are run that assemble, compile and interpret a small
program each; what they print is compared.

What differs from the software as it was recovered, all of it in the two
lists of parameters on the expander disk, INSTALL and DEADSTART:

- One drive, the master device on channel 20, instead of nine: 607 MB on the
  card instead of 5.5 GB.  The drive is not reserved for requests by name
  (RBN): with nothing but reserved devices COS halts during the install.
- No striped group.  What COS writes to the group of drives 24 to 26 it does
  not always read back; the reference simulator shows the same.
- No device in Buffer Memory (BMR-0-20).  Buffer Memory is empty after every
  start, and with the device COS asks each time whether to label it anew.

Jobs say ACCOUNT,AC=CRAY,US=SYSTEM.  An interactive session is the user SYSTEM
whatever it says, and a dataset that a job saves without a user number is
SYSTEM's too after the next start, when the job that saved it cannot get at
it any more.  With the user number, jobs and sessions have their datasets in
common.

The other files of the expander disk are as they were.  Three jobs are added
to it as examples: JCAL, JFTN and JLISP.
"""
import gzip
import os
import shutil
import subprocess
import sys
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkboot import boot_file  # noqa: E402
import expdisk  # noqa: E402

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
TOOLS = os.path.join(ROOT, 'software', 'cos-tools')
SYSTEM = os.path.join(ROOT, 'software', 'cos-1.17')
MODEL = os.path.join(ROOT, 'tools', 'target', 'release', 'cray-xmp-sys')
CORE = 'Cray-XMP'
DRIVE_BYTES = 823 * 10 * 18 * 4096      # cylinders, head groups, sectors, bytes
PRINTER_BYTES = 8 << 20
TAPE_BYTES = 8 << 20
STAMP = ('01/01/89', '01:01:01')        # the date of the files that come on the disk

MGL = '''<mistergamedescription>
    <rbf>_Computer/%s</rbf>
    <file delay="1" type="s" index="0" path="exp_disk.img"/>
    <file delay="1" type="s" index="1" path="drives.img"/>
    <file delay="1" type="s" index="2" path="printer.txt"/>
    <file delay="1" type="f" index="1" path="cos117.ios"/>
</mistergamedescription>
''' % CORE

# of software/cos-tools: programs that become commands, libraries, and the text LISP starts from
COMMANDS = ('CAL', 'LDR', 'LIB', 'DASM', 'KFTC', 'LISPF4',
            'CHARGES', 'COPYD', 'COPYF', 'COPYR', 'NOTE', 'SKIPD', 'SKIPF', 'SKIPR')
LIBRARIES = ('IOLIB', 'INTFLIB', 'RTLIB', 'EMLIB', 'SYSLIB', 'CLIB', 'COSLIB', 'BASLIB', 'PASLIB')
STOCK = ('TEDI', 'AUDIT')               # programs on the expander disk as it comes
FORTRAN = 'IOLIB:INTFLIB:RTLIB:EMLIB:SYSLIB:CLIB'   # what a FORTRAN program is linked with
ACCOUNT = 'ACCOUNT,AC=CRAY,US=SYSTEM.'

# files of the expander disk that the disk JSETUP runs from has no room for
SPARE = ('STATION/COS_117_1', 'STATION/COS_117_2', 'STATION/JDIAGGO', 'STATION/JFLUSH',
         'STATION/JGENCAT', 'STATION/JINSTALL', 'STATION/JSYSDIR', 'STATION/RECALL',
         'STATION/RESTART', 'STATION/MINSTALL', 'BIN/DMP10', 'BIN/GENCAT', 'BIN/JCSDEF',
         'BIN/LOADCAT', 'BIN/PLD10', 'BIN/PRVDEF', 'BIN/RECALL', 'BIN/RECIO', 'BIN/RELOAD')

OTHER_DEVICES = ('29-1-21A', '29-1-22A', 'STRIPE-1', '29-1-24A', '29-1-25A', '29-1-26A',
                 '29-1-30A', '29-1-31A', '29-1-32A', 'BMR-0-20', 'MT0', 'MT1', 'MT2', 'MT3')


def parameters(start, master):
    """A list of parameters for START: the master device and nothing else."""
    lines = ['*' + start, '*SKIPEFT', '*CONFIG,DVN=MD-1-20A,AVAIL,' + master]
    lines += ['*CONFIG,DVN=%s,NAVAIL' % device for device in OTHER_DEVICES]
    lines += ['*END']
    text = ('\r'.join(lines) + '\r').encode('ascii')
    # the kernel hands COS whole words
    return text + bytes(-len(text) % 8)


INSTALL = parameters('INSTALL', 'MSD=Y,WDL=Y,VOL=N')
DEADSTART = parameters('DEADSTART', 'MSD=Y')

CAL_EXAMPLE = '''         IDENT     HELLO
         ENTRY     HELLO
         START     HELLO
HELLO    S0        4
         S1        ='Hello from COS'Z
         S2        O'17
         EX
         S0        0
         EX
         END
'''

FORTRAN_EXAMPLE = '''      PROGRAM PRIMES
      INTEGER I, J
      LOGICAL P
      PRINT 100, 50
      DO 20 I = 2, 50
        P = .TRUE.
        DO 10 J = 2, I - 1
          IF (MOD(I, J) .EQ. 0) P = .FALSE.
   10   CONTINUE
        IF (P) PRINT 200, I
   20 CONTINUE
  100 FORMAT(' PRIMES BELOW', I3)
  200 FORMAT(1X, I4)
      END
'''

LISP_EXAMPLE = '''(ROLLIN (OPEN0 'LISPINI T T))
(DEFINEQ (FACT (LAMBDA (N) (COND ((ZEROP N) 1) (T (TIMES N (FACT (SUB1 N))))))))
(FACT 10)
(EXIT)
'''

# compile the FORTRAN in the local dataset SRC, link it and run it
COMPILE = (['ACCESS,DN=%s.' % name for name in FORTRAN.split(':')]
           + ['MEMORY,FL,USER.', 'KFTC,I=SRC,O=CODE.', 'REWIND,DN=CODE.', 'CAL,X,I=CODE,L=0.',
              'LDR,AB,DN=$BLD,LIB=%s.' % FORTRAN, '$ABD.'])
# a job that is kept on the drive, for SUBMIT,DN=FTN. in a session: the same
# for the permanent dataset SRC, which the session wrote and saved.  What a job
# prints goes back to where it was submitted, and a session has no printer: the
# DISPOSE sends it to the station's.
FTN = ['JOB,JN=FTN,T=60.', ACCOUNT, 'DISPOSE,DN=$OUT,MF=AP,DEFER.', 'ACCESS,DN=SRC.'] + COMPILE

EXAMPLES = {
    'JCAL': ['JOB,JN=JCAL,T=60.', ACCOUNT, 'COPYF,O=SRC.', 'REWIND,DN=SRC.', 'CAL,I=SRC.',
             'LDR,AB,DN=$BLD.', '$ABD.', '/EOF'] + CAL_EXAMPLE.splitlines(),
    'JFTN': ['JOB,JN=JFTN,T=60.', ACCOUNT, 'COPYF,O=SRC.', 'REWIND,DN=SRC.'] + COMPILE
            + ['/EOF'] + FORTRAN_EXAMPLE.splitlines(),
    'JLISP': ['JOB,JN=JLISP,T=60.', ACCOUNT, 'ACCESS,DN=LISPSYS.', 'ACCESS,DN=LISPINI.',
              'MEMORY,FL,USER.', 'LISPF4.', '/EOF'] + LISP_EXAMPLE.splitlines(),
}
# what the three jobs print, each line as the printer has it
EXPECTED = ('Hello from COS', 'PRIMES BELOW 50', '  47', '3628800')


def job(lines):
    return expdisk.from_text(('\n'.join(lines) + '\n').encode('ascii'))


def tool(name):
    return open(os.path.join(TOOLS, name), 'rb').read()


def replace(disk, path, data):
    if expdisk.find(disk, path):
        expdisk.remove(disk, path)
    expdisk.put(disk, path, data, *STAMP)


def setup_disk(stock):
    """The expander disk the drive is made from: the programs, the libraries, and JSETUP."""
    disk = bytearray(stock)
    for path in SPARE:
        if expdisk.find(disk, path):
            expdisk.remove(disk, path)
    replace(disk, 'STATION/INSTALL', INSTALL)
    replace(disk, 'STATION/DEADSTART', DEADSTART)
    for name in COMMANDS + LIBRARIES:
        replace(disk, 'BIN/' + name, tool(name))
    replace(disk, 'BIN/LISPSYS', expdisk.from_text(tool('LISPSYS')))
    replace(disk, 'BIN/FTN', job(FTN))
    lines = ['JOB,JN=JSETUP,T=600.', ACCOUNT]
    for name in COMMANDS + STOCK:
        lines += ['FETCH,DN=%s,MF=AP,TEXT=BIN/%s.' % (name, name), 'SAVE,DN=%s,EXO=ON.' % name,
                  'RELEASE,DN=%s.' % name, 'ACCESS,DN=%s,ENTER.' % name]
    for name in LIBRARIES + ('LISPSYS', 'FTN'):
        lines += ['FETCH,DN=%s,MF=AP,TEXT=BIN/%s.' % (name, name), 'SAVE,DN=%s.' % name,
                  'RELEASE,DN=%s.' % name]
    # the interpreter reads its library of functions from the job's own deck and
    # writes its state, with them in it, to LISPINI
    lines += ['ACCESS,DN=LISPSYS.', 'MEMORY,FL,USER.', 'LISPF4.', 'SAVE,DN=LISPINI.', '/EOF']
    lines += tool('LISPINI').decode('ascii').splitlines()
    replace(disk, 'STATION/JSETUP', job(lines))
    return disk


def release_disk(stock):
    """The expander disk that goes on the card: as it was, with the two lists of
    parameters for one drive and the example jobs."""
    disk = bytearray(stock)
    replace(disk, 'STATION/INSTALL', INSTALL)
    replace(disk, 'STATION/DEADSTART', DEADSTART)
    for name, lines in EXAMPLES.items():
        replace(disk, 'STATION/' + name, job(lines))
    return disk


START = '''wait kernel ENTER DATE [MM/DD/YY]
run 100
type kernel 01/01/89
wait kernel ENTER TIME [HH:MM:SS]
run 100
type kernel %s
run 500
type kernel START COS_117 %s
wait kernel START COMPLETE
run 1000
type kernel STATION
wait station CRAY STATION.  VERSION 4.2.2, IOS.
type station LOGON
run 4000
type station STMSG
wait station ENTER CONFIGURATION CHANGES OR 'GO' TO CONTINUE
type station REPLY,0,GO
'''
INSTALLED = START % ('09:00:00', 'INSTALL') + '''run 12000
type station STMSG
wait station DISK DATA WILL BE DESTROYED
type station REPLY,1,GO
run 30000
type station STMSG,I
wait station STARTUP COMPLETE
run 5000
'''
JOBS = '''run 20000
type station STMSG,I
wait station STARTUP COMPLETE
type station CLASS,ALL,ON
run 1000
type station LIMIT,5
run 1000
'''
SETUP = START % ('10:00:00', 'DEADSTART') + JOBS + '''type station SUBMIT,JSETUP
wait printer JOB,JN=JSETUP
wait printer END OF JOB
run 5000
printer
'''
CHECK = START % ('11:00:00', 'DEADSTART') + JOBS + ''.join(
    'type station SUBMIT,%s\nwait printer JOB,JN=%s\nwait printer END OF JOB\nrun 3000\n' % (name, name)
    for name in EXAMPLES) + 'printer\n'


def model(work, name, disk, drive, script, save, wait):
    """Run the system model on an expander disk and a drive; what it printed."""
    system = os.path.join(work, name)
    os.makedirs(os.path.join(system, 'target', 'cos_117'))
    for item in ('boot_tape.tap', os.path.join('target', 'cos_117', 'iop_kern.bin')):
        shutil.copyfile(os.path.join(work, item), os.path.join(system, item))
    open(os.path.join(system, 'exp_disk.img'), 'wb').write(disk)
    os.symlink(drive, os.path.join(system, 'biop_dk20.img'))
    open(os.path.join(system, 'script'), 'w').write(script)
    command = [MODEL, system, '--script', os.path.join(system, 'script'), '--wait', str(wait), '--quiet']
    if save:
        os.makedirs(save)
        command += ['--save', save]
    done = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    text = done.stdout.decode('latin-1')
    if done.returncode:
        open(os.path.join(work, name + '.txt'), 'w').write(text)
        sys.exit('mkcos: the model stopped at step `%s`:\n%s\n(what it printed is in %s)'
                 % (name, done.stderr.decode('latin-1')[-600:], os.path.join(work, name + '.txt')))
    return text


def faults(text):
    """Lines of a job's log that say a statement failed."""
    return [line.rstrip() for line in text.splitlines()
            if ' ABORT ' in line or 'PD009' in line or ' CS0' in line or 'Trap ' in line]


def main(argv):
    zipped, check = None, True
    args = []
    while argv:
        a = argv.pop(0)
        if a == '--zip' and argv:
            zipped = argv.pop(0)
        elif a == '--no-check':
            check = False
        else:
            args.append(a)
    if len(args) != 1:
        sys.exit(__doc__)
    out = os.path.abspath(args[0])
    # as the model wants them laid out
    names = ['boot_tape.tap', 'exp_disk.img', os.path.join('target', 'cos_117', 'iop_kern.bin')]
    if not os.path.isfile(MODEL):
        sys.exit('mkcos: no %s; run `make tools`' % MODEL)
    games = os.path.join(out, 'games', CORE)
    computer = os.path.join(out, '_Computer')
    licenses = os.path.join(games, 'licenses')
    work = os.path.join(out, 'work')
    shutil.rmtree(work, ignore_errors=True)
    for directory in (games, computer, licenses, os.path.join(work, 'target', 'cos_117')):
        os.makedirs(directory, exist_ok=True)

    kernel = open(os.path.join(SYSTEM, 'iop_kern.bin'), 'rb').read()
    tape = open(os.path.join(SYSTEM, 'boot_tape.tap'), 'rb').read()
    stock = gzip.open(os.path.join(SYSTEM, 'exp_disk.img.gz'), 'rb').read()
    try:
        open(os.path.join(games, 'cos117.ios'), 'wb').write(boot_file(kernel, tape))
    except ValueError as e:
        sys.exit('mkcos: %s' % e)
    print('cos117.ios')
    open(os.path.join(work, names[0]), 'wb').write(tape)
    open(os.path.join(work, names[2]), 'wb').write(kernel)

    # an empty drive; the file takes no room until it is written to
    empty = os.path.join(work, 'empty.img')
    with open(empty, 'wb') as f:
        f.truncate(DRIVE_BYTES)
    setup = setup_disk(stock)
    model(work, 'install', setup, empty, INSTALLED, os.path.join(work, 'installed'), 300)
    print('drives.img: COS installed')
    text = model(work, 'setup', setup, os.path.join(work, 'installed', 'biop_dk20.img'), SETUP,
                 os.path.join(work, 'set'), 3600)
    saved, entered = text.count('SAVE    COMPLETE'), text.count('ACCESS  COMPLETE')
    # besides the programs and libraries: LISPSYS, FTN and LISPINI saved, LISPSYS accessed
    want = (len(COMMANDS + STOCK + LIBRARIES) + 3, len(COMMANDS + STOCK) + 1)
    if faults(text) or (saved, entered) != want:
        open(os.path.join(work, 'setup.txt'), 'w').write(text)
        sys.exit('mkcos: JSETUP saved %d datasets and accessed %d, not %d and %d; %d statements failed '
                 '(its printout is in %s)' % (saved, entered, want[0], want[1], len(faults(text)),
                                              os.path.join(work, 'setup.txt')))
    print('drives.img: %d programs entered as commands, %d datasets saved' % (len(COMMANDS + STOCK), saved))
    drive = os.path.join(work, 'set', 'biop_dk20.img')

    disk = release_disk(stock)
    if check:
        text = model(work, 'check', disk, drive, CHECK, None, 600)
        lines = [line.rstrip() for line in text.splitlines()]
        absent = [e for e in EXPECTED if not any(line == e or line.endswith('    ' + e) for line in lines)]
        if faults(text) or absent:
            open(os.path.join(work, 'check.txt'), 'w').write(text)
            sys.exit('mkcos: the example jobs did not print %s; %d statements failed (their printout is in %s)'
                     % (absent, len(faults(text)), os.path.join(work, 'check.txt')))
        print('checked: %s run from the finished files' % ', '.join(EXAMPLES))

    open(os.path.join(games, 'exp_disk.img'), 'wb').write(disk)
    print('exp_disk.img')
    shutil.copyfile(drive, os.path.join(games, 'drives.img'))
    print('drives.img')
    open(os.path.join(games, 'printer.txt'), 'wb').write(b'\n' * PRINTER_BYTES)
    print('printer.txt')
    open(os.path.join(games, 'tape.tap'), 'wb').write(b'\xff' * TAPE_BYTES)
    print('tape.tap')
    open(os.path.join(computer, 'COS 1.17.mgl'), 'w').write(MGL)
    print('COS 1.17.mgl')
    for directory in (TOOLS, SYSTEM):
        for name in sorted(os.listdir(directory)):
            if name.startswith('LICENSE'):
                shutil.copyfile(os.path.join(directory, name), os.path.join(licenses, name + '.txt'))
    shutil.rmtree(work)

    if zipped:
        with zipfile.ZipFile(zipped, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for directory, _, files in sorted(os.walk(out)):
                for name in sorted(files):
                    path = os.path.join(directory, name)
                    z.write(path, os.path.relpath(path, out))
        print('%s: %.1f MB' % (zipped, os.path.getsize(zipped) / 1e6))


if __name__ == '__main__':
    main(sys.argv[1:])

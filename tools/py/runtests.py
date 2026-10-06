#!/usr/bin/env python3
"""Run the simulation test suites.

    runtests.py quick [-j JOBS]   smoke tests under every start-up, the directed tests,
                                  the self-checking clock and interrupt tests, the
                                  floating-point reference vectors, 200 random programs;
                                  then the X-MP mode: its tests and 100 random programs;
                                  then the I/O Processor: 400 random programs, and
                                  the self-check of three of them together; then
                                  the X-MP core: its terminal on 400 random screens,
                                  its message when no boot file is loaded, and the
                                  self-check on the whole core with what it prints
    runtests.py full [-j JOBS]    quick, then 200,000 generated vectors for each
                                  floating-point operation, 10,000 random programs and
                                  2,000 for the X-MP mode; then, if the COS 1.17
                                  software is there (CRAY1_SYSTEM, or the directory
                                  research/Cray 1 Disk Image from Youtube), the system
                                  model dead starts it and runs a job, the I/O
                                  Processor follows the kernel's boot step by step,
                                  the I/O Subsystem boots it, the whole machine
                                  loads and starts COS, and the X-MP core boots the
                                  kernel, is reset, starts COS with the station
                                  logged on, and is reset with the CPU running

Needs the host tools (make tools) and the simulations (make sim).  Programs are
compared with the reference model by difftest.py; see docs/DEVELOPMENT.md.
"""
import glob
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PY = sys.executable
DIFF = os.path.join(ROOT, 'tools/py/difftest.py')
IOP = os.path.join(ROOT, 'tools/py/ioptest.py')
CORE = os.path.join(ROOT, 'tools/py/coretest.py')
FPBENCH = os.path.join(ROOT, 'sim/build/fp/Vfp_tb')
SYS = os.path.join(ROOT, 'tools/target/release/cray1-sys')
SYSTEM = os.environ.get('CRAY1_SYSTEM', os.path.join(ROOT, 'research/Cray 1 Disk Image from Youtube'))


def smoke_variants(out):
    """Write each smoke test once for every start-up its RUNTIMES line names."""
    os.makedirs(out, exist_ok=True)
    for old in glob.glob(os.path.join(out, '*.cal')):
        os.remove(old)
    made = []
    for path in sorted(glob.glob(os.path.join(ROOT, 'tests/smoke/*.cal'))):
        src = open(path).read()
        m = re.search(r'^\* RUNTIMES:(.*)$', src, re.M)
        name = os.path.basename(path)[:-4]
        for rt in (m.group(1).split() if m else ['direct']):
            text = src
            if rt != 'direct':
                text = text.replace('rt_direct.cal', 'rt_exch.cal')
                if rt in ('user', 'reloc'):
                    base = '0' if rt == 'user' else '1000'
                    text = re.sub(r'^(\s+INCLUDE\s+"rt_exch\.cal".*)$', r'\1\n         TUSER   ' + base,
                                  text, count=1, flags=re.M)
            made.append(os.path.join(out, '%s_%s.cal' % (name, rt)))
            open(made[-1], 'w').write(text)
    return made


def rtl_only():
    """Self-checking programs the reference model cannot run (clocks and interrupts from outside)."""
    print('== self-checking RTL tests', flush=True)
    asm = os.path.join(ROOT, 'tools/target/release/cray1')
    cpu = os.path.join(ROOT, 'sim/build/cpu/Vcray_cpu')
    good, total = 0, 0
    for cal in sorted(glob.glob(os.path.join(ROOT, 'tests/rtl_only/*.cal'))):
        img = os.path.join(ROOT, 'build', os.path.basename(cal)[:-4] + '.img')
        subprocess.run([asm, 'asm', cal, '-I', 'tests/rt', '-o', img], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        m = re.search(r'^\* SIM:(.*)$', open(cal).read(), re.M)      # more simulator arguments for this test
        extra = m.group(1).split() if m else []
        for mode in (['--mem', '0'], ['--mem', 'rand:1-9'], ['--mem', 'slow'], ['--mem', 'ddr3', '--step']):
            total += 1
            r = subprocess.run([cpu, '--image', img, '--cycles', '2000000', '--quiet'] + mode + extra, cwd=ROOT,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if r.returncode == 0:
                good += 1
            else:
                print('FAIL %s %s: exit %d' % (os.path.basename(cal), ' '.join(mode), r.returncode))
    print('%d runs, %d failed' % (total, total - good), flush=True)
    return good == total


def step(title, cmd):
    print('== ' + title, flush=True)
    r = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    lines = r.stdout.strip().splitlines()
    print('\n'.join(lines[-6:]), flush=True)
    return r.returncode == 0 and not any(re.search(r'[1-9]\d* (failed|mismatches)', l) for l in lines[-2:])


def main():
    a = sys.argv[1:]
    if not a or a[0] not in ('quick', 'full'):
        sys.exit(__doc__)
    jobs = a[a.index('-j') + 1] if '-j' in a else str(os.cpu_count() or 4)
    ok = True
    smoke = smoke_variants(os.path.join(ROOT, 'build/smoke'))
    ok &= step('smoke tests, %d start-up variants' % len(smoke),
               [PY, DIFF, 'file'] + smoke + ['-I', 'tests/rt', '-j', jobs])
    ok &= step('directed tests',
               [PY, DIFF, 'file'] + sorted(glob.glob(os.path.join(ROOT, 'tests/directed/*.cal'))) + ['-I', 'tests/rt', '-j', jobs])
    ok &= rtl_only()
    ok &= step('floating-point reference vectors', [FPBENCH, 'tests/fp/xmp_ref.vec'])
    xmp = sorted(glob.glob(os.path.join(ROOT, 'tests/xmp/*.cal')))
    ok &= step('X-MP mode: %d tests' % len(xmp), [PY, DIFF, 'file'] + xmp + ['-I', 'tests/rt', '-j', jobs])
    ok &= step('X-MP mode: 100 random programs', [PY, DIFF, 'rand', '1', '100', '-n', '250', '--xmp', '-j', jobs])
    ok &= step('I/O Processor: 400 random programs', [PY, IOP, 'rand', '1', '200', '-j', jobs])
    ok &= step('I/O Processors together: self-check', [PY, IOP, 'selftest'])
    ok &= step('X-MP core: the terminal on 400 random screens', [PY, CORE, 'screens'])
    ok &= step('X-MP core: the printer\'s file, 400 of them', [PY, CORE, 'spool'])
    ok &= step('X-MP core: no boot file', [PY, CORE, 'nofile'])
    ok &= step('X-MP core: the self-check, with its printing on the screen and in the file', [PY, CORE, 'printer'])
    if a[0] == 'quick':
        ok &= step('200 random programs', [PY, DIFF, 'rand', '1', '200', '-n', '250', '-j', jobs])
    else:
        vec = os.path.join(ROOT, 'build/fpvec')
        ok &= step('generate floating-point vectors',
                   ['cargo', 'run', '--release', '--quiet', '--manifest-path', 'tools/Cargo.toml', '-p', 'cray1-fp',
                    '--example', 'gen_vectors', '--', vec, '200000', '1'])
        for f in sorted(glob.glob(os.path.join(vec, '*.vec'))):
            ok &= step('floating point: ' + os.path.basename(f), [FPBENCH, f])
            ok &= step('floating point with gaps: ' + os.path.basename(f), [FPBENCH, f, '--gaps'])
        ok &= step('10,000 random programs', [PY, DIFF, 'rand', '1', '10000', '-n', '250', '-j', jobs])
        ok &= step('X-MP mode: 2,000 random programs',
                   [PY, DIFF, 'rand', '1001', '3000', '-n', '250', '--xmp', '-j', jobs])
        ok &= step('I/O Processor: 4,000 random programs', [PY, IOP, 'rand', '201', '2200', '-j', jobs])
        if os.path.exists(os.path.join(SYSTEM, 'boot_tape.tap')):
            ok &= step('system model: COS 1.17 dead starts and runs a job',
                       [SYS, SYSTEM, '--script', 'tests/sys/cos.script', '--quiet'])
            ok &= step('I/O Processor: the kernel boots on each of the three', [PY, IOP, 'kernel', SYSTEM])
            ok &= step('I/O Subsystem: the kernel boots', [PY, IOP, 'boot', SYSTEM])
            ok &= step('CPU and I/O Subsystem: COS is loaded and started', [PY, IOP, 'machine', SYSTEM, '--start'])
            ok &= step('X-MP core: the kernel boots, and again after a reset', [PY, CORE, 'boot', SYSTEM])
            ok &= step('X-MP core: COS is started and the station logs on', [PY, CORE, 'start', SYSTEM])
            ok &= step('X-MP core: a reset under a running CPU', [PY, CORE, 'restart', SYSTEM])
    print('ALL PASSED' if ok else 'FAILED')
    sys.exit(0 if ok else 1)


main()

#!/usr/bin/env python3
"""Prove that RTL modules still do what they did in an earlier revision.

    equiv.py [--rev REV] [--timeout SEC] [FILE.v ...]

For every module in the named files (default: each Verilog file under rtl/
that differs from REV, default HEAD), Yosys checks the working tree against
REV: same outputs, same next state of every register, and the same values
sent to every submodule, for all inputs.  Submodules are not looked into;
their own files are checked on their own.

This is for edits that must not change behaviour: formatting, lint clean-ups,
renaming nothing.  It needs registers, ports and submodule instances to keep
their names.  Exit status 0 when everything is proven.
"""
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SKIP = ('rtl/pll',)


def sources(base):
    out = []
    for d, _, names in os.walk(os.path.join(base, 'rtl')):
        for n in sorted(names):
            path = os.path.relpath(os.path.join(d, n), base)
            if n.endswith(('.v', '.sv')) and not path.startswith(SKIP):
                out.append(path)
    return sorted(out)


def read_cmd(base, path, lib):
    inc = ' '.join('-I%s' % os.path.join(base, d) for d in ('rtl/cray/cray-1x', 'rtl/terminal', os.path.dirname(path)))
    return 'read_verilog %s%s-defer %s %s' % ('-sv ' if path.endswith('.sv') else '', '-lib ' if lib else '', inc,
                                              os.path.join(base, path))


def load(base, path, module, name):
    """Yosys commands that leave `module` of the tree at `base` stashed as `name`."""
    cmds = ['design -reset']
    cmds += [read_cmd(base, p, True) for p in sources(base) if p != path]
    cmds.append(read_cmd(base, path, False))
    cmds += ['hierarchy -top %s' % module, 'rename -top %s' % name]
    cmds += ['proc', 'opt_clean', 'memory_map', 'opt_clean',
             # turn every submodule connection into a port of the module
             'expose -evert %s/t:* %s/t:$* %%d' % (name, name),
             'opt_clean', 'design -stash %s' % name]
    return cmds


def prove(gold_base, path, module, timeout, work):
    cmds = load(gold_base, path, module, 'gold') + load(ROOT, path, module, 'gate')
    cmds += ['design -reset', 'design -copy-from gold -as gold gold', 'design -copy-from gate -as gate gate',
             'equiv_make gold gate equiv', 'hierarchy -top equiv', 'equiv_simple', 'equiv_induct',
             'equiv_status -assert']
    script = os.path.join(work, 'equiv.ys')
    log = os.path.join(work, '%s.log' % module)
    open(script, 'w').write('\n'.join(cmds) + '\n')
    try:
        r = subprocess.run(['yosys', '-q', '-l', log, script], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                           timeout=timeout)
    except subprocess.TimeoutExpired:
        return 'no answer in %d s' % timeout
    text = open(log).read() if os.path.exists(log) else ''
    if r.returncode == 0 and 'Equivalence successfully proven' in text:
        return None
    lines = [l.strip() for l in text.splitlines() if 'Unproven $equiv' in l or l.startswith('ERROR')]
    return '; '.join(lines[:3]) or 'yosys exit %d (log %s)' % (r.returncode, log)


def main():
    a = sys.argv[1:]
    if '-h' in a or '--help' in a:
        sys.exit(__doc__)
    rev, timeout, files = 'HEAD', 600, []
    i = 0
    while i < len(a):
        if a[i] == '--rev': rev = a[i + 1]; i += 2
        elif a[i] == '--timeout': timeout = int(a[i + 1]); i += 2
        else: files.append(os.path.relpath(os.path.abspath(a[i]), ROOT)); i += 1
    work = tempfile.mkdtemp(prefix='equiv_')
    gold = os.path.join(work, 'gold')
    os.makedirs(gold)
    tar = subprocess.run(['git', 'archive', rev, 'rtl'], cwd=ROOT, stdout=subprocess.PIPE, check=True).stdout
    subprocess.run(['tar', '-x', '-C', gold], input=tar, check=True)
    if not files:
        files = [p for p in sources(ROOT) if os.path.exists(os.path.join(gold, p))
                 and open(os.path.join(gold, p), 'rb').read() != open(os.path.join(ROOT, p), 'rb').read()]
    failed = checked = 0
    for path in files:
        if not os.path.exists(os.path.join(gold, path)):
            print('%-44s not in %s, skipped' % (path, rev))
            continue
        text = open(os.path.join(ROOT, path)).read()
        for module in re.findall(r'^\s*module\s+(\w+)', text, re.M):
            checked += 1
            why = prove(gold, path, module, timeout, work)
            print('%-44s %-28s %s' % (path, module, 'same' if why is None else 'DIFFERENT: ' + why), flush=True)
            failed += why is not None
    print('%d checks, %d failed (logs in %s)' % (checked, failed, work))
    sys.exit(1 if failed else 0)


main()

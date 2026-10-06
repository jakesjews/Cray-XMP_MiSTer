#!/usr/bin/env python3
"""Drive the cores on a real MiSTer from the development machine.

For the CRAY X-MP core (set CRAY_CORE=CrayXMP for deploy, shot and direct-video):

  hil.py xmp-start [BOOTFILE] [-t SEC] [SESSION OPTIONS]
                                   start the core through an MGL that mounts exp_disk.img
                                   and drives.img of games/CrayXMP and loads the boot
                                   file (BOOTFILE is copied there first), then work the
                                   consoles as `session` does
  hil.py session [-t SEC] [--break] [--type WAIT=KEYS]... [--until TEXT] [--screen C]...
                                   work the consoles of the running core through the
                                   serial port (see mister_agent.py); --break starts
                                   the machine again first

For the CRAY-1 core:

  hil.py deploy [RBF]              copy the core and the on-device agent
  hil.py launch [IMAGE] [-t SEC]   start the core (loading IMAGE through an MGL,
                                   as the menu would) and print console output
  hil.py run IMAGE [-t SEC] [--until TEXT] [--poison WORD:COUNT]
                                   hold the CPU with a serial BREAK, write IMAGE into
                                   memory from Linux, release, print console output
  hil.py batch DIR [--timeout SEC] run a directory made by mkbatch.py and compare memory
  hil.py uart SEC [--break] [--send TEXT] [--until TEXT]
  hil.py keys TEXT [--gap SEC]     type on a virtual keyboard: characters, \\r, \\xHH for
                                   control keys, {f12} {enter} {esc} {up} {down} for others
  hil.py peek WORD [COUNT] | poke WORD HEX... | fill WORD COUNT HEX
  hil.py dump FILE WORD COUNT      copy a memory range to a local file
  hil.py shot OUT.png              take a screenshot (needs direct video off)
  hil.py direct-video on|off       per-core direct video override in MiSTer.ini
  hil.py sh COMMAND                run a shell command on the MiSTer

Environment: MISTER (default root@mister), MISTER_PW (default 1).
"""
import os
import subprocess
import sys
import time

HOST = os.environ.get('MISTER', 'root@mister')
PW = os.environ.get('MISTER_PW', '1')
ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
SSH_OPTS = ['-o', 'PreferredAuthentications=password', '-o', 'PubkeyAuthentication=no',
            '-o', 'StrictHostKeyChecking=accept-new', '-o', 'LogLevel=ERROR', '-o', 'ConnectTimeout=8']
CORE = os.environ.get('CRAY_CORE', 'Cray1')
XMP_GAMES = '/media/fat/games/CrayXMP'
XMP_MGL = '/tmp/CrayXMP_test.mgl'
RBF_DEV = '/media/fat/_Computer/%s.rbf' % CORE
GAMES = '/media/fat/games/%s' % CORE
AGENT = '/tmp/mister_agent.py'
MGL = '/tmp/%s_test.mgl' % CORE


def ssh(cmd, capture=False, check=True):
    # ssh exits 255 when it cannot connect; the network can drop briefly while a
    # core loads, so that case is retried.
    for attempt in range(6):
        p = subprocess.run(['sshpass', '-p', PW, 'ssh'] + SSH_OPTS + [HOST, cmd],
                           stdout=subprocess.PIPE if capture else None, stderr=subprocess.DEVNULL)
        if p.returncode != 255:
            break
        time.sleep(3)
    if check and p.returncode != 0:
        sys.exit('ssh command failed: %s' % cmd)
    return p.stdout if capture else None


def _scp(src, dst):
    # the MiSTer's network drops for a few seconds whenever a core loads
    for attempt in range(6):
        p = subprocess.run(['sshpass', '-p', PW, 'scp'] + SSH_OPTS + [src, dst], stderr=subprocess.DEVNULL)
        if p.returncode == 0:
            return
        time.sleep(3)
    sys.exit('scp failed: %s -> %s' % (src, dst))


def scp_to(local, remote):
    _scp(local, '%s:%s' % (HOST, remote))


def scp_from(remote, local):
    _scp('%s:%s' % (HOST, remote), local)


def push_agent():
    scp_to(os.path.join(ROOT, 'tools', 'py', 'mister_agent.py'), AGENT)


def agent(args, capture=False):
    return ssh('python3 %s %s' % (AGENT, ' '.join("'%s'" % a.replace("'", "'\\''") for a in args)), capture=capture)


def cmd_deploy(args):
    rbf = args[0] if args else os.path.join(ROOT, 'output_files', CORE + '.rbf')
    ssh('mkdir -p %s' % GAMES)
    scp_to(rbf, RBF_DEV)
    push_agent()
    print('deployed %s' % rbf)


def take_time(args, default):
    if '-t' in args:
        i = args.index('-t')
        t = float(args[i + 1])
        del args[i:i + 2]
        return t
    return default


def cmd_launch(args):
    seconds = take_time(args, 12)
    push_agent()
    if args:
        scp_to(args[0], GAMES + '/hil.cry')
        mgl = ('<mistergamedescription><rbf>_Computer/%s</rbf>'
               '<file delay="2" type="f" index="1" path="hil.cry"/></mistergamedescription>' % CORE)
        ssh("cat > %s <<'EOF'\n%s\nEOF" % (MGL, mgl))
        target = MGL
    else:
        target = RBF_DEV
    # start listening before the core loads so the banner is not missed
    ssh('(python3 %s uart %g > /tmp/cray_uart.txt 2>/tmp/cray_uart.err &) ; sleep 0.5; echo load_core %s > /dev/MiSTer_cmd'
        % (AGENT, seconds, target))
    time.sleep(seconds + 1)
    sys.stdout.buffer.write(ssh('cat /tmp/cray_uart.txt; cat /tmp/cray_uart.err >&2', capture=True))


def run_session(seconds, args, then=''):
    """Run the agent's session on the MiSTer, detached, because the network can
    drop while a core loads; `then` is a shell command to run once it listens."""
    quoted = ' '.join("'%s'" % a.replace("'", "'\\''") for a in args)
    script = ('python3 %s session %g %s > /tmp/xmp_session.txt 2>&1\necho $? > /tmp/xmp_session.done\n'
              % (AGENT, seconds, quoted))
    ssh("rm -f /tmp/xmp_session.done; cat > /tmp/xmp_session.sh <<'HILEOF'\n%sHILEOF\n"
        "(sh /tmp/xmp_session.sh > /dev/null 2>&1 &) ; sleep 0.5; %s" % (script, then or 'true'))
    end = time.time() + seconds + 30
    code = b''
    while time.time() < end and not code.strip():
        time.sleep(2)
        code = ssh('cat /tmp/xmp_session.done 2>/dev/null', capture=True, check=False) or b''
    sys.stdout.buffer.write(ssh('cat /tmp/xmp_session.txt', capture=True))
    sys.stdout.flush()
    if code.strip() != b'0':
        sys.exit(1)


def cmd_xmp_start(args):
    seconds = take_time(args, 60)
    push_agent()
    if args and not args[0].startswith('--'):
        scp_to(args.pop(0), XMP_GAMES + '/boot.ios')
    mgl = ('<mistergamedescription><rbf>_Computer/CrayXMP</rbf>'
           '<file delay="1" type="s" index="0" path="exp_disk.img"/>'
           '<file delay="1" type="s" index="1" path="drives.img"/>'
           '<file delay="1" type="f" index="1" path="boot.ios"/></mistergamedescription>')
    ssh("cat > %s <<'EOF'\n%s\nEOF" % (XMP_MGL, mgl))
    run_session(seconds, args, 'echo load_core %s > /dev/MiSTer_cmd' % XMP_MGL)


def cmd_session(args):
    seconds = take_time(args, 30)
    push_agent()
    run_session(seconds, args)


def cmd_run(args):
    seconds = take_time(args, 8)
    push_agent()
    scp_to(args[0], '/tmp/hil.cry')
    agent(['run', '/tmp/hil.cry', str(seconds)] + args[1:])


def cmd_batch(args):
    """batch DIR [--timeout S]: run a directory made by mkbatch.py on the hardware."""
    push_agent()
    tar = '/tmp/craybatch.tar'
    subprocess.run(['tar', 'cf', tar, '-C', args[0], '.'], check=True, env=dict(os.environ, COPYFILE_DISABLE='1'))
    scp_to(tar, '/tmp/craybatch.tar')
    ssh('rm -rf /tmp/craybatch && mkdir /tmp/craybatch && tar xf /tmp/craybatch.tar -C /tmp/craybatch')
    agent(['batch', '/tmp/craybatch'] + args[1:])


def cmd_shot(args):
    before = ssh('ls -t /media/fat/screenshots/%s 2>/dev/null | head -1' % CORE, capture=True).decode().strip()
    ssh('echo screenshot > /dev/MiSTer_cmd')
    for _ in range(20):
        time.sleep(0.5)
        newest = ssh('ls -t /media/fat/screenshots/%s 2>/dev/null | head -1' % CORE, capture=True).decode().strip()
        if newest and newest != before:
            scp_from('/media/fat/screenshots/%s/%s' % (CORE, newest.replace(' ', '\\ ')), args[0])
            print('saved', args[0])
            return
    sys.exit('no screenshot appeared')


def cmd_direct_video(args):
    # A per-core section in MiSTer.ini overrides the global setting.  Only the
    # [Cray1] section is touched; a one-time backup is kept next to the file.
    script = r"""
import re, os, shutil
p = '/media/fat/MiSTer.ini'
if not os.path.exists(p + '.cray1bak'):
    shutil.copy(p, p + '.cray1bak')
s = open(p).read()
s = re.sub(r'(?ms)^\[%s\][^\n]*\n(?:(?!\[).*\n?)*', '', s)
if %r == 'off':
    if not s.endswith('\n'):
        s += '\n'
    s += '[%s]\ndirect_video=0\n'
open(p, 'w').write(s)
""" % (CORE, args[0], CORE)
    ssh("python3 - <<'PYEOF'\n%s\nPYEOF" % script)
    print(ssh("grep -n -A2 '^\\[%s\\]' /media/fat/MiSTer.ini || echo 'no [%s] section (global setting applies)'" % (CORE, CORE), capture=True).decode())


def cmd_dump(args):
    push_agent()
    agent(['dump', '/tmp/hil.dump', args[1], args[2]])
    scp_from('/tmp/hil.dump', args[0])


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    c, args = sys.argv[1], sys.argv[2:]
    if c == 'deploy': cmd_deploy(args)
    elif c == 'launch': cmd_launch(args)
    elif c == 'xmp-start': cmd_xmp_start(args)
    elif c == 'session': cmd_session(args)
    elif c == 'run': cmd_run(args)
    elif c == 'batch': cmd_batch(args)
    elif c == 'shot': cmd_shot(args)
    elif c == 'direct-video': cmd_direct_video(args)
    elif c == 'dump': cmd_dump(args)
    elif c in ('uart', 'peek', 'poke', 'fill', 'keys'):
        push_agent()
        agent([c] + args)
    elif c == 'sh': ssh(' '.join(args))
    else: sys.exit(__doc__)


if __name__ == '__main__':
    main()

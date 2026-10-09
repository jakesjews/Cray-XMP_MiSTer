#!/usr/bin/env python3
"""Check the model's Ampex terminal against the terminal's firmware.

    d80test.py SEED CASES [-j JOBS] [--events MIN MAX] [--focus AREA]
               [--print] [--keys] [--keep DIR]

Each case is a random stream of bytes from the host and of keys, a few hundred
to a few thousand of them, for a terminal switched on with random switch
settings.  The stream is run on the firmware (d80emu.py: the program ROMs on
a model of the terminal's hardware) and on the reference model (the
screen_dump example of tools/crates/ios, which is the Screen of
src/screen.rs), an event at a time and each to rest, and everything is
compared every hundred events and at the end: every cell of every page as it
is in memory (character, protect bit, attributes), the page origins, page,
cursor, modes, the attribute latch, tab stops, keyboard lock, the status
line, the bytes sent to the host, the number of bells, and the firmware's
working variables that the next event can depend on.  The four tables of key
codes are compared outright first.

When a case differs the stream is cut down to the shortest start of it that
differs, and that is printed with the first thing that is different.  The
last line is `N cases, M failed`; the exit status is 1 if any failed.

Two ends of a case are not failures.  The firmware can go into a loop that it
never leaves (ESC X in protect mode with auto flip, for one): the case then
ends there, and passes if the model says "hung" behind the same event and not
before.  And with function keys the firmware can start to work on memory that
is not its function keys' (its stack): the model says "lost" where that
begins, and the case is compared up to there.  Both are counted in a line of
their own before the last.

The streams reach every control code and escape sequence, right and wrong, in
every mode; --focus keeps to one area (the names are in AREAS below), without
it each case draws its own.  Two things are left out unless asked for,
because after them the firmware is a different machine:

  --print  ESC P, ESC O, ESC J and ESC K.  The model has no printer, and the
           firmware is run with one that never becomes ready.
  --keys   programming and playing the function keys (ESC e ... ESC x and
           the PROG keys); the model is then built with them.

The firmware is named with --rom or in the environment variable
CRAY_XMP_D80_ROM.  The model must have been built:
cargo build --release -p cray-xmp-ios --examples (in tools).
"""
import multiprocessing
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import d80emu  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(HERE))
MODEL = os.environ.get('CRAY_XMP_SCREEN_DUMP',
                       os.path.join(ROOT, 'tools/target/release/examples/screen_dump'))
EVERY = 100                 # events between two comparisons
ESC = 0x1B


def flag(state, name):
    """From the line `hung H lost L` of a state: what the model says of the
    firmware, that it is in a loop for good, or that the model cannot follow
    it any more."""
    for line in state:
        words = line.split()
        if words[0] == 'hung':
            return words[words.index(name) + 1] == '1'
    return False


AREAS = ['mixed', 'text', 'cursor', 'erase', 'insert', 'tabs', 'protect', 'attributes',
         'pages', 'send', 'program', 'keyboard', 'noise']

# what an event stream is made of, and how much of each a mixed stream has
WEIGHTS = {
    'text': 30, 'word': 20, 'move': 16, 'bell': 1, 'control': 3, 'sub': 1, 'del': 1, 'nul': 1,
    'address': 10, 'corner': 4, 'erase_line': 4, 'erase_page': 3, 'clear': 2, 'clear_all': 1,
    'char': 5, 'line': 5, 'tab_set': 4, 'tab': 5, 'attribute': 5, 'write_attribute': 2,
    'write_protect': 3, 'protect': 2, 'block': 2, 'program': 1, 'flip': 2, 'lock': 1,
    'drawing': 3, 'field': 3, 'page': 2, 'send': 3, 'ask': 2, 'escape': 4, 'byte': 4,
    'key': 8, 'function': 4, 'reset': 1, 'typing': 1, 'pattern': 0.3, 'mode_error': 0.7,
    'form': 2, 'wall': 0.5, 'own_column': 0.4, 'scroll': 3, 'print': 0, 'transparent': 0,
    'record': 0, 'play': 0,
}

# what a stream that keeps to one area has more of
FOCUS = {
    'mixed': {},
    'text': {'text': 80, 'scroll': 30, 'move': 40, 'corner': 20},
    'cursor': {'address': 60, 'corner': 40, 'move': 60, 'ask': 15, 'flip': 6, 'page': 6},
    'erase': {'erase_line': 30, 'erase_page': 25, 'clear': 12, 'clear_all': 5, 'sub': 6,
              'write_protect': 12, 'protect': 8, 'attribute': 10, 'write_attribute': 6},
    'insert': {'char': 40, 'line': 40, 'flip': 8, 'page': 6, 'attribute': 8, 'write_attribute': 6,
               'write_protect': 8, 'protect': 4},
    'tabs': {'tab_set': 40, 'tab': 50, 'erase_line': 10, 'protect': 8, 'write_protect': 8,
             'corner': 15},
    'protect': {'protect': 14, 'write_protect': 25, 'form': 25, 'field': 20, 'tab': 25,
                'tab_set': 14, 'move': 40, 'send': 10, 'erase_page': 8, 'char': 10, 'flip': 5,
                'wall': 3, 'own_column': 3},
    'attributes': {'attribute': 40, 'write_attribute': 16, 'field': 25, 'write_protect': 8,
                   'drawing': 10, 'send': 8, 'line': 8, 'scroll': 8},
    'pages': {'page': 30, 'flip': 20, 'scroll': 20, 'line': 20, 'move': 30, 'clear_all': 4,
              'protect': 5, 'form': 5},
    'send': {'send': 40, 'ask': 10, 'write_attribute': 10, 'attribute': 20, 'write_protect': 14,
             'protect': 10, 'form': 10, 'program': 4, 'drawing': 10, 'move': 30, 'key': 20},
    'program': {'program': 12, 'control': 40, 'escape': 30, 'byte': 30, 'nul': 10, 'send': 8,
                'function': 30, 'key': 20, 'mode_error': 6},
    'keyboard': {'key': 120, 'function': 50, 'block': 8, 'lock': 5, 'program': 2, 'reset': 4,
                 'typing': 20, 'pattern': 2, 'mode_error': 3},
    'noise': {'byte': 200, 'escape': 60, 'control': 40},
}


def row_byte(rng):
    p = rng.random()
    if p < 0.55:
        return 0x20 + rng.randrange(24)
    if p < 0.75:
        return 0x20 + rng.choice((0, 1, 11, 12, 22, 23))
    if p < 0.87:
        return 0x20 + rng.randrange(24, 48)
    if p < 0.94:
        return rng.randrange(0x20)
    return rng.randrange(0x38, 0x80)


def column_byte(rng):
    p = rng.random()
    if p < 0.55:
        return 0x20 + rng.randrange(80)
    if p < 0.80:
        return 0x20 + rng.choice((0, 1, 71, 72, 75, 78, 79))
    if p < 0.92:
        return 0x20 + rng.randrange(80, 96)
    return rng.randrange(0x80)


def text(rng, n):
    return [0x20 + rng.randrange(0x5F) for _ in range(n)]


class Stream:
    """The events of a case, drawn one group at a time."""

    def __init__(self, rng, focus, printing, keys):
        self.rng = rng
        weights = dict(WEIGHTS)
        for name, w in FOCUS[focus].items():
            weights[name] = weights[name] + w
        if printing:
            # After ESC P the printer's queue is full for good and ESC J takes
            # nothing more: a third of the streams has ESC J first.
            if rng.random() < 0.35:
                weights['print'] = 0.1
                weights['transparent'] = 1.2
            else:
                weights['print'] = 6
                weights['transparent'] = 0.15
            weights['typing'] += 3
        if keys:
            weights['record'] = 40
            weights['play'] = 60
        # the function keys this stream programs, again and again, and plays
        self.own = [(rng.choice('ab'), rng.randrange(10)) for _ in range(rng.randint(2, 6))]
        # Few streams fill the memory of the function keys: the firmware
        # does not keep its books right once that has been full, and soon
        # after plays its stack.
        self.fill_memory = rng.random() < 0.12
        self.names = [n for n in weights if weights[n] > 0]
        self.weights = [weights[n] for n in self.names]
        # some streams hold a mode on for most of their length
        self.on = {m: rng.choice((0.3, 0.5, 0.5, 0.8)) for m in
                   ('write_attribute', 'write_protect', 'protect', 'block', 'flip', 'program', 'lock')}
        self.on['program'] = rng.choice((0.1, 0.25, 0.25, 0.6))
        self.on['lock'] = rng.choice((0.2, 0.4))
        # the last event took the firmware long
        self.slow = False

    def address(self):
        return [ESC, ord('='), row_byte(self.rng), column_byte(self.rng)]

    def group(self, firmware):
        """A list of events: ('h', byte), ('K', row, bit, shift, ctrl),
        ('P', 'a' or 'b', row, bit)."""
        rng = self.rng
        name = rng.choices(self.names, self.weights)[0]
        if self.slow and rng.random() < 0.4:
            name = 'relief'
        if firmware.mem[d80emu.V_FK_RECORDING] & 1 and rng.random() < 0.6:
            # a key is still being programmed: end that
            name = 'record'
        out = getattr(self, 'g_' + name)(firmware)
        return [e if isinstance(e, tuple) else ('h', e) for e in out]

    def g_relief(self, _):
        # With protect mode on and nothing but protected cells the firmware
        # walks through every page for each byte it gets, which takes the
        # terminal a fifth of a second and this test far longer: a stream
        # does not stay there for long.
        return self.rng.choice(([ESC, ord("'")], [0x1A], [ESC, ord('*')], [ESC, ord('+')]))

    def pair(self, name, on, off):
        return [ESC, ord(on if self.rng.random() < self.on[name] else off)]

    def g_text(self, _):
        return text(self.rng, self.rng.randint(1, 60))

    def g_word(self, _):
        return text(self.rng, self.rng.randint(1, 6))

    def g_move(self, _):
        return [self.rng.choice((0x08, 0x08, 0x09, 0x0A, 0x0A, 0x0A, 0x0B, 0x0C, 0x0C, 0x0D, 0x0D,
                                 0x1E, 0x1F))]

    def g_bell(self, _):
        return [0x07]

    def g_control(self, _):
        return [self.rng.randrange(0x20)]

    def g_sub(self, _):
        return [0x1A]

    def g_del(self, _):
        return [0x7F]

    def g_nul(self, _):
        return [0x00]

    def g_address(self, _):
        return self.address()

    def g_corner(self, _):
        rng = self.rng
        row = rng.choice((0, 0, 11, 12, 23, 23, 23))
        column = rng.choice((0, 0, 1, 77, 78, 79, 79, 79))
        out = [ESC, ord('='), 0x20 + row, 0x20 + column]
        if rng.random() < 0.6:
            out += text(rng, rng.randint(1, 4))
        return out

    def g_erase_line(self, _):
        return [ESC, ord(self.rng.choice('Tt'))]

    def g_erase_page(self, _):
        return [ESC, ord(self.rng.choice('Yy;:'))]

    def g_clear(self, _):
        return [ESC, ord(self.rng.choice('*+Z'))]

    def g_clear_all(self, _):
        return [ESC, ord('X')]

    def g_char(self, _):
        return [ESC, ord(self.rng.choice('QW'))]

    def g_line(self, _):
        return [ESC, ord(self.rng.choice('ER'))]

    def g_tab_set(self, _):
        return [ESC, ord(self.rng.choice('1111223'))]

    def g_tab(self, _):
        return self.rng.choice(([0x09], [0x09], [ESC, ord('i')], [ESC, ord('I')]))

    def g_attribute(self, _):
        return [ESC, ord(self.rng.choice('jklmnopq'))]

    def g_write_attribute(self, _):
        return self.pair('write_attribute', 'A', 'a')

    def g_write_protect(self, _):
        return self.pair('write_protect', ')', '(')

    def g_protect(self, _):
        return self.pair('protect', '&', "'")

    def g_block(self, _):
        return self.pair('block', 'B', 'C')

    def g_program(self, _):
        return self.pair('program', 'c', 'd')

    def g_flip(self, _):
        return self.pair('flip', 'v', 'w')

    def g_lock(self, _):
        return self.pair('lock', '#', '"')

    def g_drawing(self, _):
        rng = self.rng
        x = ord('A') + rng.randrange(11) if rng.random() < 0.85 else rng.randrange(0x80)
        return [ESC, ord('G'), x]

    def g_field(self, _):
        rng = self.rng
        p = rng.random()
        r1 = rng.randrange(24)
        r2 = min(23, r1 + rng.choice((0, 0, 0, 1, 2, rng.randrange(24))))
        c1 = rng.randrange(80)
        c2 = rng.randrange(c1, 80) if r1 == r2 else rng.randrange(80)
        if p < 0.6:
            pass
        elif p < 0.68:              # the end before the start
            r2 = r1
            c1, c2 = max(c1, c2), min(c1, c2)
        elif p < 0.74:
            r1, r2 = max(r1, r2), min(r1, r2)
        elif p < 0.82:              # one of the four off the screen
            which = rng.randrange(4)
            r1, c1, r2, c2 = [(24 + rng.randrange(8) if i in (0, 2) else 80 + rng.randrange(8))
                              if i == which else v for i, v in enumerate((r1, c1, r2, c2))]
        elif p < 0.88:              # the whole page
            r1, c1, r2, c2 = 0, 0, 23, 79
        else:
            return [ESC, ord('f'), row_byte(rng), column_byte(rng), row_byte(rng), column_byte(rng)]
        return [ESC, ord('f'), 0x20 + r1, 0x20 + c1, 0x20 + r2, 0x20 + c2]

    def g_page(self, _):
        return [ESC, ord(self.rng.choice('NNNF'))]

    def g_send(self, _):
        return [ESC, ord(self.rng.choice('4567'))]

    def g_ask(self, _):
        return [ESC, ord('?')]

    def g_escape(self, _):
        rng = self.rng
        out = [ESC, rng.randrange(0x80)]
        # sometimes with bytes behind it that a sequence with parameters eats
        out += [rng.randrange(0x80) for _ in range(rng.choice((0, 0, 0, 1, 2, 4)))]
        return out

    def g_byte(self, _):
        return [self.rng.randrange(0x80)]

    def g_key(self, _):
        rng = self.rng
        return [('K', rng.randrange(13), rng.randrange(8), int(rng.random() < 0.25),
                 int(rng.random() < 0.15))]

    def g_function(self, _):
        # the keys that the terminal carries out itself
        rng = self.rng
        row, bit = rng.choice(((5, 6), (5, 7), (6, 6), (6, 7), (7, 7), (8, 4), (8, 6), (10, 4),
                               (11, 4), (11, 6), (12, 1), (12, 2), (12, 3), (12, 4), (12, 5),
                               (12, 6), (9, 5), (8, 5), (9, 4), (10, 5), (10, 6), (2, 0), (2, 0),
                               (12, 0), (3, 0)))
        return [('K', row, bit, int(rng.random() < 0.4), int(rng.random() < 0.12))]

    def g_reset(self, _):
        # CTRL+CLEAR, sometimes on a locked keyboard
        return ([ESC, ord('#')] if self.rng.random() < 0.3 else []) + [('K', 11, 4, 0, 1)]

    def g_typing(self, _):
        # more keys at once than the terminal has room for when it is stuck
        rng = self.rng
        return [('K', rng.choice((1, 2, 3)), rng.randrange(2, 8), 0, 0) for _ in range(rng.randint(3, 16))]

    def g_pattern(self, _):
        # CTRL and the unmarked key: the test pattern over the page
        return [('K', 12, 0, 0, 1)]

    def g_mode_error(self, _):
        # the keys PROT MODE and WRITE PROT in program mode, and ESC c in the
        # modes it is refused in
        rng = self.rng
        if rng.random() < 0.5:
            out = [ESC, ord("'"), ESC, ord('('), ESC, ord('a'), ESC, ord('c')]
            out += [('K', 12, rng.choice((1, 3)), 1, 0)]
        else:
            out = [ESC, ord(rng.choice("&)A")), ESC, ord('c')]
        if rng.random() < 0.6:
            out += [ESC, ord('d')]
        return out

    def g_wall(self, _):
        # a page with no cell that is not protected, protect mode, and then
        # something that looks for one
        rng = self.rng
        out = [ESC, ord(')'), ESC, ord('f'), 0x20, 0x20, 0x20 + 23, 0x20 + 79]
        if rng.random() < 0.7:
            out += [ESC, ord('&')]
        out += rng.choice(([ESC, ord('5')], [ESC, ord('4')], [0x09], [ESC, ord('I')], [0x0A],
                           [ESC, ord('Q')], [ESC, ord('1')], [ESC, ord('7')], [ESC, ord('N')]))
        if rng.random() < 0.5:
            out += [ESC, ord('(')]
        return out

    def g_form(self, _):
        # fields: protected text with gaps, sometimes whole protected lines
        rng = self.rng
        out = []
        if rng.random() < 0.5:
            out += self.address()
        for _ in range(rng.randint(1, 4)):
            out += [ESC, ord(')')] + text(rng, rng.choice((1, 3, 8, 30, 80, 100)))
            out += [ESC, ord('(')] + text(rng, rng.choice((0, 1, 2, 5, 12)))
            if rng.random() < 0.3:
                out += [0x0D, 0x0A]
        if rng.random() < 0.5:
            out += [ESC, ord('&')]
        return out

    def g_own_column(self, _):
        # ESC 1 in protect mode with the cursor on the last cell of the page,
        # where the search before it ended: the cursor's own cell becomes
        # protected, and then something is done there
        rng = self.rng
        out = [ESC, ord('&'), ESC, ord('='), 0x20 + 23, 0x20 + 79, ESC, ord('1')]
        out += rng.choice(([ESC, ord('Q')], [ESC, ord('W')], [ESC, ord('4')], [ESC, ord('5')],
                           [0x41], [0x08], [ESC, ord('T')], [ESC, ord('2')]))
        return out

    def g_scroll(self, _):
        rng = self.rng
        out = [ESC, ord('='), 0x20 + rng.choice((21, 22, 23, 23)), 0x20 + rng.randrange(80)]
        for _ in range(rng.randint(1, 30)):
            out += text(rng, rng.choice((0, 3, 20, 85))) + rng.choice(([0x0A], [0x0D, 0x0A], [0x1F]))
        return out

    def g_print(self, _):
        return [ESC, ord(self.rng.choice('PPO'))]

    def g_transparent(self, _):
        # behind ESC J the terminal takes two to four bytes more
        rng = self.rng
        return rng.choice(([ESC, ord('J')], [ESC, ord('K')], [ESC, ord('J'), ESC, ord('K')],
                           [ESC, ord('J'), ESC, ord('T'), 0x41], [ESC, ord('J'), 0x41, ESC, ord('K'), 0x42],
                           [ESC, ord('J'), 0x41, 0x42, ESC, ord('K')], [ESC, ord('J'), 0x0D, ESC, ord('=')]))

    def g_record(self, firmware):
        # a key's contents: text, sequences, keys, and the two sequences that
        # switch between the screen and the host
        rng = self.rng
        if firmware.mem[d80emu.V_FK_RECORDING] & 1:
            return [ESC, ord('x')]
        if firmware.mem[d80emu.V_MODE] & 0x10:
            # a key that was being played has stopped for good: another one
            return self.g_play(firmware)
        if rng.random() < 0.85:
            prog, digit = rng.choice(self.own)
            k, n = ord(prog.upper()), ord('0') + digit
        else:
            k, n = rng.randrange(1, 0x80), rng.randrange(1, 0x80)
            # no key above the twentieth: the firmware takes the numbers 20
            # to 31 too, keeps those keys in the memory of the others and
            # loses itself
            index = ((k - 0x41) & 0xFF) * 10 & 0xFF
            index = (0x46 if index > 0x0A else index) + n - 0x30 & 0xFF
            if 20 <= index < 32 or ESC in (k, n):
                k, n = ord('A'), ord('0') + rng.randrange(10)
        # ('e', ...) is a byte from the host like ('h', ...), marked as the
        # one ESC e that run_case lets through
        out = [ESC, ('e', ord('e')), k, n]
        to_host = rng.random() < 0.4
        room = 0x43F0 - firmware.word(d80emu.V_FK_FREE) - 30
        for i in range(rng.randint(0, 7)):
            what = rng.random()
            if to_host and i == 1:
                more = [ESC, ord('S')]
            elif to_host and i == 3:
                more = [ESC, ord('V')]
            elif what < 0.42:
                more = text(rng, rng.randint(1, 12))
            elif what < 0.52:
                more = text(rng, rng.randint(40, 150))
            elif what < 0.56:
                more = [ESC, ord('S')]
            elif what < 0.62:
                more = [ESC, ord('V')]
            elif what < 0.7:
                more = self.address()
            elif what < 0.80:
                more = [ESC, ord(rng.choice('TtQWERjkAa)(45?GNv&cd'))]
            elif what < 0.82:
                # two errors in one go when the key is played: ESC c where it
                # is refused, and a cursor address off the screen (the first
                # two 7E are taken now, the other two when the key is played)
                more = [ESC, ord('c'), ESC, ord('='), 0x7E, 0x7E, 0x7E, 0x7E]
            elif what < 0.9:
                more = [rng.choice((0x0D, 0x0A, 0x08, 0x07, 0x7F, 0x1A, 0x09))]
            elif what < 0.97:
                more = self.g_function(firmware)
            else:
                more = [ESC, rng.choice((0x0D, 0x20, 0x07))]     # behind ESC S, a key stops at these
            if self.fill_memory or len(out) + len(more) < room:
                out += more
        if rng.random() < 0.93:
            out += [ESC, ord('x')]
        return out

    def g_play(self, firmware):
        rng = self.rng
        if firmware.mem[d80emu.V_FK_RECORDING] & 1:
            # a PROG key now would leave a key half programmed, which the
            # firmware does not survive
            return [ESC, ord('x')]
        digits = ((7, 2), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6), (0, 7), (7, 0), (7, 1))
        pad = ((8, 2), (9, 3), (9, 2), (9, 1), (10, 3), (10, 2), (10, 1), (11, 3), (11, 2), (11, 1))
        p = rng.random()
        if p < 0.75:
            prog, digit = rng.choice(self.own)
            row, bit = rng.choice((digits, pad))[digit]
        elif p < 0.9:
            prog = rng.choice('ab')
            row, bit = rng.choice(digits + pad)
        else:
            # PROG A and a key that is no digit: mostly nothing, and : ; @
            # are the keys B0, B1 and B6
            prog = 'a'
            row, bit = rng.choice(((7, 3), (5, 3), (7, 5), (2, 2), (4, 6), (6, 5), (12, 5), (2, 0)))
        return [('P', prog, row, bit)]


# ---------------------------------------------------------------- the two sides

def script_line(event):
    if event[0] == 'h':
        return 'h %02x' % event[1]
    return ' '.join(str(x) for x in event)


def wanted(t, event, options):
    """Not the events that start what a run is to be without: a print, the
    transparent mode, the programming of a function key (but for the ESC e
    the stream marks as its own, whose parameters it has chosen).  The letter
    behind ESC is known when it comes: the firmware's collector says that it
    waits for one."""
    letter = None
    if event[0] in 'he':
        letter = event[1]
    elif event[0] == 'K':
        code = t.key_code(event[1], event[2], event[3], event[4])
        if code == 0xD0 and not options['print']:
            return False
        if code < 0x80 and t.mem[d80emu.V_MODE] & 0x28:
            letter = code
    elif not options['keys']:
        return False
    if letter is None or t.mem[d80emu.V_ESC_CNT] != 0xFF:
        return True
    if letter in (ord('P'), ord('J')) and not options['print']:
        return False
    if letter == ord('e') and event[0] != 'e':
        return False
    return True


def config_words(config):
    return ['pages=%d' % config['pages'], 'duplex=%s' % ('half' if config['half_duplex'] else 'full'),
            'wrap=%d' % config['wrap'], 'bell=%d' % config['bell'], 'hz=%d' % (50 if config['hz50'] else 60),
            'lock=%d' % config['lock'], 'fk=%d' % config['fk'], 'printer=0']


def model_dumps(config, events, dump_after, directory):
    """Run the model: the states behind the events whose numbers are in
    `dump_after` (0 is the state switched on)."""
    lines = ['config ' + ' '.join(config_words(config))]
    if 0 in dump_after:
        lines.append('d')
    for i, event in enumerate(events):
        lines.append(script_line(event))
        if i + 1 in dump_after:
            lines.append('d')
    path = os.path.join(directory, 'script')
    with open(path, 'w') as f:
        f.write('\n'.join(lines) + '\n')
    done = subprocess.run([MODEL, path], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          universal_newlines=True)
    if done.returncode != 0:
        raise RuntimeError('screen_dump: status %d: %s' % (done.returncode, done.stderr.strip()))
    dumps = []
    current = []
    for line in done.stdout.split('\n'):
        if not line:
            continue
        current.append(line)
        if line == 'end':
            dumps.append(current)
            current = []
    return dumps


_booted = {}


def firmware_for(rom, config):
    """A terminal with these switches, switched on.  Switching on takes most
    of the time of a short case, so each kind is kept."""
    key = tuple(sorted(config.items()))
    if key not in _booted:
        t = d80emu.power_on(rom, config)
        _booted[key] = t.snapshot()
    t = d80emu.Terminal(rom, pages=config['pages'], half_duplex=config['half_duplex'],
                        wrap=config['wrap'], bell=config['bell'], hz50=config['hz50'],
                        lock=config['lock'], printer=config['printer'])
    t.restore(_booted[key])
    return t


def apply(t, event):
    if event[0] == 'h':
        t.host(event[1])
    elif event[0] == 'K':
        t.key(event[1], event[2], bool(event[3]), bool(event[4]))
    else:
        t.key(event[2], event[3], prog=event[1])


def first_difference(firmware, model):
    if len(firmware) != len(model):
        return 'the firmware prints %d lines, the model %d' % (len(firmware), len(model))
    for a, b in zip(firmware, model):
        if a == b:
            continue
        name = a.split()[0]
        if name == 'status' or name[0] == 'p' and name[1:].isdigit():
            ca, cb = a.split()[-1], b.split()[-1]
            for column in range(80):
                x, y = ca[3 * column:3 * column + 3], cb[3 * column:3 * column + 3]
                if x != y:
                    where = 'status line' if name == 'status' else 'page %s memory row %s' % (
                        name[1:], a.split()[1][1:])
                    return '%s column %d: firmware %s, model %s (attributes, then protect and character)' % (
                        where, column, x, y)
        return 'firmware: %s\n  model:    %s' % (a, b)
    return None


def make_config(rng, printing, keys):
    return {'pages': rng.choice((2, 2, 4)), 'half_duplex': rng.random() < 0.25,
            'wrap': rng.random() < 0.85, 'bell': rng.choice((0, 0, 0, 1, 40, 72, 79, 80, 100)),
            'hz50': rng.random() < 0.2, 'lock': rng.random() < 0.03, 'printer': False, 'fk': keys}


def run_case(job):
    seed, case, options = job
    rng = random.Random(seed * 1000003 + case)
    rom = d80emu.load_rom(options['rom'])
    config = make_config(rng, options['print'], options['keys'])
    focus = options['focus'] or rng.choice(AREAS)
    stream = Stream(rng, focus, options['print'], options['keys'])
    length = rng.randint(options['events'][0], options['events'][1])
    label = 'seed %d case %d (%s; %s)' % (seed, case, focus, ' '.join(config_words(config)))
    events = []
    states = {}
    hang = None
    try:
        t = firmware_for(rom, config)
        states[0] = t.dump()
        while len(events) < length:
            for event in stream.group(t):
                if not wanted(t, event, options):
                    continue
                if event[0] == 'e':
                    event = ('h', event[1])
                events.append(event)
                before = t.steps
                apply(t, event)
                stream.slow = t.steps - before > 50000
                if len(events) % EVERY == 0:
                    states[len(events)] = t.dump()
        if len(events) not in states:
            states[len(events)] = t.dump()
    except d80emu.Stuck as e:
        # the firmware has gone into a loop for good: the stream ends here,
        # and the model must say the same of its last event
        hang = str(e)
    with tempfile.TemporaryDirectory() as directory:
        if options['keep']:
            with open(os.path.join(options['keep'], 'seed%d-case%d' % (seed, case)), 'w') as f:
                f.write('config %s\n%s\nd\n' % (' '.join(config_words(config)),
                                                '\n'.join(script_line(e) for e in events)))
        if hang:
            last = model_dumps(config, events, set([len(events) - 1, len(events)]), directory)
            if flag(last[1], 'lost'):
                hang = None
            elif flag(last[0], 'hung') or not flag(last[1], 'hung'):
                return '%s\n  the firmware hangs in event %d (%s), the model does not\n%s' % (
                    label, len(events), hang, show(events))
        points = sorted(states)
        dumps = model_dumps(config, events, set(points), directory)
        if len(dumps) != len(points):
            return '%s\n  the model printed %d states, not %d' % (label, len(dumps), len(points))
        bad = None
        for point, dump in zip(points, dumps):
            if dump != states[point]:
                bad = point
                break
        if bad is None:
            return 'hang' if hang else 'lost' if flag(dumps[-1], 'lost') else None
        if bad == 0:
            return '%s\n  differs when switched on: %s' % (label, first_difference(states[0], dumps[0]))
        # the first event behind which the two differ: both again, from the
        # last comparison that agreed, a state behind every event
        low = max(p for p in points if p < bad)
        t = firmware_for(rom, config)
        for event in events[:low]:
            apply(t, event)
        t.dump()
        steps = list(range(low + 1, bad + 1))
        model = model_dumps(config, events, set([low] + steps), directory)[1:]
        for point, state in zip(steps, model):
            if flag(state, 'lost'):
                # the model has said that it cannot follow the firmware
                # from here on; up to here the two agreed
                return 'lost'
            apply(t, events[point - 1])
            firmware = t.dump()
            if state != firmware:
                return '%s\n  differs behind %d of %d events: %s\n%s' % (
                    label, point, len(events), first_difference(firmware, state), show(events[:point]))
        return '%s\n  differs behind event %d, but not when the events from %d on are run one by one' % (
            label, bad, low)


def show(events):
    """A stream as the lines of a script, bytes from the host run together."""
    lines = []
    run = []
    for event in events:
        if event[0] == 'h':
            run.append('%02x' % event[1])
            if len(run) == 24:
                lines.append('    h ' + ' '.join(run))
                run = []
        else:
            if run:
                lines.append('    h ' + ' '.join(run))
                run = []
            lines.append('    ' + script_line(event))
    if run:
        lines.append('    h ' + ' '.join(run))
    return '\n'.join(lines)


def check_tables(rom):
    """The model's four tables of key codes against the firmware's."""
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'script')
        with open(path, 'w') as f:
            f.write('tables\n')
        out = subprocess.run([MODEL, path], stdout=subprocess.PIPE, universal_newlines=True).stdout.split()
    base = d80emu.KEY_TABLES
    names = ('plain', 'ctrl', 'shift', 'caps')
    for i, name in enumerate(names):
        want = rom[base + 104 * i:base + 104 * (i + 1)].hex()
        got = out[2 * i + 1] if len(out) == 8 and out[2 * i] == name else ''
        if got != want:
            return 'the model\'s table of key codes "%s" is not the firmware\'s' % name
    return None


def main():
    args = sys.argv[1:]
    options = {'rom': None, 'focus': None, 'print': False, 'keys': False, 'events': (200, 3000),
               'keep': None}
    jobs = os.cpu_count() or 1
    plain = []
    while args:
        a = args.pop(0)
        if a == '--rom':
            options['rom'] = args.pop(0)
        elif a == '-j':
            jobs = int(args.pop(0))
        elif a == '--events':
            options['events'] = (int(args.pop(0)), int(args.pop(0)))
        elif a == '--focus':
            options['focus'] = args.pop(0)
            if options['focus'] not in AREAS:
                sys.exit('d80test: --focus is one of ' + ' '.join(AREAS))
        elif a == '--print':
            options['print'] = True
        elif a == '--keys':
            options['keys'] = True
        elif a == '--keep':
            options['keep'] = args.pop(0)
        else:
            plain.append(a)
    if len(plain) != 2:
        sys.exit(__doc__)
    seed, cases = int(plain[0]), int(plain[1])
    options['rom'] = d80emu.rom_path(options['rom'])
    rom = d80emu.load_rom(options['rom'])
    if not os.path.exists(MODEL):
        sys.exit('d80test: %s is not there: cargo build --release -p cray-xmp-ios --examples' % MODEL)
    failed = 0
    hangs = 0
    lost = 0
    problem = check_tables(rom)
    if problem:
        print(problem)
        failed += 1
    work = [(seed, case, options) for case in range(cases)]
    if jobs > 1 and cases > 1:
        with multiprocessing.Pool(jobs) as pool:
            results = pool.imap_unordered(run_case, work, chunksize=1)
            for result in results:
                if result == 'hang':
                    hangs += 1
                elif result == 'lost':
                    lost += 1
                elif result:
                    failed += 1
                    print(result)
                    sys.stdout.flush()
    else:
        for job in work:
            result = run_case(job)
            if result == 'hang':
                hangs += 1
            elif result == 'lost':
                lost += 1
            elif result:
                failed += 1
                print(result)
                sys.stdout.flush()
    if hangs:
        print('%d cases end where the firmware hangs, and the model says so' % hangs)
    if lost:
        print('%d cases are compared only up to where the firmware leaves the memory of its function keys, '
              'and the model says so' % lost)
    print('%d cases, %d failed' % (cases, failed))
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()

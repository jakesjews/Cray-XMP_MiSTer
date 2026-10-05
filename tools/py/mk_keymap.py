#!/usr/bin/env python3
"""Generate rtl/terminal/keymap.mem: PS/2 set 2 make code -> ASCII, US layout.

The table has 512 entries indexed by {shift, scancode[7:0]} for non-extended
codes.  Zero means the key produces no character.  Extended keys (keypad Enter,
keypad slash, Delete) are handled in term_keyboard.v.
"""
import os

UNSHIFTED = {
    0x1C: 'a', 0x32: 'b', 0x21: 'c', 0x23: 'd', 0x24: 'e', 0x2B: 'f', 0x34: 'g',
    0x33: 'h', 0x43: 'i', 0x3B: 'j', 0x42: 'k', 0x4B: 'l', 0x3A: 'm', 0x31: 'n',
    0x44: 'o', 0x4D: 'p', 0x15: 'q', 0x2D: 'r', 0x1B: 's', 0x2C: 't', 0x3C: 'u',
    0x2A: 'v', 0x1D: 'w', 0x22: 'x', 0x35: 'y', 0x1A: 'z',
    0x45: '0', 0x16: '1', 0x1E: '2', 0x26: '3', 0x25: '4', 0x2E: '5', 0x36: '6',
    0x3D: '7', 0x3E: '8', 0x46: '9',
    0x0E: '`', 0x4E: '-', 0x55: '=', 0x5D: '\\', 0x54: '[', 0x5B: ']',
    0x4C: ';', 0x52: "'", 0x41: ',', 0x49: '.', 0x4A: '/', 0x29: ' ',
    0x5A: '\r', 0x66: '\b', 0x0D: '\t', 0x76: '\x1b',
    # keypad (num lock assumed on)
    0x70: '0', 0x69: '1', 0x72: '2', 0x7A: '3', 0x6B: '4', 0x73: '5', 0x74: '6',
    0x6C: '7', 0x75: '8', 0x7D: '9', 0x71: '.', 0x79: '+', 0x7B: '-', 0x7C: '*',
}
SHIFTED = {
    '`': '~', '1': '!', '2': '@', '3': '#', '4': '$', '5': '%', '6': '^', '7': '&',
    '8': '*', '9': '(', '0': ')', '-': '_', '=': '+', '\\': '|', '[': '{', ']': '}',
    ';': ':', "'": '"', ',': '<', '.': '>', '/': '?',
}
KEYPAD = {0x70, 0x69, 0x72, 0x7A, 0x6B, 0x73, 0x74, 0x6C, 0x75, 0x7D, 0x71, 0x79, 0x7B, 0x7C}

table = [0] * 512
for code, ch in UNSHIFTED.items():
    table[code] = ord(ch)
    if code in KEYPAD:
        sh = ch
    elif ch.isalpha():
        sh = ch.upper()
    else:
        sh = SHIFTED.get(ch, ch)
    table[0x100 | code] = ord(sh)

out = os.path.join(os.path.dirname(__file__), '..', '..', 'rtl', 'terminal', 'keymap.mem')
with open(out, 'w') as f:
    for v in table:
        f.write('%02x\n' % v)
print('wrote', os.path.normpath(out), sum(1 for v in table if v), 'mapped entries')

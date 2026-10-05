#!/usr/bin/env python3
"""Turn a Cray memory image into the monitor ROM file.

    mkrom.py IMAGE.img OUT.mem [WORDS]

The image is raw big-endian 64-bit words from word 0.  The output has one word
per line in hex, padded with zero words to WORDS (default 4096).
"""
import sys

def main():
    img, out = sys.argv[1], sys.argv[2]
    words = int(sys.argv[3]) if len(sys.argv) > 3 else 4096
    data = open(img, 'rb').read()
    n = (len(data) + 7) // 8
    if n > words:
        sys.exit('%s has %d words, the ROM holds %d' % (img, n, words))
    data += b'\0' * (words * 8 - len(data))
    with open(out, 'w') as f:
        for i in range(words):
            f.write(data[i * 8:i * 8 + 8].hex() + '\n')
    print('%s: %d of %d words used' % (out, n, words))

main()

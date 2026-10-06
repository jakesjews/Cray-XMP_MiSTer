// The printer's file of the X-MP core (rtl/mister/print_spool.v) against what
// it should hold.
//
//   Vprint_spool [CASES] [--seed N]
//
// Each case mounts a file of a random length of which an earlier session has
// used a random number of blocks, or no file at all, and prints random
// characters with pauses of random length, some longer than the time a part
// full block waits.  Printing begins before the file is mounted: what is
// printed until then is lost, and what comes while it is being mounted waits.  The framework's side answers the way hps_io does, with a
// random delay.  At the end the file must hold the earlier blocks as they
// were, then everything that was printed, without the zeros, as far as the
// file has room, then blanks and a line feed to the end of the block, and
// line feeds after that.  With RW_POISON the block memory gives a wrong byte
// when it is read in the clock it is written in.
#include "Vprint_spool.h"
#include "verilated.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <random>
#include <vector>

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    int cases = 300; unsigned seed = 1;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--seed") && i + 1 < argc) seed = atoi(argv[++i]);
        else cases = atoi(argv[i]);
    }
    std::mt19937 rng(seed);
    auto pick = [&](int a, int b) { return a + (int)(rng() % (unsigned)(b - a + 1)); };
    long written = 0, read = 0;

    for (int n = 0; n < cases; n++) {
        auto *top = new Vprint_spool;
        // the file: `blocks` long, `used` of them printed on before
        int blocks = pick(0, 4) == 0 ? 0 : pick(1, 40), used = blocks ? pick(0, blocks) : 0;
        if (pick(0, 5) == 0) used = 0;
        std::vector<uint8_t> file((size_t)blocks * 512, '\n');
        for (int b = 0; b < used * 512; b++) file[b] = (b % 512 == 511 && pick(0, 3)) ? '\n' : (uint8_t)pick(0x20, 0x7E);
        // a used block may be all line feeds but for one character, anywhere in it
        if (used) { int b = pick(0, used - 1); for (int k = 0; k < 512; k++) file[b * 512 + k] = '\n'; file[b * 512 + pick(0, 511)] = 'x'; }
        std::vector<uint8_t> want = file;

        // what is printed: bursts, with pauses
        size_t total = pick(0, 3) == 0 ? pick(0, 40) : pick(0, 3000);
        std::vector<uint8_t> text(total);
        for (auto &c : text) c = pick(0, 20) == 0 ? 0 : pick(0, 12) == 0 ? '\n' : (uint8_t)pick(0x20, 0x7E);
        size_t sent = 0, lost_before = 0;

        // the framework: one request at a time
        int state = 0, wait = 0, pos = 0; bool writing = false; uint32_t lba = 0; uint8_t data[512];
        bool mounted_yet = false, came_to_rest = false; int mount_at = pick(2, 300), pause = 0, idle = 0;
        top->i_valid = 0; top->i_mounted = 0; top->i_ack = 0; top->i_buff_wr = 0; top->i_buff_addr = 0; top->i_buff_dout = 0; top->i_blocks = 0;
        bool failed = false;
        for (long t = 0; t < 4000000 && !failed; t++) {
            top->clk = 0; top->eval();
            bool rd = top->o_rd, wr = top->o_wr; uint32_t want_lba = top->o_lba; uint8_t din = top->o_buff_din;
            bool taken = top->i_valid && top->o_ready;
            top->clk = 1; top->eval();

            // the printer
            if (taken) { sent++; if (t <= mount_at || !blocks) lost_before = sent; top->i_valid = 0; pause = pick(0, 6) == 0 ? pick(0, 400) : pick(0, 2); }
            if (!top->i_valid && sent < text.size()) {
                if (pause > 0) pause--; else { top->i_char = text[sent]; top->i_valid = 1; }
            }
            // the file is mounted once, a little after the start; with no file nothing is
            top->i_mounted = 0;
            if (!mounted_yet && t >= mount_at) {
                if (blocks && t < mount_at + 5) { top->i_mounted = 1; top->i_blocks = blocks; }
                else mounted_yet = true;
            }

            // the framework
            top->i_buff_wr = 0;
            switch (state) {
            case 0:
                if (rd || wr) { writing = wr; lba = want_lba; state = 1; wait = pick(1, 40); if (lba >= (uint32_t)blocks) { printf("case %d: block %u is asked for of a file of %d\n", n, lba, blocks); failed = true; } }
                break;
            case 1:
                if (--wait > 0) break;
                top->i_ack = 1; pos = 0; top->i_buff_addr = 0; state = 2; wait = 3;
                if (!writing) memcpy(data, &file[(size_t)lba * 512], 512);
                break;
            case 2:
                if (--wait > 0) break;
                wait = 3;
                if (writing) {
                    if (pos == 512) { memcpy(&file[(size_t)lba * 512], data, 512); written++; state = 3; break; }
                    data[pos++] = din; top->i_buff_addr = pos & 511;
                } else {
                    if (pos == 512) { read++; state = 3; break; }
                    top->i_buff_addr = pos; top->i_buff_dout = data[pos]; top->i_buff_wr = 1; pos++;
                }
                break;
            case 3: top->i_ack = 0; state = 0; break;
            }
            top->eval();

            // over when everything is printed and nothing has moved for a while
            bool busy = top->i_valid || rd || wr || state != 0 || !top->o_ready;
            idle = (sent == text.size() && mounted_yet && !busy) ? idle + 1 : 0;
            if (idle > 400) { came_to_rest = true; break; }
        }
        if (!failed && !came_to_rest) { printf("case %d: the file never comes to rest\n", n); failed = true; }
        // what the file should hold: the characters taken once it was mounted
        size_t at = (size_t)used * 512;
        for (size_t k = lost_before; k < text.size(); k++) if (text[k] && at < want.size()) want[at++] = text[k];
        if (at % 512 != 0 && at > (size_t)used * 512) { while (at % 512 != 511) want[at++] = ' '; want[at++] = '\n'; }
        if (!failed && sent != text.size()) { printf("case %d: %zu of %zu characters were taken\n", n, sent, text.size()); failed = true; }
        if (!failed && file != want) {
            size_t d = 0; while (file[d] == want[d]) d++;
            printf("case %d (%d blocks, %d used, %zu characters): byte %zu of the file is %02x, not %02x\n", n, blocks, used, text.size(), d, file[d], want[d]);
            failed = true;
        }
        delete top;
        if (failed) return 1;
    }
    printf("%d files are as they should be (%ld blocks written, %ld read)\n", cases, written, read);
    return 0;
}

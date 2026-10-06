// The Ampex terminal of the X-MP core against the model's: character streams
// and the screens they make come from `screen_random` (tools/crates/ios).
//
//   Vterm_ampex_tb FILE [--seed N]
//
// Each stream is sent after a reset, with idle clocks at random between the
// characters, and the screen memory and the cursor are compared at its end.
#include "Vterm_ampex_tb.h"
#include "Vterm_ampex_tb___024root.h"
#include "verilated.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <random>
#include <vector>

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    const char *file = nullptr;
    unsigned seed = 1;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--seed") && i + 1 < argc) seed = atoi(argv[++i]);
        else file = argv[i];
    }
    FILE *f = file ? fopen(file, "rb") : nullptr;
    if (!f) { fprintf(stderr, "usage: Vterm_ampex_tb FILE [--seed N]\n"); return 2; }

    auto *top = new Vterm_ampex_tb;
    std::mt19937 rng(seed);
    auto clock = [&] { top->clk = 0; top->eval(); top->clk = 1; top->eval(); };

    int cases = 0;
    for (;; cases++) {
        uint8_t head[4];
        if (fread(head, 1, 4, f) != 4) break;
        size_t length = head[0] | head[1] << 8 | head[2] << 16 | (size_t)head[3] << 24;
        std::vector<uint8_t> bytes(length), want(24 * 80 + 2);
        if (fread(bytes.data(), 1, length, f) != length || fread(want.data(), 1, want.size(), f) != want.size()) {
            fprintf(stderr, "case %d: the file ends early\n", cases);
            return 2;
        }

        top->reset = 1; top->din_valid = 0;
        for (int n = 0; n < 4; n++) clock();
        top->reset = 0;
        int busy = rng() % 3;   // how often the sender pauses
        for (size_t n = 0; n < length;) {
            top->din = bytes[n] & 0x7F;
            top->din_valid = busy == 0 || rng() % (busy * 3) != 0;
            top->eval();
            bool taken = top->din_valid && top->din_ready;
            clock();
            if (taken) n++;
        }
        top->din_valid = 0;
        // a deleted line and a cleared screen take a few thousand clocks
        for (int n = 0; n < 8000; n++) clock();
        if (!top->din_ready) { fprintf(stderr, "case %d: the terminal is still busy\n", cases); return 1; }

        int top_row = top->top_row;
        for (int y = 0; y < 24; y++)
            for (int x = 0; x < 80; x++) {
                uint8_t got = top->rootp->term_ampex_tb__DOT__screen[(y + top_row) % 24 * 80 + x];
                if (got != want[y * 80 + x]) {
                    fprintf(stderr, "case %d (%zu characters): line %d column %d is %02x, the model has %02x\n",
                            cases, length, y, x, got, want[y * 80 + x]);
                    return 1;
                }
            }
        if (top->cur_y != want[1920] || top->cur_x != want[1921]) {
            fprintf(stderr, "case %d: the cursor is at %d,%d, the model's at %d,%d\n",
                    cases, top->cur_y, top->cur_x, want[1920], want[1921]);
            return 1;
        }
    }
    fclose(f);
    printf("%d screens equal the model's\n", cases);
    delete top;
    return cases ? 0 : 2;
}

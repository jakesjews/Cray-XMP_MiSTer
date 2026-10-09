// The Ampex terminal of the X-MP core against the model's: characters, keys
// and the states the model's terminal is in behind them come from
// `screen_random` (tools/crates/ios), which says what its file holds.
//
//   Vterm_ampex_tb FILE [--seed N] [--verbose] [--times]
//
// Each case is run on a terminal that has just been reset.  A character or a
// key is offered when the terminal has taken the one before, at once or some
// clocks later, and is held until it is taken; what the terminal sends is
// taken at once in some cases and with pauses in others.  At each state in
// the file the terminal is let come to rest and everything is compared: every
// cell of every page and of the status line, the last row of each page and
// the first line the display is told, the page shown, the cursor, program
// mode, the characters sent since the state before, and the bells rung.  All
// the time the bell output has to sound for eight frames from each bell.
//
// A case that differs is reported with the first thing that is different, and
// the run goes on with the next.  The last line is `N cases, M failed`.
//
// With --times nothing pauses, and for each case the event the terminal was
// busy with for longest is printed, with the clocks from the one in which it
// was taken to the one in which the terminal takes the next.
#include "Vterm_ampex_tb.h"
#include "Vterm_ampex_tb___024root.h"
#include "verilated.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <random>
#include <string>
#include <vector>

namespace {

struct File {
    FILE *f;
    bool ended = false;
    int byte() {
        int c = fgetc(f);
        if (c == EOF) ended = true;
        return c;
    }
    unsigned word() { unsigned low = byte() & 0xFF; return low | (byte() & 0xFF) << 8; }
};

const int ROWS = 24, COLUMNS = 80;

}  // namespace

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    const char *file = nullptr;
    unsigned seed = 1;
    bool verbose = false, times = false;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--seed") && i + 1 < argc) seed = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--verbose")) verbose = true;
        else if (!strcmp(argv[i], "--times")) times = true;
        else file = argv[i];
    }
    File in{file ? fopen(file, "rb") : nullptr};
    if (!in.f) { fprintf(stderr, "usage: Vterm_ampex_tb FILE [--seed N] [--verbose] [--times]\n"); return 2; }

    auto *top = new Vterm_ampex_tb;
    auto *r = top->rootp;
    std::mt19937 rng(seed);

    int cases = 0, failed = 0;
    unsigned long long clocks = 0, longest = 0;
    for (;;) {
        int start = in.byte();
        if (in.ended) break;
        if (start != 0xC5) { fprintf(stderr, "the file is not one of screen_random's\n"); return 2; }
        int pages = in.byte();
        bool one = pages == 1;
        unsigned number = in.word();
        number |= in.word() << 16;

        // what this case is like: how often the sender pauses, how often the
        // receiver does, how often a frame of the display ends
        int gaps = rng() % 4, taking = rng() % 4, frame_every = 2 + rng() % 200;
        if (times) gaps = taking = 0;
        unsigned long long slowest = 0;
        int slowest_event = 0, slowest_character = -1, slowest_key = -1;
        std::vector<uint8_t> sent;
        std::vector<std::vector<uint16_t>> want(1 + pages * ROWS, std::vector<uint16_t>(COLUMNS, 0));
        int bell_left = 0;
        std::string problem;
        int events = 0;

        auto clock = [&] {
            top->dout_ready = taking == 0 || (taking == 1 ? rng() % 2 : taking == 2 ? rng() % 8 == 0 : rng() % 64 != 0);
            top->frame = rng() % frame_every == 0;
            top->clk = 0;
            top->eval();
            if (top->dout_valid && top->dout_ready) sent.push_back(top->dout);
            uint32_t bells = top->bells;
            bool frame = top->frame;
            top->clk = 1;
            top->eval();
            clocks++;
            if (top->bells != bells) bell_left = 8;
            else if (frame && bell_left) bell_left--;
            if (problem.empty() && (top->bell != 0) != (bell_left != 0))
                problem = "the bell output is " + std::to_string(top->bell) + " with " + std::to_string(bell_left) + " frames of a bell left";
        };
        // let the terminal come to rest
        auto rest = [&] {
            unsigned long long from = clocks;
            while (!top->din_ready || !top->key_ready) {
                clock();
                if (clocks - from > 40000000) { if (problem.empty()) problem = "the terminal does not come to rest"; break; }
            }
            if (clocks - from > longest) longest = clocks - from;
        };
        // offer a character, a key or both until taken
        auto offer = [&](int character, int key) {
            int idle = 0;
            switch (gaps) {
                case 0: break;
                case 1: idle = rng() % 4 == 0 ? rng() % 4 : 0; break;
                case 2: idle = rng() % 3 == 0 ? rng() % 30 : 0; break;
                default: idle = rng() % 50 == 0 ? rng() % 3000 : rng() % 3; break;
            }
            top->din_valid = 0;
            top->key_valid = 0;
            for (int n = 0; n < idle; n++) clock();
            top->din = character < 0 ? rng() & 0x7F : character;
            top->key = key < 0 ? rng() & 0xFF : key;
            top->din_valid = character >= 0;
            top->key_valid = key >= 0;
            unsigned long long from = clocks;
            for (;;) {
                top->clk = 0;
                top->eval();
                bool taken = (character < 0 || top->din_ready) && (key < 0 || top->key_ready);
                clock();
                if (taken) break;
                if (clocks - from > 40000000) { if (problem.empty()) problem = "the terminal does not take what it is offered"; break; }
            }
            if (clocks - from > longest) longest = clocks - from;
            top->din_valid = 0;
            top->key_valid = 0;
            events++;
            if (times) {
                from = clocks;
                rest();
                if (clocks - from + 1 > slowest) {
                    slowest = clocks - from + 1;
                    slowest_event = events;
                    slowest_character = character;
                    slowest_key = key;
                }
            }
        };

        top->one = one;
        top->reset = 1;
        top->din_valid = 0;
        top->key_valid = 0;
        for (int n = 0; n < 4; n++) clock();
        top->reset = 0;
        bell_left = 0;
        problem.clear();
        clock();

        int states = 0;
        for (;;) {
            int kind = in.byte();
            if (in.ended) { fprintf(stderr, "case %u: the file ends early\n", number); return 2; }
            if (kind == 0x45) break;
            if (kind == 0x68) { int c = in.byte(); if (problem.empty()) offer(c, -1); continue; }
            if (kind == 0x6B) { int k = in.byte(); if (problem.empty()) offer(-1, k); continue; }
            if (kind == 0x62) { int c = in.byte(); int k = in.byte(); if (problem.empty()) offer(c, k); continue; }
            if (kind != 0x53) { fprintf(stderr, "case %u: the file is not one of screen_random's\n", number); return 2; }

            // a state
            int head[8];
            for (int &h : head) h = in.byte();
            unsigned bells = in.word();
            bells |= in.word() << 16;
            unsigned length = in.word();
            std::vector<uint8_t> sent_want(length);
            for (auto &b : sent_want) b = in.byte();
            for (auto &row : want) {
                int how = in.byte();
                if (how == 1) { unsigned cell = in.word(); for (auto &c : row) c = cell; }
                else if (how == 2) for (auto &c : row) c = in.word();
            }
            states++;
            if (!problem.empty()) continue;
            rest();
            for (int n = 0; n < 2; n++) clock();
            if (!problem.empty()) continue;

            char text[200];
            auto differ = [&](const char *what, int got, int expect) {
                if (problem.empty() && got != expect) {
                    snprintf(text, sizeof text, "%s is %d, the model has %d", what, got, expect);
                    problem = text;
                }
            };
            differ("the page shown", top->page, head[0]);
            differ("the last row of the first page", top->bottom0, head[1]);
            if (!one) differ("the last row of the second page", top->bottom1, head[2]);
            differ("the first line's row", top->top_row, ((one || !head[0] ? head[1] : head[2]) + 1) % ROWS);
            differ("the cursor's row", top->cur_y, head[3]);
            differ("the cursor's column", top->cur_x, head[4]);
            differ("the cursor in the status line", top->cur_status, head[5]);
            differ("program mode", top->prog_mode, head[6]);
            differ("the number of bells", top->bells, bells);
            if (problem.empty() && sent != sent_want) {
                size_t n = 0;
                while (n < sent.size() && n < sent_want.size() && sent[n] == sent_want[n]) n++;
                snprintf(text, sizeof text, "%zu characters were sent, the model sends %zu; the first that differs is number %zu: %02x, the model's %02x",
                         sent.size(), sent_want.size(), n, n < sent.size() ? sent[n] : 0x100, n < sent_want.size() ? sent_want[n] : 0x100);
                problem = text;
            }
            sent.clear();
            for (int n = 0; n < 1 + pages * ROWS && problem.empty(); n++)
                for (int x = 0; x < COLUMNS; x++) {
                    // the status line is behind the rows of the first page
                    int at = n == 0 ? 1920 + x : ((n - 1) / ROWS) * 2048 + ((n - 1) % ROWS) * 80 + x;
                    unsigned got = one ? r->term_ampex_tb__DOT__attrs1[at] << 8 | r->term_ampex_tb__DOT__chars1[at]
                                       : r->term_ampex_tb__DOT__attrs2[at] << 8 | r->term_ampex_tb__DOT__chars2[at];
                    if (got != want[n][x]) {
                        if (n == 0) snprintf(text, sizeof text, "the status line's column %d is %03x, the model has %03x", x, got, want[n][x]);
                        else snprintf(text, sizeof text, "page %d memory row %d column %d is %03x, the model has %03x (attributes, then protect bit and character)",
                                      (n - 1) / ROWS, (n - 1) % ROWS, x, got, want[n][x]);
                        problem = text;
                        break;
                    }
                }
            if (!problem.empty()) {
                snprintf(text, sizeof text, " (at state %d, behind %d events%s)", states - 1, events, head[7] ? ", the model has hung" : "");
                problem += text;
            }
        }
        if (!problem.empty()) {
            failed++;
            printf("case %u (%d pages): %s\n", number, pages, problem.c_str());
            fflush(stdout);
        } else if (verbose) printf("case %u (%d pages): %d events, %d states\n", number, pages, events, states);
        if (times) {
            printf("case %u: event %d of %d took %llu clocks:", number, slowest_event, events, slowest);
            if (slowest_character >= 0) printf(" character %02x", slowest_character);
            if (slowest_key >= 0) printf(" key %02x", slowest_key);
            printf("\n");
        }
        cases++;
    }
    fclose(in.f);
    printf("the longest wait for the terminal was %llu clocks\n%d cases, %d failed\n", longest, cases, failed);
    delete top;
    return cases && !failed ? 0 : cases ? 1 : 2;
}

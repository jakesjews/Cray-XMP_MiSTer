// Behavioural main memory for CPU-level simulation, speaking the CPU's
// port (rtl/cray/cray_cpu.v): a request (req, we, len, addr, wdata) is taken
// in a clock where take is high; reads of len words are answered in order,
// one word an ack; a write is done with its take.
// Latency and the clocks in which nothing is taken are configurable so no CPU
// path can come to depend on them.
//
// The top 16 of the four million words are the test bench's I/O page:
//   0x3FFFF0 CON_STAT  read: bit 0 input available, bit 1 output ready, bit 2
//                      console interrupt enabled, bit 3 requested; write: bit 0
//                      enables the console interrupt and clears a request
//   0x3FFFF1 CON_DATA  read: next input character; write: print character
//   0x3FFFF2 TEST_EXIT write: end of test, value is the exit code
//   0x3FFFF3 CYCLES    read: clock counter
#pragma once
#include <cstdint>
#include <cstdio>
#include <deque>
#include <random>
#include <set>
#include <string>
#include <vector>

struct MemProfile {
    std::string name = "fixed";
    int lat_min = 1, lat_max = 1;       // clocks from taking a read to its first word
    int busy_pct = 0;                   // chance per clock that nothing is taken
    int gap_pct = 0;                    // chance of an idle clock between the words of a read
    int stall_x1000 = 0;                // chance (per 100000) of a long stall on a read
    int stall_min = 30, stall_max = 100;
    static MemProfile parse(const std::string &s) {
        MemProfile p; p.name = s;
        if (s == "0" || s == "fast")      { p.lat_min = p.lat_max = 1; }
        // as the DE10-Nano's DDR3 behaves through rtl/mister/ddr3_ports.sv: a word about 16
        // clocks after its read was taken (tools/py/membench.py against the times measured
        // under COS), now and then a clock in which the bridge is busy
        else if (s == "mister")           { p.lat_min = 15; p.lat_max = 17; p.busy_pct = 3; p.stall_x1000 = 100; }
        else if (s == "ddr3")             { p.lat_min = 6; p.lat_max = 12; p.busy_pct = 10; p.gap_pct = 5; p.stall_x1000 = 300; }
        else if (s == "slow")             { p.lat_min = 10; p.lat_max = 40; p.busy_pct = 40; p.gap_pct = 30; p.stall_x1000 = 2000; }
        else if (s.rfind("fixed:", 0) == 0) { p.lat_min = p.lat_max = std::max(1, atoi(s.c_str() + 6)); }
        else if (s.rfind("rand:", 0) == 0)  { sscanf(s.c_str() + 5, "%d-%d", &p.lat_min, &p.lat_max); if (p.lat_min < 1) p.lat_min = 1; if (p.lat_max < p.lat_min) p.lat_max = p.lat_min; p.busy_pct = 20; }
        return p;
    }
};

class CrayMemModel {
public:
    static constexpr uint32_t WORDS = 1u << 22;
    static constexpr uint32_t IO_BASE = WORDS - 16;
    std::vector<uint64_t> mem;
    MemProfile prof;
    std::string console_out;
    std::deque<uint8_t> console_in;
    bool echo_console = true;
    bool exited = false;
    uint64_t exit_code = 0;
    bool con_int_en = false, con_int_req = false;   // the console (MCU) interrupt, of the test bench's I/O page
    void ctrl_c() { if (con_int_en) con_int_req = true; }
    uint64_t cycle = 0, reads = 0, writes = 0;
    bool log = false;
    bool bad = false;
    std::set<uint32_t> written;         // every memory word the program stored into

    explicit CrayMemModel(uint32_t seed = 1) : mem(WORDS), rng(seed) {
        for (auto &w : mem) w = ((uint64_t)rng() << 32) | rng();     // DDR3 is not cleared
    }

    bool load(const std::string &path, uint32_t word = 0) {
        FILE *f = fopen(path.c_str(), "rb");
        if (!f) return false;
        uint8_t b[8]; size_t n = 0;
        while (fread(b, 1, 8, f) == 8 && word + n < WORDS) {
            uint64_t v = 0; for (int i = 0; i < 8; i++) v = (v << 8) | b[i];
            mem[word + n++] = v;
        }
        fclose(f);
        image_words = (uint32_t)n;
        return true;
    }
    uint32_t image_words = 0;

    // Outputs for the next clock: whether a request will be taken in it, and
    // a word read
    bool take = true;
    bool ack = false;
    uint64_t rdata = 0;

    // Called once per clock with what the CPU drove during the clock that
    // ended, which was taken if take was high during it.  A read's words are
    // queued, each with the clock it comes in; the first comes after the
    // profile's latency, the rest one a clock with the profile's gaps, and
    // never before the last word of the read before.
    void step(bool req, bool we, unsigned len, uint32_t addr, uint64_t wdata) {
        cycle++;
        if (req && take) {
            if (we) write(addr & 0x3FFFFF, wdata);
            else {
                if (len < 1 || len > 16) { fprintf(stderr, "mem: a read of %u words (cycle %llu)\n", len, (unsigned long long)cycle); bad = true; }
                if (((addr + len - 1) >> 4) != (addr >> 4)) { fprintf(stderr, "mem: a read across a line at %o (cycle %llu)\n", addr, (unsigned long long)cycle); bad = true; }
                uint64_t when = cycle + range(prof.lat_min, prof.lat_max);
                if (prof.stall_x1000 && (int)(rng() % 100000) < prof.stall_x1000) when += range(prof.stall_min, prof.stall_max);
                if (when <= last_word) when = last_word + 1;
                for (unsigned b = 0; b < len; b++) {
                    if (b && prof.gap_pct && (int)(rng() % 100) < prof.gap_pct) when += range(1, 3);
                    pending.push_back({when, (addr + b) & 0x3FFFFF});
                    last_word = when;
                    when++;
                }
            }
        }
        ack = false;
        if (!pending.empty() && pending.front().when <= cycle) {
            rdata = read(pending.front().addr);
            pending.pop_front();
            ack = true;
        }
        take = !(prof.busy_pct && (int)(rng() % 100) < prof.busy_pct);
    }

    // End state in the same text format as the reference model (cray-xmp-run --state):
    //   exit <code or none>
    //   console <hex bytes>
    //   mem <octal address> <16 hex digits>     one line per word stored, ascending
    bool write_state(const std::string &path) const {
        FILE *f = fopen(path.c_str(), "w");
        if (!f) return false;
        if (exited) fprintf(f, "exit %llu\n", (unsigned long long)exit_code); else fprintf(f, "exit none\n");
        fprintf(f, "console ");
        for (unsigned char c : console_out) fprintf(f, "%02x", c);
        fprintf(f, "\n");
        for (uint32_t a : written) fprintf(f, "mem %o %016llx\n", a, (unsigned long long)mem[a]);
        fclose(f);
        return true;
    }

private:
    struct Word { uint64_t when; uint32_t addr; };
    std::deque<Word> pending;
    uint64_t last_word = 0;
    std::mt19937 rng;
    int range(int a, int b) { return a + (int)(rng() % (uint32_t)(b - a + 1)); }

    uint64_t read(uint32_t a) {
        reads++;
        uint64_t v;
        if (a >= IO_BASE && a < WORDS) {
            switch (a - IO_BASE) {
                case 0: v = (console_in.empty() ? 0 : 1) | 2 | (con_int_en ? 4 : 0) | (con_int_req ? 8 : 0); break;
                case 1: if (console_in.empty()) v = 0; else { v = console_in.front(); console_in.pop_front(); } break;
                case 3: v = cycle; break;
                default: v = 0;
            }
        } else if (a < WORDS) v = mem[a];
        else { v = 0; fprintf(stderr, "mem: read beyond memory at %o (cycle %llu)\n", a, (unsigned long long)cycle); bad = true; }
        if (log) fprintf(stderr, "[%8llu] RD %07o -> %022llo\n", (unsigned long long)cycle, a, (unsigned long long)v);
        return v;
    }
    void write(uint32_t a, uint64_t v) {
        writes++;
        if (log) fprintf(stderr, "[%8llu] WR %07o <- %022llo\n", (unsigned long long)cycle, a, (unsigned long long)v);
        if (a >= IO_BASE && a < WORDS) {
            switch (a - IO_BASE) {
                case 0: con_int_en = v & 1; con_int_req = false; break;
                case 1: console_out.push_back((char)v); if (echo_console) { fputc((int)(v & 0xFF), stdout); fflush(stdout); } break;
                case 2: exited = true; exit_code = v; break;
                default: break;
            }
        } else if (a < WORDS) { mem[a] = v; written.insert(a); }
        else { fprintf(stderr, "mem: write beyond memory at %o (cycle %llu)\n", a, (unsigned long long)cycle); bad = true; }
    }
};

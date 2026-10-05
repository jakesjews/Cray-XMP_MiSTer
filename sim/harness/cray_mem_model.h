// Behavioural main memory for CPU-level simulation, speaking the CPU's
// request/acknowledge port:
//   req, we, burst, addr, wdata held until the last ack; ack is one clock per
//   word, never in the clock req first rises; burst = 16-word read.
// Latency is configurable so no CPU path can come to depend on it.
//
// The top 16 words of the 1M-word space are the core's invented I/O page:
//   0xFFFF0 CON_STAT  read: bit 0 input available, bit 1 output ready
//   0xFFFF1 CON_DATA  read: next input character; write: print character
//   0xFFFF2 TEST_EXIT write: end of test, value is the exit code
//   0xFFFF3 CYCLES    read: clock counter
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
    int lat_min = 1, lat_max = 1;       // clocks from accepting a request to its first ack
    int gap_pct = 0;                    // chance of an idle clock between burst words
    int stall_x1000 = 0;                // chance (per 100000) of a long stall on a request
    int stall_min = 30, stall_max = 100;
    static MemProfile parse(const std::string &s) {
        MemProfile p; p.name = s;
        if (s == "0" || s == "fast")      { p.lat_min = p.lat_max = 1; }
        else if (s == "ddr3")             { p.lat_min = 6; p.lat_max = 12; p.gap_pct = 5; p.stall_x1000 = 300; }
        else if (s == "slow")             { p.lat_min = 10; p.lat_max = 40; p.gap_pct = 30; p.stall_x1000 = 2000; }
        else if (s.rfind("fixed:", 0) == 0) { p.lat_min = p.lat_max = std::max(1, atoi(s.c_str() + 6)); }
        else if (s.rfind("rand:", 0) == 0)  { sscanf(s.c_str() + 5, "%d-%d", &p.lat_min, &p.lat_max); if (p.lat_min < 1) p.lat_min = 1; if (p.lat_max < p.lat_min) p.lat_max = p.lat_min; }
        return p;
    }
};

class CrayMemModel {
public:
    static constexpr uint32_t WORDS = 1u << 20;
    static constexpr uint32_t IO_BASE = 0xFFFF0;
    std::vector<uint64_t> mem;
    MemProfile prof;
    std::string console_out;
    std::deque<uint8_t> console_in;
    bool echo_console = true;
    bool exited = false;
    uint64_t exit_code = 0;
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

    // Outputs for the next clock
    bool ack = false;
    uint64_t rdata = 0;

    // Called once per clock with what the CPU drove during the clock that ended.
    void step(bool req, bool we, bool burst, uint32_t addr, uint64_t wdata) {
        cycle++;
        bool was_ack = ack;
        ack = false;
        if (busy) {
            if (!req) { fprintf(stderr, "mem: request withdrawn before its last ack (cycle %llu)\n", (unsigned long long)cycle); bad = true; busy = false; return; }
            if (addr != cur_addr || we != cur_we) { fprintf(stderr, "mem: request changed while pending (cycle %llu)\n", (unsigned long long)cycle); bad = true; }
            if (--wait <= 0) {
                uint32_t a = (cur_addr + done) & 0x3FFFFF;
                if (cur_we) write(a, wdata); else rdata = read(a);
                ack = true;
                done++;
                if (done == total) busy = false;
                else wait = 1 + ((prof.gap_pct && (int)(rng() % 100) < prof.gap_pct) ? range(1, 3) : 0);
            }
        } else if (req && !was_ack) {
            busy = true; cur_addr = addr; cur_we = we; done = 0;
            total = (burst && !we) ? 16 : 1;
            wait = range(prof.lat_min, prof.lat_max);
            if (prof.stall_x1000 && (int)(rng() % 100000) < prof.stall_x1000) wait += range(prof.stall_min, prof.stall_max);
        }
    }

    // End state in the same text format as the reference model (cray1-run --state):
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
    bool busy = false, cur_we = false;
    uint32_t cur_addr = 0;
    int wait = 0, done = 0, total = 1;
    std::mt19937 rng;
    int range(int a, int b) { return a + (int)(rng() % (uint32_t)(b - a + 1)); }

    uint64_t read(uint32_t a) {
        reads++;
        uint64_t v;
        if (a >= IO_BASE && a < WORDS) {
            switch (a - IO_BASE) {
                case 0: v = (console_in.empty() ? 0 : 1) | 2; break;
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
                case 1: console_out.push_back((char)v); if (echo_console) { fputc((int)(v & 0xFF), stdout); fflush(stdout); } break;
                case 2: exited = true; exit_code = v; break;
                default: break;
            }
        } else if (a < WORDS) { mem[a] = v; written.insert(a); }
        else { fprintf(stderr, "mem: write beyond memory at %o (cycle %llu)\n", a, (unsigned long long)cycle); bad = true; }
    }
};

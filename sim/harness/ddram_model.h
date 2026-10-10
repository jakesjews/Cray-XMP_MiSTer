// Behavioural model of the MiSTer DDRAM port (Avalon-MM style: BUSY is
// waitrequest, DOUT_READY is readdatavalid).  Memory is a flat byte image of the
// physical range starting at 0x3000_0000, exactly as Linux and the menu see it.
#pragma once
#include <cstdint>
#include <cstdio>
#include <deque>
#include <random>
#include <string>
#include <vector>

struct DdramProfile {
    int  lat_min = 14, lat_max = 16;  // clocks from accepting a read to the first word (a DE10-Nano: about 15 at 105 MHz)
    int  busy_pct = 5;                // chance per clock that the port is busy
    int  gap_pct = 0;                 // chance of an idle clock between burst words
    int  stall_pct_x1000 = 0;         // chance per command of a long stall
    int  stall_min = 30, stall_max = 100;
};

class DdramModel {
public:
    static constexpr uint32_t BASE = 0x30000000u;
    std::vector<uint8_t> mem;
    DdramProfile prof;
    uint64_t reads = 0, writes = 0;

    explicit DdramModel(size_t bytes = 32u << 20, uint32_t seed = 1) : mem(bytes, 0), rng(seed) {
        // DDR3 is not cleared when a core loads; start with junk to catch reliance on zeros
        for (auto &b : mem) b = (uint8_t)rng();
    }

    bool load(const std::string &path, uint32_t offset = 0) {
        FILE *f = fopen(path.c_str(), "rb");
        if (!f) return false;
        size_t n = fread(mem.data() + offset, 1, mem.size() - offset, f);
        fclose(f);
        fprintf(stderr, "ddram: loaded %zu bytes from %s\n", n, path.c_str());
        return true;
    }

    // One Cray word, big-endian in the byte image
    uint64_t word(uint32_t w) const {
        uint64_t v = 0;
        for (int i = 0; i < 8; i++) v = (v << 8) | mem[(size_t)w * 8 + i];
        return v;
    }
    void set_word(uint32_t w, uint64_t v) {
        for (int i = 0; i < 8; i++) mem[(size_t)w * 8 + i] = (uint8_t)(v >> (56 - 8 * i));
    }

    // Outputs of the model for the next clock
    bool     busy = false;
    bool     dout_ready = false;
    uint64_t dout = 0;

    // Call once per clock with the values the core drove during the clock that
    // just ended and the BUSY it saw.  Updates busy/dout/dout_ready for the next.
    void step(bool rd, bool we, uint32_t addr29, uint8_t burstcnt, uint64_t din, uint8_t be) {
        bool accepted = (rd || we) && !busy;
        if (accepted) {
            uint64_t byte_addr = (uint64_t)addr29 << 3;
            if (byte_addr < BASE || byte_addr - BASE + 8ull * (burstcnt ? burstcnt : 1) > mem.size()) {
                fprintf(stderr, "ddram: access outside the model at 0x%llx\n", (unsigned long long)byte_addr);
                bad_access = true;
            } else {
                size_t off = (size_t)(byte_addr - BASE);
                if (we) {
                    for (int i = 0; i < 8; i++)
                        if (be & (1 << i)) mem[off + i] = (uint8_t)(din >> (8 * i));
                    writes++;
                } else {
                    int lat = range(prof.lat_min, prof.lat_max);
                    if (prof.stall_pct_x1000 && (int)(rng() % 100000) < prof.stall_pct_x1000)
                        lat += range(prof.stall_min, prof.stall_max);
                    // the words are read now, as the bridge serves its requests in
                    // order: a write taken after this read does not reach them
                    uint64_t when = std::max(now + lat, last_data + 1);
                    for (int b = 0; b < burstcnt; b++) {
                        if (b && prof.gap_pct && (int)(rng() % 100) < prof.gap_pct) when += range(1, 3);
                        uint64_t d = 0;
                        for (int i = 7; i >= 0; i--) d = (d << 8) | mem[off + 8u * b + i];   // little-endian bus
                        pending.push_back({when, d});
                        last_data = when;
                        when++;
                    }
                    reads++;
                }
            }
        }
        now++;
        dout_ready = false;
        if (!pending.empty() && pending.front().when <= now) {
            dout = pending.front().data;
            pending.pop_front();
            dout_ready = true;
        }
        busy = prof.busy_pct && (int)(rng() % 100) < prof.busy_pct;
    }

    bool bad_access = false;

private:
    struct Beat { uint64_t when; uint64_t data; };
    std::deque<Beat> pending;
    std::mt19937 rng;
    uint64_t now = 0, last_data = 0;
    int range(int a, int b) { return a + (int)(rng() % (uint32_t)(b - a + 1)); }
};

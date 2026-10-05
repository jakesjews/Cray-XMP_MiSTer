// What the test benches of the I/O Subsystem serve the hardware from: files
// read whole, and disk images served a sector at a time the way the MiSTer
// framework serves them.
#pragma once
#include <cstdint>
#include <cstdio>
#include <map>
#include <string>
#include <vector>

static std::vector<uint8_t> read_file(const std::string &path) {
    std::vector<uint8_t> out;
    FILE *f = fopen(path.c_str(), "rb");
    if (!f) return out;
    uint8_t b[65536]; size_t n;
    while ((n = fread(b, 1, sizeof b, f)) > 0) out.insert(out.end(), b, b + n);
    fclose(f);
    return out;
}

// The disk of the Peripheral Expander as the MiSTer framework serves it: a
// request for a sector of 512 bytes is acknowledged, the bytes go one at a
// time into (or come out of) the core's buffer, and the acknowledge drops.
// It serves several drives that share the buffer signals; a request moves
// `bytes` bytes (512, or 4,096 as eight blocks) from block lba of one drive.
struct SectorDisks {
    int bytes;
    std::vector<FILE *> file;
    std::vector<std::map<uint32_t, std::vector<uint8_t>>> written;
    int state = 0, wait = 0, at = 0, drive = 0; bool writing = false; uint32_t lba = 0;
    std::vector<uint8_t> sector;
    long reads = 0, writes = 0;
    // what the core is given: the acknowledge of each drive, and the buffer signals
    unsigned ack = 0, buff_addr = 0, buff_dout = 0; bool buff_wr = false;
    SectorDisks(int drives, int bytes) : bytes(bytes), file(drives, nullptr), written(drives), sector(bytes) {}
    void load() {
        std::fill(sector.begin(), sector.end(), 0);
        auto w = written[drive].find(lba);
        if (w != written[drive].end()) sector = w->second;
        else if (file[drive] && fseek(file[drive], (long)lba * 512, SEEK_SET) == 0) { size_t n = fread(sector.data(), 1, bytes, file[drive]); (void)n; }
    }
    // one clock: the outputs of the core before the edge
    void clock(unsigned rd, unsigned wr, uint32_t want, uint8_t din) {
        buff_wr = false;
        switch (state) {
        case 0:
            for (size_t n = 0; n < file.size(); n++)
                if ((rd | wr) >> n & 1) { drive = n; writing = wr >> n & 1; lba = want; state = 1; wait = 3; break; }
            break;
        case 1: if (--wait == 0) { ack = 1u << drive; at = 0; state = 2; wait = 4; if (!writing) load(); } break;
        case 2:
            if (--wait > 0) break;
            wait = 4;
            if (writing) {
                // the byte at the address given four clocks ago
                if (at > 0) sector[at - 1] = din;
                if (at == bytes) { written[drive][lba] = sector; writes++; state = 3; break; }
                buff_addr = at++;
            } else {
                if (at == bytes) { reads++; state = 3; break; }
                buff_addr = at; buff_dout = sector[at]; buff_wr = true; at++;
            }
            break;
        case 3: ack = 0; state = 0; break;
        }
    }
};

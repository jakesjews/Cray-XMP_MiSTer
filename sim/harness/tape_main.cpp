// The tape drive of the Peripheral Expander with the reel on it
// (sim/tb/ios_tape_tb.v) against what a tape does.
//
//   Vios_tape_tb [CASES] [--seed N]
//
// A case puts a tape on the drive: a tape file with records and file marks
// on it or a blank one, with or without a write ring, or the boot tape.
// Then commands follow at random: read, space forward and backward, write,
// write a file mark, rewind, and one the drive does not have.  Behind each
// the status, the address and the count the drive gives back and what it
// stored in Local Memory have to be those of a tape that is kept here, and
// at the end of the case the file has to hold that tape: its records and
// file marks, and nothing behind them.  Now and then another tape is mounted
// in the middle, or the machine is reset, which puts the boot tape back on.
//
// The file is served a block of 512 bytes at a time as the MiSTer framework
// serves it, the boot tape a word at a time.
//
// At the end the times of a write, of spacing and of a file mark are counted
// with the times of the real drive: what a read of as many bytes takes.
// Exit status: 0 if everything was as it should be, 1 if not.
#include "Vios_tape_tb.h"
#include "verilated.h"
#include "ios_media.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <optional>
#include <random>
#include <string>
#include <vector>

typedef std::optional<std::vector<uint8_t>> Item;   // a record, or a file mark

// The tape as it should be.
struct Tape {
    std::vector<Item> items;
    size_t position = 0;
    int state = 0;            // 0 load point, 1 behind a record, 2 behind a file mark, 3 end
    long room = 0;            // bytes of the file
    bool locked = false;
    static long size(const Item &i) { return i ? (long)i->size() + 8 : 4; }
    long at() const { long n = 0; for (size_t k = 0; k < position; k++) n += size(items[k]); return n; }
    unsigned dia(unsigned status) const { return status | (locked ? 4 : 0) | (state == 0 ? 0x80 : state == 2 ? 0x100 : state == 3 ? 0x200 : 0); }
    // the file that holds it
    std::vector<uint8_t> bytes(bool end) const {
        std::vector<uint8_t> out;
        for (const Item &i : items) {
            uint32_t n = i ? i->size() : 0;
            for (int k = 0; k < 4; k++) out.push_back(n >> (8 * k) & 0xFF);
            if (i) { out.insert(out.end(), i->begin(), i->end()); for (int k = 0; k < 4; k++) out.push_back(n >> (8 * k) & 0xFF); }
        }
        if (end) for (int k = 0; k < 4; k++) out.push_back(0xFF);
        return out;
    }
};

// What a file holds, read as the drive reads it.
static std::vector<Item> parse(const std::vector<uint8_t> &f, long room, bool &sound) {
    std::vector<Item> items;
    long at = 0;
    sound = true;
    while (at + 4 <= room) {
        uint32_t n = f[at] | f[at + 1] << 8 | f[at + 2] << 16 | (uint32_t)f[at + 3] << 24;
        if (n >> 24) break;
        if (n == 0) { items.push_back(std::nullopt); at += 4; continue; }
        if (at + n + 8 > room) break;
        uint32_t m = f[at + 4 + n] | f[at + 5 + n] << 8 | f[at + 6 + n] << 16 | (uint32_t)f[at + 7 + n] << 24;
        if (m != n) sound = false;
        items.push_back(std::vector<uint8_t>(f.begin() + at + 4, f.begin() + at + 4 + n));
        at += n + 8;
    }
    return items;
}

static const long MS = 80000, PARCEL = MS / 30, START = 8 * MS;

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    long cases = 300; unsigned seed = 1;
    for (int n = 1; n < argc; n++) {
        if (!strcmp(argv[n], "--seed") && n + 1 < argc) seed = atoi(argv[++n]);
        else if (argv[n][0] != '+') cases = atol(argv[n]);
    }
    std::mt19937 rng(seed);
    auto pick = [&](int n) { return (int)(rng() % n); };

    Vios_tape_tb *top = new Vios_tape_tb;
    std::vector<uint16_t> mem(1 << 16, 0);
    SectorDisks sd(1, 512);
    std::vector<uint64_t> boot_words;
    long clocks = 0, failed = 0, commands = 0, written = 0;

    top->clk = 0; top->rst = 1; top->i_real = 0; top->i_strobe = 0; top->i_function = 0; top->i_a = 0;
    top->i_dma_ack = 0; top->i_dma_rdata = 0; top->i_boot_ack = 0; top->i_boot_data = 0; top->i_boot_bytes = 0;
    top->i_mounted = 0; top->i_blocks = 0; top->i_readonly = 0;
    top->i_sd_ack = 0; top->i_sd_buff_addr = 0; top->i_sd_buff_dout = 0; top->i_sd_buff_wr = 0;
    top->eval();

    auto clock = [&]() {
        top->eval();
        bool req = top->o_dma_req, we = top->o_dma_we; uint16_t addr = top->o_dma_addr, wdata = top->o_dma_wdata;
        bool boot_req = top->o_boot_req && !top->i_boot_ack; uint32_t boot_addr = top->o_boot_addr;
        unsigned rd = top->o_sd_rd, wr = top->o_sd_wr; uint32_t lba = top->o_sd_lba; uint8_t din = top->o_sd_buff_din;
        top->i_dma_ack = req;
        top->eval();
        top->clk = 1; top->eval();
        if (req) { if (we) mem[addr] = wdata; else top->i_dma_rdata = mem[addr]; }
        top->i_boot_ack = boot_req;
        if (boot_req) top->i_boot_data = boot_addr < boot_words.size() ? boot_words[boot_addr] : 0;
        sd.clock(rd, wr, lba, din);
        top->i_sd_ack = sd.ack; top->i_sd_buff_addr = sd.buff_addr; top->i_sd_buff_dout = sd.buff_dout; top->i_sd_buff_wr = sd.buff_wr;
        top->i_strobe = 0; top->i_mounted = 0;
        top->eval();
        top->clk = 0; top->eval();
        clocks++;
    };
    auto wait = [&](long n) { while (n-- > 0) clock(); };
    auto fn = [&](int f, unsigned a) { top->i_function = f; top->i_a = a; top->i_strobe = 1; clock(); };
    auto del = [&](int f, unsigned a) { fn(f, a); wait(90); };
    auto reg = [&](int f) { del(f, 0); fn(010, 0); return (unsigned)top->o_data; };
    auto until_ask = [&](long limit) { long n = 0; while (!top->o_ask && n < limit) { clock(); n++; } return n; };
    // a command: C, B, A and Start; then the interrupt, and Clear
    auto command = [&](int code, unsigned b, unsigned c, long limit) {
        del(016, c); del(015, b); del(014, code << 3);
        fn(017, 1);
        long n = until_ask(limit);
        if (n >= limit) { printf("a command %d was not done\n", code); failed++; }
        wait(90);
        del(017, 2);
        return n;
    };
    // the file in the hands of the framework: what was written over what it held
    std::vector<uint8_t> file;
    auto mount = [&](const std::vector<uint8_t> &bytes, long blocks, bool readonly) {
        file.assign(blocks * 512, 0);
        for (size_t n = 0; n < file.size(); n++) file[n] = n < bytes.size() ? bytes[n] : (uint8_t)(rng() | 1);
        sd.written[0].clear();
        for (long k = 0; k < blocks; k++) sd.written[0][(uint32_t)k] = std::vector<uint8_t>(file.begin() + k * 512, file.begin() + (k + 1) * 512);
        top->i_blocks = blocks; top->i_readonly = readonly; top->i_mounted = 1;
        clock();
        wait(4);
    };
    auto held = [&]() {
        std::vector<uint8_t> f(file.size());
        for (auto &w : sd.written[0]) if ((size_t)w.first * 512 < f.size()) std::copy(w.second.begin(), w.second.end(), f.begin() + (size_t)w.first * 512);
        return f;
    };
    auto random_tape = [&](long room, bool blank) {
        Tape t;
        t.room = room;
        if (blank) return t;
        long used = 0;
        for (int k = pick(12); k > 0; k--) {
            Item i;
            if (pick(5)) { std::vector<uint8_t> r(1 + pick(pick(4) ? 60 : 1200)); for (auto &b : r) b = rng(); i = r; }
            if (used + Tape::size(i) + 4 > room) break;
            used += Tape::size(i);
            t.items.push_back(i);
        }
        return t;
    };

    for (long c = 0; c < cases && !failed; c++) {
        top->rst = 1; wait(4); top->rst = 0; wait(4);
        fn(05, 022); del(07, 2);
        // the boot tape, which a reset has put on
        Tape boot = random_tape(1 << 20, false);
        boot.locked = true;
        std::vector<uint8_t> bb = boot.bytes(false);
        boot.room = bb.size();
        boot_words.assign((bb.size() + 7) / 8, 0);
        for (size_t n = 0; n < bb.size(); n++) boot_words[n / 8] |= (uint64_t)bb[n] << (56 - 8 * (n % 8));
        top->i_boot_bytes = bb.size();
        Tape tape = boot;
        bool on_file = false;
        auto new_file = [&]() {
            long blocks = 1 + pick(pick(3) ? 6 : 40);
            tape = random_tape(blocks * 512, !pick(4));
            tape.locked = !pick(5);
            // behind a tape that does not fill its file the file says so; a blank one may hold anything that is no length
            std::vector<uint8_t> b = tape.bytes(true);
            if ((long)b.size() > blocks * 512) b.resize(blocks * 512);
            mount(b, blocks, tape.locked);
            on_file = true;
        };
        if (pick(8)) new_file();

        int steps = 5 + pick(40);
        for (int s = 0; s < steps && !failed; s++) {
            if (!pick(40)) { new_file(); continue; }
            if (!pick(120)) {
                // a reset of the machine: the boot tape again, at its load point
                top->rst = 1; wait(4); top->rst = 0; wait(4);
                fn(05, 022); del(07, 2);
                tape = boot; tape.position = 0; tape.state = 0; on_file = false;
                continue;
            }
            int kind = pick(100);
            unsigned b = 0x1000 + pick(0x8000), status = 1;
            int count = 0, code = 0;
            std::vector<uint16_t> stored;
            for (int n = 0; n < 700; n++) mem[(uint16_t)(b + n)] = rng();
            std::vector<uint16_t> before(mem.begin() + b, mem.begin() + b + 700);
            unsigned b_after = b, c_after = 0;
            if (kind < 30) {                    // read
                code = 0; count = pick(4) ? 700 : pick(40);
                if (tape.position >= tape.items.size() || tape.at() + 4 > tape.room) { tape.state = 3; status = 0x8001; }
                else if (!tape.items[tape.position]) { tape.position++; tape.state = 2; status = 0x8001; }
                else {
                    const std::vector<uint8_t> &r = *tape.items[tape.position];
                    for (size_t n = 0; n < r.size() && (int)(n / 2) < count; n += 2) stored.push_back(r[n] << 8 | (n + 1 < r.size() ? r[n + 1] : 0));
                    tape.position++; tape.state = 1;
                }
                b_after = b + stored.size(); c_after = (uint16_t)(-count + (int)stored.size());
            } else if (kind < 45) {             // space forward
                code = 3; count = pick(6) ? 1 + pick(3) : 0;
                int left = count;
                while (left > 0) {
                    if (tape.position >= tape.items.size() || tape.at() + 4 > tape.room) { tape.state = 3; status = 0x8001; break; }
                    bool record = (bool)tape.items[tape.position];
                    tape.position++; tape.state = record ? 1 : 2;
                    if (!record) { status = 0x8001; break; }
                    left--;
                }
                c_after = (uint16_t)-left;
            } else if (kind < 60) {             // space backward
                code = 4; count = pick(6) ? 1 + pick(3) : 0;
                int left = count;
                while (left > 0) {
                    if (tape.position == 0) { tape.state = 0; status = 0x8001; break; }
                    tape.position--;
                    bool record = (bool)tape.items[tape.position];
                    tape.state = tape.position == 0 ? 0 : record ? 1 : 2;
                    if (!record) { status = 0x8001; break; }
                    left--;
                }
                c_after = (uint16_t)-left;
            } else if (kind < 82) {             // write
                code = 5; count = pick(8) ? 1 + pick(pick(4) ? 30 : 600) : 0;
                c_after = (uint16_t)-count;
                if (tape.locked) status = 0x9001;
                else if (count == 0) status = 1;
                else if (tape.at() + 2 * count + 8 > tape.room) { tape.state = 3; status = 0x8001; }
                else {
                    std::vector<uint8_t> r;
                    for (int n = 0; n < count; n++) { r.push_back(mem[(uint16_t)(b + n)] >> 8); r.push_back(mem[(uint16_t)(b + n)] & 0xFF); }
                    tape.items.resize(tape.position);
                    tape.items.push_back(r);
                    tape.position++; tape.state = 1;
                    b_after = b + count; c_after = 0; written++;
                }
            } else if (kind < 90) {             // write a file mark
                code = 6; count = pick(3);
                c_after = (uint16_t)-count;
                if (tape.locked) status = 0x9001;
                else if (tape.at() + 4 > tape.room) { tape.state = 3; status = 0x8001; }
                else { tape.items.resize(tape.position); tape.items.push_back(std::nullopt); tape.position++; tape.state = 2; written++; }
            } else if (kind < 96) {             // rewind
                code = 1; count = pick(3); c_after = (uint16_t)-count;
                tape.position = 0; tape.state = 0;
            } else {                            // erase, and a command that is none
                code = pick(2) ? 7 : 2; count = pick(3); c_after = (uint16_t)-count;
            }
            command(code, b, (uint16_t)-count, 4000000);
            commands++;
            unsigned got = reg(1), got_b = reg(2), got_c = reg(3);
            std::vector<uint16_t> want(before);
            for (size_t n = 0; n < stored.size(); n++) want[n] = stored[n];
            bool same_mem = std::equal(want.begin(), want.end(), mem.begin() + b);
            if (got != tape.dia(status) || got_b != (uint16_t)b_after || got_c != c_after || !same_mem) {
                printf("case %ld step %d, command %d with a count of %d on %s: status %04x for %04x, address %04x for %04x, count %04x for %04x%s\n",
                       c, s, code, count, on_file ? "a tape file" : "the boot tape", got, tape.dia(status), got_b, (uint16_t)b_after, got_c, c_after,
                       same_mem ? "" : ", and Local Memory is not as it should be");
                failed++;
            }
        }
        // the file holds the tape, and ends behind it
        if (on_file && !failed) {
            bool sound;
            std::vector<Item> items = parse(held(), tape.room, sound);
            if (!sound || items != tape.items) { printf("case %ld: the file holds %zu records and file marks, not the %zu of the tape\n", c, items.size(), tape.items.size()); failed++; }
        }
    }
    printf("%ld cases: %ld commands, %ld records and file marks written\n", cases, commands, written);

    // ---- the times of the real drive
    if (!failed) {
        auto check = [&](const char *what, long got, long low, long high) {
            bool ok = got >= low && got <= high;
            if (!ok) { failed++; printf("%-58s %9ld clocks  OUT OF RANGE\n    it should be %ld to %ld\n", what, got, low, high); }
        };
        top->rst = 1; wait(4); top->rst = 0; wait(4);
        top->i_real = 1;
        fn(05, 022); del(07, 2);
        Tape blank; blank.room = 64 * 512;
        mount(blank.bytes(true), 64, false);
        // what the file takes besides: a block to fetch and one to put back, some thousand clocks each
        const long FILE_TIME = 3 * 512 * 4 + 400;
        check("a write of 300 parcels", command(5, 0x1000, (uint16_t)-300, 4000000), START + 300 * PARCEL, START + 300 * PARCEL + FILE_TIME);
        wait(6 * MS);
        check("a write of 100 parcels", command(5, 0x1000, (uint16_t)-100, 4000000), START + 100 * PARCEL, START + 100 * PARCEL + FILE_TIME);
        wait(6 * MS);
        check("a file mark written", command(6, 0, 0, 4000000), START + PARCEL, START + PARCEL + FILE_TIME);
        wait(6 * MS);
        check("backward over the file mark", command(4, 0, (uint16_t)-1, 4000000), START + PARCEL, START + PARCEL + FILE_TIME);
        wait(6 * MS);
        check("backward over two records", command(4, 0, (uint16_t)-2, 8000000), 2 * START + 400 * PARCEL, 2 * START + 400 * PARCEL + FILE_TIME);
        wait(6 * MS);
        check("forward over two records", command(3, 0, (uint16_t)-2, 8000000), 2 * START + 400 * PARCEL, 2 * START + 400 * PARCEL + FILE_TIME);
        wait(6 * MS);
        long far = 300 * 2 + 480 + 100 * 2 + 480;
        check("a rewind over them", command(1, 0, 0, 8000000), far * (MS / 160), far * (MS / 160) + 10);
    }

    printf("%s\n", failed ? "the tape bench FAILED" : "the tape bench passed");
    delete top;
    return failed ? 1 : 0;
}

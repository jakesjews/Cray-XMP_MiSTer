// The I/O Subsystem (rtl/ios/ios.v: three I/O Processors and their consoles)
// booting its kernel, with stand-ins for what is not written in hardware yet:
// Buffer Memory, and the Peripheral Expander with its tape drive.  Every
// other channel has nothing on it.
//
//   Vios KERNEL TAPE [--until TEXT] [--type TEXT=KEYS]... [--ms N]
//        [--poke PARCEL=VALUE]... [--quiet]
//
// KERNEL is the IOP kernel (parcels, high byte first), which is put into
// Buffer Memory at address 0; TAPE the boot tape in .tap format.  The run
// ends when the MIOP's operator console (channels 46 and 47) has shown TEXT,
// or after N milliseconds of machine time (default 20000).  --type presses
// KEYS on that console once it has shown TEXT (\r is RETURN); several are
// taken in order.  --poke changes a parcel of the kernel (hexadecimal).
// Exit status: 0 if TEXT was shown (or none was asked for), 1 if not.
#include "Vios.h"
#include "Vios___024root.h"
#include "verilated.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
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

// The tape: records, and file marks (a record of no bytes).
struct Tape {
    std::vector<std::vector<uint8_t>> items; std::vector<bool> mark;
    size_t position = 0; int state = 0;                 // 0 beginning, 1 record, 2 file mark, 3 end
    bool load(const std::vector<uint8_t> &b) {
        for (size_t at = 0; at + 4 <= b.size();) {
            size_t n = b[at] | b[at + 1] << 8 | b[at + 2] << 16 | (size_t)b[at + 3] << 24; at += 4;
            if (n == 0) { items.emplace_back(); mark.push_back(true); continue; }
            if (at + n + 4 > b.size()) return false;
            items.emplace_back(b.begin() + at, b.begin() + at + n); mark.push_back(false); at += n + 4;
        }
        return !items.empty();
    }
};

// The Peripheral Expander of the MIOP with the tape drive, device 22, as the
// model has it (tools/crates/ios/src/expander.rs); operations take no time.
struct Expander {
    Tape tape;
    unsigned address = 0, bus = 0, mask = 0;
    bool channel_interrupts = false, device_interrupts = false;
    bool busy = false, done = false;                    // of the channel
    unsigned a = 0, b = 0, c = 0, status = 0;            // of the tape drive
    bool t_done = false, t_interrupt = false;
    bool tape_selected() const { return address == 022; }
    unsigned device_status() const {
        if (!tape_selected()) return 0;
        return 1 << 7 | ((mask & 1 << 5) ? 0 : 1 << 9) | t_interrupt << 13 | t_done << 15;
    }
    unsigned channel_status() const { return channel_interrupts << 8 | busy << 10 | busy << 11 | done << 12; }
    bool ask() const { return (channel_interrupts && done) || (device_interrupts && t_interrupt && !(mask & 1 << 5)); }
    unsigned dia() const { static const unsigned bits[] = {0x80, 0, 0x100, 0x200}; return status | bits[tape.state]; }
    // returns the value read; mem is the MIOP's Local Memory
    template <class Mem> unsigned function(unsigned fn, unsigned acc, Mem &mem) {
        bool selected = tape_selected();
        switch (fn) {
        case 000: busy = done = false; channel_interrupts = false; return 0;
        case 001: bus = selected ? dia() : 0; break;
        case 002: bus = selected ? b : 0; break;
        case 003: bus = selected ? c : 0; break;
        case 004: break;
        case 005: address = acc & 077; return 0;
        case 006: mask = acc; break;
        case 007: channel_interrupts = acc & 1; device_interrupts = acc & 2; return 0;
        case 010: return bus;
        case 011: return device_status() | channel_status() | (t_interrupt ? 022 : 0);
        case 013: return device_status() | channel_status() | address;
        case 014: if (selected) a = acc; break;
        case 015: if (selected) b = acc; break;
        case 016: if (selected) c = acc; break;
        case 017:
            if (!selected) break;
            if (acc & 2) t_interrupt = false;
            if (acc & 1) {
                switch (a >> 3 & 7) {
                case 0:                                 // read a record, at most the count of it
                    if (tape.position >= tape.items.size()) { tape.state = 3; status = 0x8001; }
                    else if (tape.mark[tape.position]) { tape.position++; tape.state = 2; status = 0x8001; }
                    else {
                        const auto &r = tape.items[tape.position++];
                        size_t parcels = (r.size() + 1) / 2, count = (0x10000 - c) & 0xFFFF;
                        for (size_t n = 0; n < parcels && n < count; n++) {
                            mem[b & 0xFFFF] = r[2 * n] << 8 | (2 * n + 1 < r.size() ? r[2 * n + 1] : 0);
                            b = (b + 1) & 0xFFFF; c = (c + 1) & 0xFFFF;
                        }
                        tape.state = 1; status = 1;
                    }
                    break;
                case 1: tape.position = 0; tape.state = 0; status = 1; break;
                default: status = 1; break;
                }
                t_done = true; t_interrupt = true;
            }
            break;
        default: return 0;
        }
        busy = false; done = true;                       // a delayed function, over at once
        return 0;
    }
};

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    std::vector<std::string> files;
    std::string until;
    long ms = 20000;
    bool quiet = false;
    std::vector<std::pair<unsigned, unsigned>> pokes;
    std::vector<std::pair<std::string, std::string>> typing;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        auto next = [&]() { return std::string(i + 1 < argc ? argv[++i] : ""); };
        if (a == "--until") until = next();
        else if (a == "--type") {
            std::string t = next(), keys; size_t eq = t.find('=');
            if (eq == std::string::npos) { fprintf(stderr, "--type takes TEXT=KEYS\n"); return 2; }
            for (size_t n = eq + 1; n < t.size(); n++) {
                if (t[n] == '\\' && n + 1 < t.size() && t[n + 1] == 'r') { keys.push_back('\r'); n++; } else keys.push_back(t[n]);
            }
            typing.push_back({t.substr(0, eq), keys});
        }
        else if (a == "--ms") ms = atol(next().c_str());
        else if (a == "--quiet") quiet = true;
        else if (a == "--poke") { unsigned p = 0, v = 0; sscanf(next().c_str(), "%x=%x", &p, &v); pokes.push_back({p, v}); }
        else files.push_back(a);
    }
    if (files.size() != 2) { fprintf(stderr, "usage: Vios KERNEL TAPE [--until TEXT] [--type TEXT=KEYS]... [--ms N] [--poke PARCEL=VALUE]... [--quiet]\n"); return 2; }
    std::vector<uint8_t> kernel = read_file(files[0]);
    Expander expander;
    if (kernel.empty() || !expander.tape.load(read_file(files[1]))) { fprintf(stderr, "cannot read the kernel or the tape\n"); return 2; }
    for (auto &p : pokes) if (2 * p.first + 1 < kernel.size()) { kernel[2 * p.first] = p.second >> 8; kernel[2 * p.first + 1] = p.second; }

    std::vector<uint64_t> bm(1 << 22, 0);
    for (size_t n = 0; n + 1 < kernel.size(); n += 2)
        bm[n / 8] |= (uint64_t)(kernel[n] << 8 | kernel[n + 1]) << (48 - 16 * (n / 2 % 4));

    Vios *top = new Vios;
    auto *in = top->rootp;
    auto &miop_memory = in->ios__DOT__core__DOT__g_iop__BRA__0__KET____DOT__u__DOT__mem;
    // Local Memory holds something at power-up; zero is as good as anything
    for (int i = 0; i < 65536; i++) {
        in->ios__DOT__core__DOT__g_iop__BRA__0__KET____DOT__u__DOT__mem[i] = 0;
        in->ios__DOT__core__DOT__g_iop__BRA__1__KET____DOT__u__DOT__mem[i] = 0;
        in->ios__DOT__core__DOT__g_iop__BRA__2__KET____DOT__u__DOT__mem[i] = 0;
    }
    top->clk = 0; top->rst = 1;
    top->i_bm_ack = 0; top->i_bm_rdata = 0;
    top->i_key_valid = 0; top->i_key = 0; top->i_char_ready = 077;
    top->i_ch_data = 0; top->i_dma_req = 0; top->i_dma_we = 0; top->i_dma_addr = 0; top->i_dma_wdata = 0;
    for (int i = 0; i < 3; i++) { top->i_ch_busy[i] = 0; top->i_ch_done[i] = 0; top->i_ch_ask[i] = 0; }

    // the flags of channels 14 to 47 octal of each IOP
    bool busy[3][40] = {}, done[3][40] = {};
    std::string console[3][4];
    uint64_t data = 0;                                   // what the functions of the last clock read
    long clocks = 0, steps[3] = {0, 0, 0};
    bool shown = until.empty();
    size_t printed = 0, said = 0, at_key = 0, typed_from = 0;
    long key_gap = 0;
    bool asked = false;

    auto set_bits = [](VlWide<3> &v, int bit, bool value) { if (value) v[bit / 32] |= 1u << (bit % 32); else v[bit / 32] &= ~(1u << (bit % 32)); };

    const long limit = ms * 80000;
    while (clocks < limit && !(shown && !until.empty())) {
        if (clocks == 8) top->rst = 0;
        // what the outputs ask for in this clock
        bool bm_req = top->o_bm_req && !top->i_bm_ack, bm_we = top->o_bm_we;
        uint32_t bm_addr = top->o_bm_addr & ((1 << 22) - 1); uint64_t bm_wdata = top->o_bm_wdata;
        uint64_t next_data = 0;
        for (int g = 0; g < 3; g++) {
            if (!(top->o_ch_strobe >> g & 1)) continue;
            unsigned ch = top->o_ch_number >> (6 * g) & 077, fn = top->o_ch_function >> (4 * g) & 017, acc = top->o_ch_a >> (16 * g) & 0xFFFF;
            unsigned value = 0;
            if (g == 0 && ch == 017) {
                value = expander.function(fn, acc, miop_memory);
                busy[0][ch] = expander.busy; done[0][ch] = expander.done;
            }
            next_data |= (uint64_t)value << (16 * g);
        }
        // a character a console holds out is taken in this clock
        for (int c = 0; c < 6; c++)
            if (top->o_char_valid >> c & 1) (c < 4 ? console[0][c] : console[c - 3][1]).push_back(top->o_char >> (7 * c) & 0x7F);
        bool key_taken = (top->i_key_valid & 8) && (top->o_key_ready & 8);
        top->clk = 1; top->eval();
        // the operator: a key, some time after the one before, once the text has been shown
        if (key_taken) { top->i_key_valid = 0; key_gap = 200000; }
        if (key_gap > 0) key_gap--;
        if (!top->i_key_valid && key_gap == 0 && said < typing.size()) {
            // the kernel drops a key that comes before it has finished its question
            if (!asked && console[0][3].find(typing[said].first, typed_from) == std::string::npos) key_gap = 4096;
            else if (!asked) { asked = true; key_gap = 800000; }
            else {
                top->i_key = (QData)(typing[said].second[at_key++] & 0x7F) << 21; top->i_key_valid = 8;
                // what the last key brings is looked for from here on
                if (at_key == typing[said].second.size()) { said++; at_key = 0; asked = false; typed_from = console[0][3].size(); }
            }
        }
        top->i_bm_ack = bm_req;
        if (bm_req) { if (bm_we) bm[bm_addr] = bm_wdata; else top->i_bm_rdata = bm[bm_addr]; }
        top->i_ch_data = next_data;
        for (int g = 0; g < 3; g++)
            for (int ch = 12; ch < 40; ch++) {
                set_bits(top->i_ch_busy, 28 * g + ch - 12, busy[g][ch]);
                set_bits(top->i_ch_done, 28 * g + ch - 12, done[g][ch]);
            }
        set_bits(top->i_ch_ask, 017 - 12, expander.ask());
        top->eval();
        for (int g = 0; g < 3; g++) steps[g] += top->o_step >> g & 1;
        top->clk = 0; top->eval();
        clocks++;
        (void)data;
        std::string &kernel_console = console[0][3];
        if (!quiet && kernel_console.size() > printed) {
            for (; printed < kernel_console.size(); printed++) { char c = kernel_console[printed]; if (c == '\n' || (c >= 0x20 && c < 0x7F)) putchar(c); }
            fflush(stdout);
        }
        if (!until.empty() && (clocks & 0xFFF) == 0 && said == typing.size() && kernel_console.find(until, typed_from) != std::string::npos) shown = true;
    }
    printf("\n%.3f s of machine time; steps: MIOP %ld, BIOP %ld, XIOP %ld; P %04x %04x %04x\n", clocks / 8e7, steps[0], steps[1], steps[2],
           (unsigned)(top->o_p & 0xFFFF), (unsigned)(top->o_p >> 16 & 0xFFFF), (unsigned)(top->o_p >> 32 & 0xFFFF));
    for (int g = 1; g < 3; g++) {
        std::string first;
        for (char c : console[g][1]) { if (c >= 0x20 && c < 0x7F) first.push_back(c); else if (!first.empty() && first.back() != '|') first.push_back('|'); }
        printf("console of the %s: %.160s\n", g == 1 ? "BIOP" : "XIOP", first.c_str());
    }
    if (!shown) printf("`%s` did not appear on the MIOP's console\n", until.c_str());
    top->final();
    delete top;
    return shown ? 0 : 1;
}

// The I/O Subsystem (rtl/ios/ios.v: three I/O Processors and their consoles)
// booting its kernel, with stand-ins for what the hardware is given from
// outside: Buffer Memory, central memory, the file that is the boot tape, and
// the sectors of the Peripheral Expander's disk and of the BIOP's nine drives.
//
//   Vios KERNEL TAPE [DISK] [--drive N=FILE]... [--until TEXT] [--type TEXT=KEYS]...
//        [--ms N] [--poke PARCEL=VALUE]... [--quiet]
//
// KERNEL is the IOP kernel (parcels, high byte first), which is put into
// Buffer Memory at address 0; TAPE the boot tape in .tap format; DISK the
// image of the expander disk, which is never written to: sectors the run
// writes are kept in memory.  Without DISK the disk is all zeros.  --drive
// gives drive N (0 to 8, in the order of the BIOP's disk channels) its image;
// a drive without one is all zeros.  The run
// ends when the MIOP's operator console (channels 46 and 47) has shown TEXT,
// or after N milliseconds of machine time (default 20000).  --type presses
// KEYS on that console once it has shown TEXT (\r is RETURN); several are
// taken in order.  --poke changes a parcel of the kernel (hexadecimal).
// --quick leaves out what only takes time: the kernel's test of Local Memory,
// most of its test of Buffer Memory, and 499 of the 500 passes of the BIOP's
// test of each disk drive (the three changes are the cray-sim project's).
//
// Built with XMP_MACHINE the hardware is the whole machine (rtl/xmp_machine.v):
// the CPU as well, with central memory served to it here.
// Exit status: 0 if TEXT was shown (or none was asked for), 1 if not.
#ifdef XMP_MACHINE
#include "Vxmp_machine.h"
#include "Vxmp_machine___024root.h"
typedef Vxmp_machine Top;
#define LOCAL_MEMORY(n) in->xmp_machine__DOT__subsystem__DOT__core__DOT__g_iop__BRA__##n##__KET____DOT__u__DOT__mem
#else
#include "Vios.h"
#include "Vios___024root.h"
typedef Vios Top;
#define LOCAL_MEMORY(n) in->ios__DOT__core__DOT__g_iop__BRA__##n##__KET____DOT__u__DOT__mem
#endif
#include "verilated.h"
#include "ios_media.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <string>
#include <vector>

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    std::vector<std::string> files;
    std::string until;
    long ms = 20000;
    bool quiet = false, quick = false;
    std::vector<std::pair<unsigned, unsigned>> pokes;
    std::vector<std::pair<std::string, std::string>> typing;
    std::vector<std::pair<int, std::string>> drive_files;
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
        else if (a == "--quick") quick = true;
        else if (a == "--drive") { std::string t = next(); size_t eq = t.find('='); if (eq != std::string::npos) drive_files.push_back({atoi(t.c_str()), t.substr(eq + 1)}); }
        else if (a == "--poke") { unsigned p = 0, v = 0; sscanf(next().c_str(), "%x=%x", &p, &v); pokes.push_back({p, v}); }
        else files.push_back(a);
    }
    if (files.size() != 2 && files.size() != 3) { fprintf(stderr, "usage: Vios KERNEL TAPE [DISK] [--drive N=FILE]... [--until TEXT] [--type TEXT=KEYS]... [--ms N] [--poke PARCEL=VALUE]... [--quiet]\n"); return 2; }
    std::vector<uint8_t> kernel = read_file(files[0]);
    std::vector<uint8_t> tape = read_file(files[1]);
    if (kernel.empty() || tape.empty()) { fprintf(stderr, "cannot read the kernel or the tape\n"); return 2; }
    SectorDisks disk(1, 512), drives(9, 4096);
    if (files.size() == 3 && !(disk.file[0] = fopen(files[2].c_str(), "rb"))) { fprintf(stderr, "cannot read the disk\n"); return 2; }
    for (auto &d : drive_files)
        if (d.first < 0 || d.first > 8 || !(drives.file[d.first] = fopen(d.second.c_str(), "rb"))) { fprintf(stderr, "cannot read drive %d\n", d.first); return 2; }
    std::vector<uint64_t> cm(1 << 22, 0);                // central memory
    if (quick) {
        pokes.push_back({0x42B7, 0x0200});
        pokes.push_back({0x43DA, 0});
        // overlay INDD29 on the tape: A = 500 becomes A = 1.  Byte 0x8C20 of
        // the first file, behind nine record lengths of four bytes.
        size_t at = 0x8C20 + 4 + 8 * (0x8C20 / 4096);
        if (at + 1 < tape.size() && tape[at] == 0x11 && tape[at + 1] == 0xF4) { tape[at] = 0x10; tape[at + 1] = 0x01; }
    }
    for (auto &p : pokes) if (2 * p.first + 1 < kernel.size()) { kernel[2 * p.first] = p.second >> 8; kernel[2 * p.first + 1] = p.second; }

    // the tape as the hardware reads it: words, the first byte of the file on top
    std::vector<uint64_t> tape_words((tape.size() + 7) / 8, 0);
    for (size_t n = 0; n < tape.size(); n++) tape_words[n / 8] |= (uint64_t)tape[n] << (56 - 8 * (n % 8));
    std::vector<uint64_t> bm(1 << 22, 0);
    for (size_t n = 0; n + 1 < kernel.size(); n += 2)
        bm[n / 8] |= (uint64_t)(kernel[n] << 8 | kernel[n + 1]) << (48 - 16 * (n / 2 % 4));

    Top *top = new Top;
    auto *in = top->rootp;
    // Local Memory holds something at power-up; zero is as good as anything
    for (int i = 0; i < 65536; i++) { LOCAL_MEMORY(0)[i] = 0; LOCAL_MEMORY(1)[i] = 0; LOCAL_MEMORY(2)[i] = 0; }
    top->clk = 0; top->rst = 1;
    top->i_bm_ack = 0; top->i_bm_rdata = 0;
    top->i_key_valid = 0; top->i_key = 0; top->i_char_ready = 077;
    top->i_tape_ack = 0; top->i_tape_data = 0; top->i_tape_bytes = tape.size();
    top->i_sd_ack = 0; top->i_sd_buff_addr = 0; top->i_sd_buff_dout = 0; top->i_sd_buff_wr = 0;
#ifdef XMP_MACHINE
    top->i_mem_ack = 0; top->i_mem_rdata = 0;
#else
    // no mainframe on the other end of the MIOP's channel pair
    top->i_cpu_ready = 0; top->i_cpu_parcel = 0; top->i_cpu_disconnect = 0; top->i_cpu_resume = 0;
#endif
    top->i_cm_ack = 0; top->i_cm_rdata = 0;
    top->i_drive_ack = 0; top->i_drive_buff_addr = 0; top->i_drive_buff_dout = 0; top->i_drive_buff_wr = 0;

    std::string console[3][4];
    long clocks = 0, steps[3] = {0, 0, 0};
    bool shown = until.empty();
    size_t printed = 0, said = 0, at_key = 0, typed_from = 0;
    long key_gap = 0;
    bool asked = false;
    uint32_t burst_at = 0; int burst_left = 0; long cpu_clocks = 0;
    (void)burst_at; (void)burst_left; (void)cpu_clocks;

    const long limit = ms * 80000;
    while (clocks < limit && !(shown && !until.empty())) {
        if (clocks == 8) top->rst = 0;
        // what the outputs ask for in this clock
        bool bm_req = top->o_bm_req && !top->i_bm_ack, bm_we = top->o_bm_we;
        uint32_t bm_addr = top->o_bm_addr & ((1 << 22) - 1); uint64_t bm_wdata = top->o_bm_wdata;
        bool tape_req = top->o_tape_req && !top->i_tape_ack; uint32_t tape_addr = top->o_tape_addr;
        bool sd_rd = top->o_sd_rd, sd_wr = top->o_sd_wr; uint32_t sd_lba = top->o_sd_lba; uint8_t sd_din = top->o_sd_buff_din;
        unsigned dr_rd = top->o_drive_rd, dr_wr = top->o_drive_wr; uint32_t dr_lba = top->o_drive_lba; uint8_t dr_din = top->o_drive_buff_din;
        bool cm_req = top->o_cm_req && !top->i_cm_ack, cm_we = top->o_cm_we; uint32_t cm_addr = top->o_cm_addr; uint64_t cm_wdata = top->o_cm_wdata;
#ifdef XMP_MACHINE
        bool mem_req = top->o_mem_req, mem_we = top->o_mem_we, mem_burst = top->o_mem_burst;
        uint32_t mem_addr = top->o_mem_addr; uint64_t mem_wdata = top->o_mem_wdata; bool mem_acked = top->i_mem_ack;
#endif
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
        top->i_tape_ack = tape_req;
        if (tape_req) top->i_tape_data = tape_addr < tape_words.size() ? tape_words[tape_addr] : 0;
        disk.clock(sd_rd, sd_wr, sd_lba, sd_din);
        top->i_sd_ack = disk.ack; top->i_sd_buff_addr = disk.buff_addr; top->i_sd_buff_dout = disk.buff_dout; top->i_sd_buff_wr = disk.buff_wr;
        drives.clock(dr_rd, dr_wr, dr_lba, dr_din);
        top->i_drive_ack = drives.ack; top->i_drive_buff_addr = drives.buff_addr; top->i_drive_buff_dout = drives.buff_dout; top->i_drive_buff_wr = drives.buff_wr;
        top->i_cm_ack = cm_req;
        if (cm_req) { if (cm_we) cm[cm_addr] = cm_wdata; else top->i_cm_rdata = cm[cm_addr]; }
#ifdef XMP_MACHINE
        // central memory for the CPU: a word a clock; a burst is 16 words read
        top->i_mem_ack = 0;
        if (burst_left > 0) { top->i_mem_ack = 1; top->i_mem_rdata = cm[burst_at++ & 0x3FFFFF]; burst_left--; }
        else if (mem_req && !mem_acked) {
            top->i_mem_ack = 1;
            if (mem_we) cm[mem_addr] = mem_wdata;
            else { top->i_mem_rdata = cm[mem_addr]; if (mem_burst) { burst_at = mem_addr + 1; burst_left = 15; } }
        }
        cpu_clocks += !top->o_cpu_held;
#endif
        top->eval();
        for (int g = 0; g < 3; g++) steps[g] += top->o_step >> g & 1;
        top->clk = 0; top->eval();
        clocks++;
        std::string &kernel_console = console[0][3];
        if (!quiet && kernel_console.size() > printed) {
            for (; printed < kernel_console.size(); printed++) { char c = kernel_console[printed]; if (c == '\n' || (c >= 0x20 && c < 0x7F)) putchar(c); }
            fflush(stdout);
        }
        if (!until.empty() && (clocks & 0xFFF) == 0 && said == typing.size() && kernel_console.find(until, typed_from) != std::string::npos) shown = true;
    }
    printf("\n%.3f s of machine time; steps: MIOP %ld, BIOP %ld, XIOP %ld; P %04x %04x %04x; expander disk sectors read %ld, written %ld; drive sectors read %ld, written %ld\n",
           clocks / 8e7, steps[0], steps[1], steps[2], (unsigned)(top->o_p & 0xFFFF), (unsigned)(top->o_p >> 16 & 0xFFFF),
           (unsigned)(top->o_p >> 32 & 0xFFFF), disk.reads, disk.writes, drives.reads, drives.writes);
#ifdef XMP_MACHINE
    printf("the CPU ran for %.3f s and is %s\n", cpu_clocks / 8e7, top->o_cpu_held ? "held by Master Clear" : "running");
#endif
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

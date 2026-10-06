// Core-level simulation of the CRAY X-MP core (module emu of CrayXMP.sv) with
// stand-ins for hps_io, the PLL and the DDR3 port.  The bench does what a
// user's MiSTer does: it loads the boot file into memory, serves the disk
// images a block at a time, types on the keyboard or on the HPS serial port,
// and reads the serial port and the video.
//
//   Vemu [BOOTFILE] [--disk N=FILE]... [--drive N=FILE]... [--type TEXT=KEYS]...
//        [--press TEXT=KEYS]... [--until TEXT] [--ms N] [--reset-at MS] [--reset-on TEXT] [--screen C]...
//        [--frame OUT.ppm] [--ddr fast|normal|slow] [--disk-wait CLOCKS] [--seed N] [--quiet]
//
// BOOTFILE (tools/py/mkboot.py) is put where the menu loads it; without one
// nothing is loaded.  Memory is full of junk otherwise, as DDR3 is.  --disk
// gives image slot N its file: 0 is the disk of the Peripheral Expander, 1 the
// nine DD-29 drives one after another.  --drive gives the image of one drive
// (0 to 8) instead, as if the nine were joined.  A file is never written to:
// blocks the run writes are kept in memory.  What has no file reads as zeros.
//
// --type sends KEYS on the serial port once a console has shown TEXT (\r is
// RETURN); --press types them on the keyboard, where {f1} and {f2} are the keys
// that choose the screen.  Several are taken in order.  TEXT is found whatever
// blanks and cursor movements lie between its characters; +N waits N
// milliseconds instead.  TEXT is looked for on the operator's console, or on
// the station if it begins with @0: (the numbers are those of the consoles in
// sim/harness/ios_main.cpp); typed KEYS go to that console, pressed ones to the
// one whose screen is shown.  The run ends when TEXT of --until has been shown,
// or after N milliseconds of machine time (default 20000).  --reset-at presses
// the menu's reset at that time, --reset-on when the operator's console has
// shown TEXT.  --screen C prints a console's 24 lines at the end; --frame
// writes the last video frame.
//
// At the end the two screens in the core are compared with what the serial
// port carried.  The CPU must not run before the operator has typed START:
// the core's activity light tells.  Exit status: 0 if the text of --until was
// shown (or none was asked for), the screens are right and the CPU waited, 1
// if not.
#include "Vemu.h"
#include "Vemu___024root.h"
#include "verilated.h"
#include "core_io.h"
#include "ddram_model.h"
#include "ios_media.h"

#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <string>
#include <unistd.h>
#include <vector>

// The image slots as hps_io and the MiSTer's main program serve them: a
// request is acknowledged, its bytes go one at a time into (or come out of)
// the core's buffer, and the acknowledge drops.  One request at a time.
struct HpsDisks {
    static constexpr uint32_t DRIVE_BLOCKS = 1185120;    // of a DD-29
    std::vector<FILE *> file;
    FILE *drive[9] = {};                 // the parts of slot 1, if it has no file of its own
    std::vector<std::map<uint32_t, std::array<uint8_t, 512>>> written;
    int state = 0, wait = 0, at = 0, disk = 0, bytes = 0, phase = 0, latency = 200;
    bool writing = false; uint32_t lba = 0;
    std::vector<uint8_t> data;
    long reads = 0, writes = 0;
    // what the core is given
    unsigned ack = 0, buff_addr = 0, buff_dout = 0; bool buff_wr = false;
    explicit HpsDisks(int slots) : file(slots, nullptr), written(slots) {}

    void load() {
        data.assign(bytes, 0);
        FILE *f = file[disk]; uint32_t at = lba;
        if (!f && disk == 1 && lba / DRIVE_BLOCKS < 9) { f = drive[lba / DRIVE_BLOCKS]; at = lba % DRIVE_BLOCKS; }
        if (f && fseeko(f, (off_t)at * 512, SEEK_SET) == 0) { size_t n = fread(data.data(), 1, bytes, f); (void)n; }
        for (int b = 0; b < bytes / 512; b++) {
            auto w = written[disk].find(lba + b);
            if (w != written[disk].end()) memcpy(&data[b * 512], w->second.data(), 512);
        }
    }
    void store() {
        for (int b = 0; b < bytes / 512; b++) memcpy(written[disk][lba + b].data(), &data[b * 512], 512);
    }
    // one clock: what the core drove before the edge.  The block number, the
    // count of blocks less one and the byte of the buffer are those of each slot.
    void clock(unsigned rd, unsigned wr, const uint32_t *lbas, const unsigned *counts, const unsigned *dins) {
        buff_wr = false;
        switch (state) {
        case 0:
            for (size_t n = 0; n < file.size(); n++)
                if ((rd | wr) >> n & 1) {
                    disk = n; writing = wr >> n & 1; lba = lbas[n]; bytes = (counts[n] + 1) * 512;
                    state = 1; wait = latency; break;
                }
            break;
        case 1:
            if (--wait > 0) break;
            ack = 1u << disk; at = 0; buff_addr = 0; phase = 0; state = 2; wait = 6;
            if (writing) data.assign(bytes, 0); else load();
            break;
        case 2:
            if (writing) {
                // hps_io reads the byte at the address and then moves the address on
                if (--wait > 0) break;
                wait = 6;
                if (at == bytes) { store(); writes++; state = 3; break; }
                data[at++] = dins[disk];
                buff_addr = at;
            } else {
                // hps_io: the byte, a clock later the write pulse, two clocks later the next address
                switch (phase) {
                case 0: if (--wait > 0) break; if (at == bytes) { reads++; state = 3; break; } buff_dout = data[at]; phase = 1; break;
                case 1: buff_wr = true; phase = 2; break;
                case 2: phase = 3; break;
                case 3: buff_addr = ++at; phase = 0; wait = 4; break;
                }
            }
            break;
        case 3: ack = 0; state = 0; break;
        }
    }
};

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    std::string boot, frame, until, reset_on, ddr = "normal";
    long ms = 20000, reset_at = -1;
    bool quiet = false;
    uint32_t seed = 1;
    int until_console = 0, disk_wait = 200;
    struct Typing { int console; bool pressed; std::string wait, keys; long delay; };
    std::vector<Typing> typing;
    std::vector<int> screens;
    std::vector<std::pair<int, std::string>> disk_files, drive_files;
    // a text may name its console: @0: is the station, anything else the operator's
    auto on_console = [](std::string &text) { int c = 0; if (text.size() > 3 && text[0] == '@' && text[2] == ':') { c = text[1] == '0'; text = text.substr(3); } return c; };
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        auto next = [&]() { return std::string(i + 1 < argc ? argv[++i] : ""); };
        if (a == "--until") { until = next(); until_console = on_console(until); until = squeeze(until); }
        else if (a == "--screen") screens.push_back(atoi(next().c_str()) == 0);
        else if (a == "--type" || a == "--press") {
            std::string t = next(), keys; size_t eq = t.find('=');
            if (eq == std::string::npos) { fprintf(stderr, "%s takes TEXT=KEYS\n", a.c_str()); return 2; }
            for (size_t n = eq + 1; n < t.size(); n++) {
                if (t[n] == '\\' && n + 1 < t.size() && t[n + 1] == 'r') { keys.push_back('\r'); n++; }
                else if (t.compare(n, 4, "{f1}") == 0) { keys.push_back(0x01); n += 3; }
                else if (t.compare(n, 4, "{f2}") == 0) { keys.push_back(0x02); n += 3; }
                else keys.push_back(t[n]);
            }
            std::string wait = t.substr(0, eq);
            int c = on_console(wait);
            long delay = wait.size() > 1 && wait[0] == '+' ? atol(wait.c_str() + 1) * 81667 : 0;
            typing.push_back({c, a == "--press", squeeze(wait), keys, delay});
        }
        else if (a == "--ms") ms = atol(next().c_str());
        else if (a == "--reset-at") reset_at = atol(next().c_str());
        else if (a == "--reset-on") reset_on = squeeze(next());
        else if (a == "--frame") frame = next();
        else if (a == "--ddr") ddr = next();
        else if (a == "--disk-wait") disk_wait = atoi(next().c_str());
        else if (a == "--seed") seed = (uint32_t)atol(next().c_str());
        else if (a == "--quiet") quiet = true;
        else if (a == "--disk") { std::string t = next(); size_t eq = t.find('='); if (eq != std::string::npos) disk_files.push_back({atoi(t.c_str()), t.substr(eq + 1)}); }
        else if (a == "--drive") { std::string t = next(); size_t eq = t.find('='); if (eq != std::string::npos) drive_files.push_back({atoi(t.c_str()), t.substr(eq + 1)}); }
        else if (a[0] == '-') { fprintf(stderr, "unknown option %s\n", a.c_str()); return 2; }
        else boot = a;
    }

    // central memory, Buffer Memory and the boot file: 0x3000_0000 to 0x3500_0000
    DdramModel ram(80u << 20, seed);
    if (ddr == "fast")      { ram.prof.lat_min = 1; ram.prof.lat_max = 1; ram.prof.busy_pct = 0; }
    else if (ddr == "slow") { ram.prof.lat_min = 8; ram.prof.lat_max = 40; ram.prof.busy_pct = 30; ram.prof.gap_pct = 30; ram.prof.stall_pct_x1000 = 500; }
    if (!boot.empty() && !ram.load(boot, 0x4000000)) { fprintf(stderr, "cannot read %s\n", boot.c_str()); return 2; }

    HpsDisks disks(2);
    disks.latency = disk_wait;
    for (auto &d : disk_files)
        if (d.first < 0 || d.first > 1 || !(disks.file[d.first] = fopen(d.second.c_str(), "rb"))) { fprintf(stderr, "cannot read the image of slot %d\n", d.first); return 2; }
    for (auto &d : drive_files)
        if (d.first < 0 || d.first > 8 || !(disks.drive[d.first] = fopen(d.second.c_str(), "rb"))) { fprintf(stderr, "cannot read the image of drive %d\n", d.first); return 2; }

    // keyboard: invert the core's own key map so typing uses the same table
    std::map<int, int> key_of;           // ascii -> code | shift << 8
    if (FILE *f = fopen("rtl/terminal/keymap.mem", "r")) {
        for (int i = 0; i < 512; i++) { unsigned v; if (fscanf(f, "%x", &v) != 1) break; if (v && !key_of.count(v)) key_of[v] = i; }
        fclose(f);
    }
    // the events of a key: ps2_key values without the toggle bit.  Caps Lock is
    // on in the core, so a capital letter is the letter's key alone.
    auto events_of = [&](unsigned char c) {
        std::vector<int> ev;
        if (c == 0x01 || c == 0x02) { int code = c == 0x01 ? 0x05 : 0x06; ev = {0x200 | code, code}; return ev; }
        if (c >= 'A' && c <= 'Z') c = c - 'A' + 'a';
        if (!key_of.count(c)) return ev;
        int code = key_of[c] & 0xFF, shift = key_of[c] >> 8;
        if (shift) ev.push_back(0x200 | 0x12);
        ev.push_back(0x200 | code);
        ev.push_back(code);
        if (shift) ev.push_back(0x12);
        return ev;
    };

    // the core reads its fonts and its key map from where they are in the
    // source tree; every file named on the command line is open by now
    if (!frame.empty() && frame[0] != '/') { char here[4096]; if (getcwd(here, sizeof here)) frame = std::string(here) + "/" + frame; }
    if (chdir("rtl/terminal") != 0) { fprintf(stderr, "run this from the top of the source tree\n"); return 2; }

    Vemu *top = new Vemu;
    auto *r = top->rootp;
    UartMonitor mon; mon.echo = false;
    UartDriver drv;
    VideoCapture vid;

    top->RESET = 1;
    top->UART_RXD = 1;
    top->DDRAM_BUSY = 0;
    top->DDRAM_DOUT_READY = 0;
    double cpu_acc = 0;
    const double cpu_ratio = 81.67 / 29.4;       // machine clock cycles per video clock cycle

    std::string console[2];              // what the serial port carried: 0 the operator's console, 1 the station
    size_t seen = 0, printed = 0, typed_from[2] = {0, 0};
    size_t said = 0, at_key = 0;
    bool asked = false, shown = false;
    long clocks = 0, key_gap = 0, waited = 0, reset_left = 0, idle_for = 0;
    std::vector<int> key_events;         // of the key being pressed
    bool start_typed = false, ran_early = false;
    size_t start_looked = 0;
    long light_fades = 0;                // clocks the activity light may still be on after a reset
    const long limit = ms * 81667;
    long ending = -1;                    // the run is over: clocks left to let the screens settle

    for (long t = 0; !Verilated::gotFinish(); t++) {
        if (t == 20) top->RESET = 0;
        if (!boot.empty()) {                        // the menu holds the core in reset while it loads
            if (t == 60)  r->emu__DOT__hps_io__DOT__sim_download = 1;
            if (t == 400) r->emu__DOT__hps_io__DOT__sim_download = 0;
        }

        // values the core drove during this video clock
        mon.step(top->UART_TXD);
        if (top->CE_PIXEL) vid.step(top->VGA_HS, top->VGA_VS, top->VGA_DE, top->VGA_R, top->VGA_G, top->VGA_B);
        idle_for = top->UART_TXD ? idle_for + 1 : 0;
        for (; seen < mon.text.size(); seen++) {
            unsigned char b = mon.text[seen];
            console[b >> 7].push_back(b & 0x7F);
        }
        // the CPU is held until the kernel has been told to start it
        if (console[0].size() != start_looked) {
            start_looked = console[0].size();
            std::string so_far = squeeze(console[0]);
            start_typed = so_far.find("STARTCOS") != std::string::npos;
            if (!reset_on.empty() && reset_left == 0 && so_far.find(reset_on) != std::string::npos) {
                r->emu__DOT__hps_io__DOT__sim_status[0] |= 1u; reset_left = 2000; reset_on.clear();
            }
        }
        if (light_fades > 0) light_fades -= 3;
        if (top->LED_USER && !start_typed && !ran_early && light_fades <= 0) { ran_early = true; printf("\nthe CPU runs at %.3f s, before START has been typed\n", clocks / 81667e3); }
        if (!quiet && console[0].size() > printed) {
            for (; printed < console[0].size(); printed++) { char c = console[0][printed]; if (c == '\n' || (c >= 0x20 && c < 0x7F)) putchar(c); }
            fflush(stdout);
        }

        top->CLK_50M = 1; top->eval();              // video clock edge: the core samples its inputs
        top->UART_RXD = drv.step();
        top->eval();

        // The machine's clock is a separate, faster one.  Its edges fall between
        // the video clock's, sometimes before the falling edge and sometimes after.
        cpu_acc += cpu_ratio;
        int n_cpu = (int)cpu_acc;
        cpu_acc -= n_cpu;
        int before = (t & 1) ? n_cpu : n_cpu / 2;
        for (int k = 0; k < n_cpu; k++) {
            if (k == before) { top->CLK_50M = 0; top->eval(); }
            bool rd = top->DDRAM_RD, we = top->DDRAM_WE;      // driven during the machine clock that ends here
            uint32_t addr = top->DDRAM_ADDR; uint8_t bc = top->DDRAM_BURSTCNT, be = top->DDRAM_BE;
            uint64_t din = top->DDRAM_DIN;
            unsigned sd_rd = r->emu__DOT__hps_io__DOT__sim_sd_rd, sd_wr = r->emu__DOT__hps_io__DOT__sim_sd_wr;
            uint64_t sd_lba = r->emu__DOT__hps_io__DOT__sim_sd_lba;
            unsigned sd_cnt = r->emu__DOT__hps_io__DOT__sim_sd_blk_cnt, sd_din = r->emu__DOT__hps_io__DOT__sim_sd_din;
            const uint32_t lbas[2] = {(uint32_t)sd_lba, (uint32_t)(sd_lba >> 32)};
            const unsigned counts[2] = {sd_cnt & 0xFF, sd_cnt >> 8}, dins[2] = {sd_din & 0xFF, sd_din >> 8};
            r->emu__DOT__pll__DOT__sim_clk1 = 1; top->eval();
            ram.step(rd, we, addr, bc, din, be);
            top->DDRAM_BUSY = ram.busy;
            top->DDRAM_DOUT_READY = ram.dout_ready;
            top->DDRAM_DOUT = ram.dout;
            disks.clock(sd_rd, sd_wr, lbas, counts, dins);
            r->emu__DOT__hps_io__DOT__sim_sd_ack = disks.ack;
            r->emu__DOT__hps_io__DOT__sim_sd_buff_addr = disks.buff_addr;
            r->emu__DOT__hps_io__DOT__sim_sd_buff_dout = disks.buff_dout;
            r->emu__DOT__hps_io__DOT__sim_sd_buff_wr = disks.buff_wr;

            // the menu's reset: everything the consoles showed is gone
            if (reset_at >= 0 && clocks == reset_at * 81667) { r->emu__DOT__hps_io__DOT__sim_status[0] |= 1u; reset_left = 2000; }
            if (reset_left > 0 && --reset_left == 0) {
                r->emu__DOT__hps_io__DOT__sim_status[0] &= ~1u;
                for (int c = 0; c < 2; c++) { console[c].clear(); typed_from[c] = 0; }
                printed = 0; start_looked = 0; start_typed = false; light_fades = 2000000;
            }

            // the operator: a key, some time after the one before, once the text has been shown
            if (key_gap > 0) key_gap--;
            if (said < typing.size() && typing[said].delay) waited++;
            if (key_gap == 0 && ending < 0 && !key_events.empty()) {
                int toggle = (r->emu__DOT__hps_io__DOT__sim_ps2_key >> 10) & 1;
                r->emu__DOT__hps_io__DOT__sim_ps2_key = ((toggle ^ 1) << 10) | key_events.front();
                key_events.erase(key_events.begin());
                key_gap = key_events.empty() ? 200000 : 20000;
            } else if (key_gap == 0 && ending < 0 && said < typing.size()) {
                const Typing &ty = typing[said];
                int c = ty.console;
                // the kernel drops a key that comes before it has finished its question
                bool there = ty.delay ? waited >= ty.delay : squeeze(console[c].substr(typed_from[c])).find(ty.wait) != std::string::npos;
                if (!asked && !there) key_gap = 4096;
                else if (!asked) { asked = true; key_gap = 800000; }
                else {
                    bool pressed = ty.pressed;
                    unsigned char key = at_key < ty.keys.size() ? ty.keys[at_key] : 0;
                    // what the last key brings is looked for from here on
                    if (++at_key >= ty.keys.size()) {
                        said++; at_key = 0; asked = false; waited = 0;
                        for (int n = 0; n < 2; n++) typed_from[n] = console[n].size();
                    }
                    if (pressed) { key_events = events_of(key); key_gap = 1; }
                    else if (key) { drv.send(std::string(1, (char)(key | (c ? 0x80 : 0)))); key_gap = 200000; }
                }
            }

            top->eval();
            r->emu__DOT__pll__DOT__sim_clk1 = 0; top->eval();
            clocks++;
            if (!until.empty() && (clocks & 0xFFFF) == 0 && said == typing.size() && ending < 0 &&
                squeeze(console[until_console].substr(typed_from[until_console])).find(until) != std::string::npos) shown = true;
        }
        top->CLK_50M = 0; top->eval();

        // At the end wait until the serial port has been silent for longer than
        // a character and a deleted line take, so that the screens and the
        // record of the port agree.
        if (ending < 0 && (shown || clocks >= limit)) ending = 0;
        if (ending >= 0 && (idle_for > 12000 || ++ending > 3000000)) break;
    }

    // the screens in the core against what the serial port carried
    bool screens_right = true;
    for (int c = 0; c < 2; c++) {
        std::string want = screen(console[c]);
        std::vector<std::string> lines;
        for (size_t at = 0; at < want.size();) { size_t nl = want.find('\n', at); lines.push_back(want.substr(at, nl - at)); at = nl + 1; }
        lines.resize(24);
        int top_row = r->emu__DOT__terminal__DOT__top_row[c];
        const auto &cells = c ? r->emu__DOT__terminal__DOT__g_screen__BRA__1__KET____DOT__screen : r->emu__DOT__terminal__DOT__g_screen__BRA__0__KET____DOT__screen;
        for (int y = 0; y < 24 && screens_right; y++)
            for (int x = 0; x < 80; x++) {
                char got = cells[(y + top_row) % 24 * 80 + x], expect = x < (int)lines[y].size() ? lines[y][x] : ' ';
                if (got != expect) {
                    printf("\nthe %s: line %d column %d shows %02x, the serial port says %02x\n", c ? "station's screen" : "operator's screen", y, x, got & 0xFF, expect & 0xFF);
                    screens_right = false;
                    break;
                }
            }
    }

    for (int c : screens) printf("\n---- %s\n%s----\n", c ? "station" : "operator's console", screen(console[c]).c_str());
    if (!frame.empty()) vid.write_ppm(frame.c_str());
    printf("\n%.3f s of machine time; memory: %llu reads, %llu writes%s; disk requests: %ld read, %ld written; %d frames (%ld lines, hsync every %ld pixels)\n",
           clocks / 81667e3, (unsigned long long)ram.reads, (unsigned long long)ram.writes, ram.bad_access ? ", BAD DDR3 ACCESS" : "",
           disks.reads, disks.writes, vid.frames, vid.last_lines, vid.hs_period);
    bool ok = (until.empty() || shown) && screens_right && !ram.bad_access && !ran_early;
    if (!until.empty()) printf("%s\n", shown ? "the text was shown" : "the text was NOT shown");
    if (!screens_right) printf("the screens are NOT what the serial port carried\n");
    if (ran_early) printf("the CPU did NOT wait for START\n");
    top->final();
    delete top;
    return ok ? 0 : 1;
}

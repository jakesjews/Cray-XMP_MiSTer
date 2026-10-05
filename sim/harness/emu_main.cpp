// Core-level simulation of the MiSTer wrapper (module emu) with stand-ins for
// hps_io, the PLL and the DDR3 port.  It captures what the core sends on the
// HPS serial port and the video it draws, and can type on the keyboard, send
// serial bytes, send a BREAK and "load" a memory image the way the menu does.
//
//   Vemu [--image FILE] [--cycles N] [--frame OUT.ppm] [--type TEXT] [--serial TEXT]
//        [--break-at CYCLE] [--font8] [--ddr fast|normal|slow] [--seed N] [--quiet]
//        [--dump START:COUNT]   (octal word address and count)
#include "Vemu.h"
#include "Vemu___024root.h"
#include "verilated.h"
#include "ddram_model.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <string>
#include <vector>

static const int UART_DIV = 255;        // 29.4 MHz / 115200

struct UartMonitor {                     // decodes the core's TXD
    int state = 0, cnt = 0, bits = 0, sh = 0;
    std::string text;
    bool echo = true;
    void step(bool txd) {
        if (state == 0) {
            if (!txd) { state = 1; cnt = UART_DIV / 2; }
        } else if (--cnt <= 0) {
            cnt = UART_DIV;
            if (state == 1) { state = txd ? 0 : 2; bits = 0; sh = 0; }
            else if (state == 2) {
                sh |= (txd ? 1 : 0) << bits;
                if (++bits == 8) state = 3;
            } else {
                if (txd) { text.push_back((char)sh); if (echo) { fputc(sh, stdout); fflush(stdout); } }
                state = 0;
            }
        }
    }
};

struct UartDriver {                      // drives the core's RXD
    std::vector<int> queue;              // 0..255 byte, -1 break
    int cnt = 0, bit = -1, cur = 0;
    bool level = true;
    void send(const std::string &s) { for (unsigned char c : s) queue.push_back(c); }
    void send_break() { queue.push_back(-1); }
    bool step() {
        if (bit < 0) {
            if (queue.empty()) return level = true;
            cur = queue.front(); queue.erase(queue.begin());
            bit = 0; cnt = (cur < 0) ? UART_DIV * 40 : UART_DIV;
            return level = false;                               // start bit or break
        }
        if (--cnt > 0) return level;
        if (cur < 0) { bit = -1; return level = true; }
        cnt = UART_DIV;
        if (bit < 8) { level = (cur >> bit) & 1; bit++; return level; }
        if (bit == 8) { bit++; return level = true; }           // stop bit
        bit = -1;
        return level = true;
    }
};

struct VideoCapture {
    static const int W = 640, H = 400;
    std::vector<uint8_t> cur, last;
    int x = 0, y = 0, last_h = 0, frames = 0;
    bool prev_de = false, prev_vs = true;
    long hs_period = 0, hs_count = 0, last_hs_tick = -1, tick = 0, lines = 0, last_lines = 0;
    bool prev_hs = true;
    VideoCapture() : cur(W * H * 3, 0), last(W * H * 3, 0) {}
    void step(bool hs, bool vs, bool de, uint8_t r, uint8_t g, uint8_t b) {
        tick++;
        if (prev_hs && !hs) { if (last_hs_tick >= 0) hs_period = tick - last_hs_tick; last_hs_tick = tick; lines++; }
        prev_hs = hs;
        if (de) {
            if (x < W && y < H) { uint8_t *p = &cur[(y * W + x) * 3]; p[0] = r; p[1] = g; p[2] = b; }
            x++;
        } else if (prev_de) { y++; x = 0; }
        prev_de = de;
        if (prev_vs && !vs) {                                   // start of vertical sync
            if (y > 0) { last = cur; last_h = y; frames++; }
            last_lines = lines; lines = 0;
            y = 0; x = 0;
        }
        prev_vs = vs;
    }
    bool write_ppm(const char *path) const {
        FILE *f = fopen(path, "wb");
        if (!f) return false;
        int h = last_h > 0 ? last_h : H;
        fprintf(f, "P6\n%d %d\n255\n", W, h);
        fwrite(last.data(), 1, (size_t)W * h * 3, f);
        fclose(f);
        return true;
    }
};

// \\r \\n \\t \\\\ \\xHH, and \\p for a pause of a million clocks when typing
static std::string unescape(const std::string &in)
{
    std::string out;
    for (size_t i = 0; i < in.size(); i++) {
        char c = in[i];
        if (c == '\\' && i + 1 < in.size()) {
            char e = in[++i];
            if (e == 'r') c = '\r'; else if (e == 'n') c = '\n'; else if (e == 't') c = '\t';
            else if (e == 'p') c = 0x1F;
            else if (e == 'x' && i + 2 < in.size()) { c = (char)strtol(in.substr(i + 1, 2).c_str(), nullptr, 16); i += 2; }
            else c = e;
        }
        out.push_back(c);
    }
    return out;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    std::string image, frame, type_text, serial_text, ddr = "normal", dump;
    long cycles = 16000000, break_at = -1, type_at = -1;
    bool font8 = false, quiet = false;
    uint32_t seed = 1;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        auto next = [&]() { return std::string(i + 1 < argc ? argv[++i] : ""); };
        if (a == "--image") image = next();
        else if (a == "--cycles") cycles = atol(next().c_str());
        else if (a == "--frame") frame = next();
        else if (a == "--type") type_text = next();
        else if (a == "--type-at") type_at = atol(next().c_str());
        else if (a == "--serial") serial_text = next();
        else if (a == "--break-at") break_at = atol(next().c_str());
        else if (a == "--font8") font8 = true;
        else if (a == "--ddr") ddr = next();
        else if (a == "--seed") seed = (uint32_t)atol(next().c_str());
        else if (a == "--quiet") quiet = true;
        else if (a == "--dump") dump = next();
    }

    DdramModel ram(32u << 20, seed);
    if (ddr == "fast")      { ram.prof.lat_min = 1; ram.prof.lat_max = 1; ram.prof.busy_pct = 0; }
    else if (ddr == "slow") { ram.prof.lat_min = 8; ram.prof.lat_max = 40; ram.prof.busy_pct = 30; ram.prof.gap_pct = 30; ram.prof.stall_pct_x1000 = 500; }
    if (!image.empty() && !ram.load(image)) { fprintf(stderr, "cannot read %s\n", image.c_str()); return 2; }

    // keyboard: invert the core's own key map so typing uses the same table
    std::map<int, int> key_of;           // ascii -> code | shift << 8
    if (FILE *f = fopen("keymap.mem", "r")) {
        for (int i = 0; i < 512; i++) { unsigned v; if (fscanf(f, "%x", &v) != 1) break; if (v && !key_of.count(v)) key_of[v] = i; }
        fclose(f);
    }
    type_text = unescape(type_text);
    serial_text = unescape(serial_text);
    std::vector<int> key_events;         // ps2_key values without the toggle bit; -1 is a pause
    for (unsigned char c : type_text) {
        if (c == '\n') c = '\r';
        if (c == 0x1F) { key_events.push_back(-1); continue; }        // \p
        bool ctrl = false;
        if (c >= 1 && c <= 26 && c != '\r' && c != '\t' && c != 8) { ctrl = true; c = (unsigned char)(c + 'a' - 1); }
        if (!key_of.count(c)) continue;
        int code = key_of[c] & 0xFF, shift = key_of[c] >> 8;
        if (ctrl) key_events.push_back(0x200 | 0x14);
        if (shift) key_events.push_back(0x200 | 0x12);
        key_events.push_back(0x200 | code);
        key_events.push_back(code);
        if (shift) key_events.push_back(0x12);
        if (ctrl) key_events.push_back(0x14);
    }

    Vemu *top = new Vemu;
    auto *r = top->rootp;
    UartMonitor mon; mon.echo = !quiet;
    UartDriver drv;
    VideoCapture vid;

    top->RESET = 1;
    top->UART_RXD = 1;
    top->DDRAM_BUSY = 0;
    top->DDRAM_DOUT_READY = 0;
    bool serial_sent = false;
    size_t key_pos = 0;
    long key_next = (type_at >= 0) ? type_at : cycles;

    for (long t = 0; t < cycles && !Verilated::gotFinish(); t++) {
        if (t == 20) top->RESET = 0;
        if (!image.empty()) {                       // the menu holds the core in reset while it loads
            if (t == 60)  r->emu__DOT__hps_io__DOT__sim_download = 1;
            if (t == 400) r->emu__DOT__hps_io__DOT__sim_download = 0;
        }
        if (t == 10 && font8) r->emu__DOT__hps_io__DOT__sim_status[0] |= (1u << 14);   // after initial blocks
        if (t == break_at) drv.send_break();
        if (!serial_text.empty() && !serial_sent && type_at >= 0 && t == type_at) { drv.send(serial_text); serial_sent = true; }
        if (key_pos < key_events.size() && t >= key_next) {
            if (key_events[key_pos] < 0) { key_pos++; key_next = t + 1000000; }      // pause
            else {
                int toggle = (r->emu__DOT__hps_io__DOT__sim_ps2_key >> 10) & 1;
                r->emu__DOT__hps_io__DOT__sim_ps2_key = ((toggle ^ 1) << 10) | key_events[key_pos++];
                key_next = t + 3000;
            }
        }

        // values the core drove during this clock
        bool rd = top->DDRAM_RD, we = top->DDRAM_WE;
        uint32_t addr = top->DDRAM_ADDR; uint8_t bc = top->DDRAM_BURSTCNT, be = top->DDRAM_BE;
        uint64_t din = top->DDRAM_DIN;
        mon.step(top->UART_TXD);
        if (top->CE_PIXEL) vid.step(top->VGA_HS, top->VGA_VS, top->VGA_DE, top->VGA_R, top->VGA_G, top->VGA_B);

        top->CLK_50M = 1; top->eval();              // clock edge: the core samples its inputs
        ram.step(rd, we, addr, bc, din, be);
        top->DDRAM_BUSY = ram.busy;
        top->DDRAM_DOUT_READY = ram.dout_ready;
        top->DDRAM_DOUT = ram.dout;
        top->UART_RXD = drv.step();
        top->eval();
        top->CLK_50M = 0; top->eval();
    }

    if (!frame.empty()) vid.write_ppm(frame.c_str());
    if (!dump.empty()) {
        unsigned start = 0, count = 8;
        sscanf(dump.c_str(), "%o:%o", &start, &count);
        for (unsigned i = 0; i < count; i++) printf("%07o: %022llo\n", start + i, (unsigned long long)ram.word(start + i));
    }
    fprintf(stderr, "\nsim: %ld cycles, %llu reads, %llu writes, %d frames (%ld lines, hsync every %ld pixels)%s\n",
            cycles, (unsigned long long)ram.reads, (unsigned long long)ram.writes, vid.frames, vid.last_lines, vid.hs_period,
            ram.bad_access ? ", BAD DDR3 ACCESS" : "");
    top->final();
    delete top;
    return ram.bad_access ? 1 : 0;
}

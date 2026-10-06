// What the core-level test benches watch and drive at the pins of the core:
// the HPS serial port and the video.
#pragma once
#include <cstdint>
#include <cstdio>
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

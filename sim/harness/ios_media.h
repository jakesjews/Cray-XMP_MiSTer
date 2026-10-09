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

// What a console has been sent, without blanks, control characters and the
// escape sequences of an Ampex Dialogue 80: for finding a text whatever moved
// the cursor between its words.
static std::string squeeze(const std::string &raw) {
    std::string out;
    for (size_t n = 0; n < raw.size(); n++) {
        unsigned char c = raw[n] & 0x7F;
        if (c == 0x1B) { n += (n + 1 < raw.size() && raw[n + 1] == '=') ? 3 : (n + 1 < raw.size() && raw[n + 1] == 'G') ? 2 : 1; continue; }
        if (c > 0x20 && c < 0x7F) out.push_back(c);
    }
    return out;
}

// The screen of an Ampex Dialogue 80, a character at a time: 24 lines of 80
// characters.  This is the terminal as far as plain text goes, enough to
// follow what the I/O Subsystem's software prints: the whole terminal is
// tools/crates/ios/src/screen.rs.  The first page only, conversation mode, no
// protected characters, no tab stops; a cursor address off the screen, which
// the terminal answers with CURS ERR, puts the cursor at the top left here.
// The line drawing characters are kept as their codes, 01 to 0B.
struct AmpexScreen {
    std::vector<std::string> lines = std::vector<std::string>(24, std::string(80, ' '));
    int line = 0, column = 0;
    int escape = 0;      // 1 after ESC, 2 the row follows, 3 the column, 4 a line drawing letter, 5 and up: characters to drop
    int row = 0;
    void down() { if (line < 23) line++; else { lines.erase(lines.begin()); lines.push_back(std::string(80, ' ')); } }
    void up() { line = line > 0 ? line - 1 : 23; }
    void right() { if (column < 79) column++; else { column = 0; down(); } }
    void left() { if (column > 0) column--; else { column = 79; up(); } }
    void home() { line = column = 0; }
    void clear() { for (auto &l : lines) l.assign(80, ' '); home(); }
    void store(char c) { lines[line][column] = c; right(); }
    void put(unsigned char c) {
        c &= 0x7F;
        if (escape == 0 && c == 0) return;      // a null is dropped when it arrives
        switch (escape) {
        case 1:
            escape = 0;
            switch (c) {
            case '=': escape = 2; break;
            case 'G': escape = 4; break;
            case 'e': escape = 6; break;        // two characters follow
            case 'f': escape = 8; break;        // four
            case '*': case '+': case 'Z': case ':': case ';': clear(); break;
            case 'T': case 't': for (int k = column; k < 80; k++) lines[line][k] = ' '; break;
            case 'Y': case 'y':
                for (int k = column; k < 80; k++) lines[line][k] = ' ';
                for (int l = line + 1; l < 24; l++) lines[l].assign(80, ' ');
                break;
            case 'R': lines.erase(lines.begin() + line); lines.push_back(std::string(80, ' ')); break;
            case 'E': lines.pop_back(); lines.insert(lines.begin() + line, std::string(80, ' ')); break;
            case 'Q': lines[line].insert(column, 1, ' '); lines[line].resize(80); break;
            case 'W': lines[line].erase(column, 1); lines[line].push_back(' '); break;
            }
            return;
        case 2: row = c; escape = 3; return;
        case 3: {
            int r = row - 0x20, k = c - 0x20;
            if (r < 0 || r > 23 || k < 0 || k > 79) home(); else { line = r; column = k; }
            escape = 0;
            return;
        }
        case 4: escape = 0; if (c >= 'A' && c <= 'K') store(c - '@'); return;
        case 0: break;
        default: escape = escape == 5 ? 0 : escape - 1; return;
        }
        if (c == 0x1B) escape = 1;
        else if (c == 0x08) left();
        else if (c == 0x0A) down();
        else if (c == 0x0B) up();
        else if (c == 0x0C) right();
        else if (c == 0x0D) column = 0;
        else if (c == 0x1A) clear();
        else if (c == 0x1E) home();
        else if (c == 0x1F) { column = 0; down(); }
        else if (c >= 0x20 && c < 0x7F) store(c);
    }
    // a line drawing character as text: a corner, tee or cross is a plus sign
    static char shown(char c) { return c == 0x09 ? '-' : c == 0x0A ? '|' : c > 0 && c < 0x20 ? '+' : c; }
    // the lines without the blanks at their ends and without the empty ones at the bottom
    std::string text() const {
        std::string out;
        for (auto l : lines) {
            for (auto &c : l) c = shown(c);
            size_t end = l.find_last_not_of(' ');
            out += (end == std::string::npos ? "" : l.substr(0, end + 1)) + "\n";
        }
        while (out.size() > 1 && out[out.size() - 1] == '\n' && out[out.size() - 2] == '\n') out.pop_back();
        return out;
    }
};

// The 24 lines of 80 characters that console shows after what it was sent.
static std::string screen(const std::string &raw) {
    AmpexScreen s;
    for (unsigned char c : raw) s.put(c);
    return s.text();
}

// What the benches wait for on a console.  A text has come when it is in what
// the console was sent since the last key, whatever blanks and cursor
// movements lie between its characters; or when the screen shows it now and
// did not when that key was typed, because the station sends only the
// characters of a display that differ from what is there.
struct Shown {
    std::string raw;                     // everything the console was sent
    AmpexScreen now;
    size_t typed_from = 0;
    std::string before;                  // the screen at the last key, without blanks
    void put(unsigned char c) { raw.push_back(c); now.put(c); }
    void typed() { typed_from = raw.size(); before = squeeze(now.text()); }
    void clear() { raw.clear(); now = AmpexScreen(); typed_from = 0; before.clear(); }
    bool has(const std::string &want) const {
        if (squeeze(raw.substr(typed_from)).find(want) != std::string::npos) return true;
        return before.find(want) == std::string::npos && squeeze(now.text()).find(want) != std::string::npos;
    }
};

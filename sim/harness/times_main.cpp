// The times of a console, of the Peripheral Expander's tape and printer and
// of the channel pair to the mainframe (sim/tb/ios_times_tb.v), as fast
// devices and with the times of the real ones.
//
//   Vios_times_tb
//
// The bench is Local Memory, the tape's file, and what takes the characters
// and the parcels.  It counts the clocks from a function to the flag that
// ends it, and the clocks from one parcel of the channel pair to the next.
// With the real times
//
//   a character to a display is ten bits at 9,600 baud;
//   a read of the tape takes 8 ms and the record's bytes at 60,000 a second,
//   and 5 ms more when it follows another within that time; a file mark is
//   a parcel long; a rewind covers the bytes and the gaps of 480 bytes that
//   were passed at 160,000 a second;
//   a new line or a new page of the printer takes 50 ms, in graphics mode 3;
//   a parcel of the channel pair follows the one before after 27 clocks.
//
// What is read, printed and sent has to be the same either way.
// Exit status: 0 if everything was as it should be, 1 if not.
#include "Vios_times_tb.h"
#include "verilated.h"

#include <cstdint>
#include <cstdio>
#include <deque>
#include <vector>

static const long MS = 80000;             // clocks, as the modules are built
static const long CHAR = MS * 25 / 24;    // a character at 9,600 baud
static const long PARCEL = MS / 30;       // two bytes of the tape
static const long START = 8 * MS, STOP = 5 * MS;
static const long REWIND = MS / 160;      // a byte's length on the way back
static const long GAP = 480;
static const long LINE = 50 * MS, ROW = 3 * MS;
static const long LINK = 27;

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    Vios_times_tb *top = new Vios_times_tb;
    std::vector<uint16_t> xmem(1 << 16, 0), lmem(1 << 16, 0);
    long clocks = 0;
    int failed = 0;

    // the tape: three records, a file mark and a record
    std::vector<uint8_t> tape;
    std::vector<std::vector<uint8_t>> records;
    auto record = [&](int bytes, int salt) {
        std::vector<uint8_t> r(bytes);
        for (int n = 0; n < bytes; n++) r[n] = (uint8_t)(n * 5 + salt + (n >> 8));
        for (int k = 0; k < 4; k++) tape.push_back(bytes >> (8 * k) & 0xFF);
        tape.insert(tape.end(), r.begin(), r.end());
        for (int k = 0; k < 4; k++) tape.push_back(bytes >> (8 * k) & 0xFF);
        records.push_back(r);
    };
    record(4096, 1); record(4096, 2); record(100, 3);
    for (int k = 0; k < 4; k++) tape.push_back(0);
    record(4096, 4);
    std::vector<uint64_t> tape_words((tape.size() + 7) / 8, 0);
    for (size_t n = 0; n < tape.size(); n++) tape_words[n / 8] |= (uint64_t)tape[n] << (56 - 8 * (n % 8));

    // what takes the characters and the parcels
    std::vector<uint8_t> shown, printed;
    long terminal_slow = 0, terminal_wait = 0;      // clocks the terminal lets a character wait
    std::vector<long> ready_at, resume_at;
    std::vector<uint16_t> sent;
    long resume_delay = 0, resume_in = -1;          // the mainframe's Resume, clocks after a Ready
    std::deque<uint16_t> to_send;
    bool awaiting = false;

    top->clk = 0; top->rst = 1; top->i_real = 0;
    top->c_display = 0; top->c_function = 0; top->c_a = 0; top->c_char_ready = 1;
    top->x_strobe = 0; top->x_function = 0; top->x_a = 0;
    top->x_dma_ack = 0; top->x_dma_rdata = 0; top->x_tape_ack = 0; top->x_tape_data = 0; top->x_tape_bytes = tape.size();
    top->x_print_ready = 1;
    top->l_in = 0; top->l_out = 0; top->l_function = 0; top->l_a = 0;
    top->l_dma_ack = 0; top->l_dma_rdata = 0;
    top->l_cpu_ready = 0; top->l_cpu_parcel = 0; top->l_cpu_disconnect = 0; top->l_resume = 0;
    top->eval();

    auto clock = [&]() {
        top->eval();
        // Local Memory: a request is taken at once, a parcel read is there in the next clock
        bool xreq = top->x_dma_req, xwe = top->x_dma_we; uint16_t xaddr = top->x_dma_addr, xwdata = top->x_dma_wdata;
        bool lreq = top->l_dma_req, lwe = top->l_dma_we; uint16_t laddr = top->l_dma_addr, lwdata = top->l_dma_wdata;
        bool tape_req = top->x_tape_req && !top->x_tape_ack; uint32_t tape_addr = top->x_tape_addr;
        if (top->c_char_valid && top->c_char_ready) shown.push_back(top->c_char);
        if (top->x_print_valid && top->x_print_ready) printed.push_back(top->x_print);
        bool ready = top->l_ready, resume = top->l_cpu_resume, valid = top->c_char_valid;
        if (ready) { ready_at.push_back(clocks); sent.push_back(top->l_parcel); resume_in = resume_delay; }
        if (resume) { resume_at.push_back(clocks); awaiting = false; }
        top->x_dma_ack = xreq; top->l_dma_ack = lreq;
        top->eval();
        top->clk = 1; top->eval();
        if (xreq) { if (xwe) xmem[xaddr] = xwdata; else top->x_dma_rdata = xmem[xaddr]; }
        if (lreq) { if (lwe) lmem[laddr] = lwdata; else top->l_dma_rdata = lmem[laddr]; }
        top->x_tape_ack = tape_req;
        if (tape_req) top->x_tape_data = tape_addr < tape_words.size() ? tape_words[tape_addr] : 0;
        top->c_display = 0; top->x_strobe = 0; top->l_in = 0; top->l_out = 0;
        // the mainframe answers a parcel, and sends its own the clock after the answer to the last
        top->l_resume = 0;
        if (resume_in >= 0) { if (resume_in == 0) top->l_resume = 1; resume_in--; }
        top->l_cpu_ready = 0;
        if (!awaiting && !to_send.empty()) { top->l_cpu_parcel = to_send.front(); to_send.pop_front(); top->l_cpu_ready = 1; awaiting = true; }
        // the terminal
        if (!valid) terminal_wait = terminal_slow;
        else if (terminal_wait > 0) terminal_wait--;
        top->c_char_ready = terminal_wait == 0;
        top->eval();
        top->clk = 0; top->eval();
        clocks++;
    };
    auto wait = [&](long n) { while (n-- > 0) clock(); };
    auto check = [&](const char *what, long got, long low, long high) {
        bool ok = got >= low && got <= high;
        if (!ok) failed++;
        printf("%-64s %9ld clocks%s\n", what, got, ok ? "" : "  OUT OF RANGE");
        if (!ok) printf("    it should be %ld to %ld\n", low, high);
    };
    auto same = [&](const char *what, bool ok) {
        if (!ok) { failed++; printf("%s: WRONG\n", what); }
    };

    // ---- the expander: a function, and one that is followed by its microsecond
    auto xfn = [&](int fn, unsigned a) { top->x_function = fn; top->x_a = a; top->x_strobe = 1; clock(); };
    auto xdel = [&](int fn, unsigned a) { xfn(fn, a); wait(100); };
    // clocks until the device asks for its interrupt, counted from the clock behind the function
    auto until_ask = [&](long limit) {
        long n = 0;
        while (!top->x_ask && n < limit) { clock(); n++; }
        return n;
    };
    auto tape_status = [&]() { xdel(1, 0); xfn(010, 0); return (unsigned)top->x_data; };
    auto read_tape = [&](unsigned at, int count) {
        xdel(016, (uint16_t)-count);   // C: the count
        xdel(015, at);                 // B: the address
        xdel(014, 0);                  // A: read
        xfn(017, 1);                   // Start
    };
    auto holds = [&](const char *what, unsigned at, const std::vector<uint8_t> &r, int parcels) {
        for (int n = 0; n < parcels; n++) {
            uint16_t want = (uint16_t)(r[2 * n] << 8 | (2 * n + 1 < (int)r.size() ? r[2 * n + 1] : 0));
            if (xmem[(uint16_t)(at + n)] != want) { printf("%s: parcel %d is %04x\n", what, n, xmem[(uint16_t)(at + n)]); failed++; return; }
        }
    };

    for (int real = 0; real < 2; real++) {
        top->rst = 1; wait(4); top->rst = 0; wait(4);
        top->i_real = real;
        printf("---- %s\n", real ? "with the times of the real devices" : "fast");

        // ---- the console
        shown.clear();
        auto show = [&](int c) { top->c_function = 014; top->c_a = c; top->c_display = 1; clock(); long n = 0; while (!top->c_done && n < 10 * CHAR) { clock(); n++; } return n; };
        long t = show('A');
        if (real) check("a character to a display", t, CHAR, CHAR + 3); else check("a character to a display", t, 0, 3);
        terminal_slow = 200000;
        t = show('B');
        check("a character the terminal lets wait 200,000 clocks", t, 200000, 200004);
        terminal_slow = 0;
        same("the characters shown", shown == std::vector<uint8_t>({'A', 'B'}));

        // ---- the tape
        xfn(05, 022);                  // the tape
        xdel(07, 2);                   // the devices may interrupt
        std::fill(xmem.begin(), xmem.end(), 0xEEEE);
        read_tape(0x1000, 2048);
        t = until_ask(20 * 80 * MS);
        long first = clocks;
        if (real) check("a read of 4,096 bytes from the load point", t, START + 2048 * PARCEL, START + 2048 * PARCEL + 10);
        else check("a read of 4,096 bytes from the load point", t, 2048, 30000);
        holds("the first record", 0x1000, records[0], 2048);
        same("the status behind a record", tape_status() == 0x0005);
        xdel(017, 2);                  // Clear
        read_tape(0x2000, 2048);
        until_ask(20 * 80 * MS);
        if (real) check("the next read, from Done to Done, the tape still stopping", clocks - first, STOP + START + 2048 * PARCEL, STOP + START + 2048 * PARCEL + 10);
        holds("the second record", 0x2000, records[1], 2048);
        xdel(017, 2);
        wait(6 * MS);
        read_tape(0x3000, 2048);
        t = until_ask(20 * 80 * MS);
        if (real) check("a read of a record of 100 bytes", t, START + 50 * PARCEL, START + 50 * PARCEL + 10);
        else check("a read of a record of 100 bytes", t, 50, 1000);
        holds("the short record", 0x3000, records[2], 50);
        same("nothing is stored behind a short record", xmem[0x3000 + 50] == 0xEEEE);
        xdel(017, 2);
        wait(6 * MS);
        read_tape(0x3800, 2048);
        t = until_ask(20 * 80 * MS);
        if (real) check("a read that finds a file mark", t, START + PARCEL, START + PARCEL + 10);
        else check("a read that finds a file mark", t, 0, 20);
        same("the status behind a file mark", tape_status() == 0x8105);
        same("nothing is stored from a file mark", xmem[0x3800] == 0xEEEE);
        xdel(017, 2);
        wait(6 * MS);
        read_tape(0x4000, 10);
        t = until_ask(20 * 80 * MS);
        if (real) check("a read of 10 parcels of a record of 4,096 bytes", t, START + 2048 * PARCEL, START + 2048 * PARCEL + 10);
        else check("a read of 10 parcels of a record of 4,096 bytes", t, 10, 200);
        holds("the ten parcels", 0x4000, records[3], 10);
        same("nothing is stored behind the count", xmem[0x4000 + 10] == 0xEEEE);
        xdel(017, 2);
        wait(6 * MS);
        xdel(014, 010);                // A: rewind
        xfn(017, 1);
        t = until_ask(200 * 80 * MS);
        long far = 3 * (4096 + GAP) + (100 + GAP) + GAP;
        if (real) check("a rewind over four records and a file mark", t, far * REWIND, far * REWIND + 10);
        else check("a rewind over four records and a file mark", t, 0, 10);
        same("the status at the load point", tape_status() == 0x0085);
        xdel(017, 2);
        wait(6 * MS);
        read_tape(0x5000, 2048);
        t = until_ask(20 * 80 * MS);
        if (real) check("a read behind the rewind", t, START + 2048 * PARCEL, START + 2048 * PARCEL + 10);
        holds("the first record again", 0x5000, records[0], 2048);
        xdel(017, 2);

        // ---- the printer
        printed.clear();
        xfn(05, 017);
        xmem[0x6000] = 'A' << 8 | 'B'; xmem[0x6001] = 'C' << 8 | 'D';
        xdel(015, (uint16_t)-2);       // B: two parcels
        xfn(016, 0x6000);              // C: print from there
        check("four characters to the printer", until_ask(MS), 0, 40);
        xdel(017, 2);
        xdel(014, 3);                  // A: new line
        xfn(017, 4);                   // Pulse
        t = until_ask(200 * MS);
        if (real) check("a new line", t, LINE, LINE + 10); else check("a new line", t, 0, 10);
        xdel(017, 2);
        xdel(014, 1); xdel(017, 4);    // graphics mode
        xdel(014, 3);
        xfn(017, 4);
        t = until_ask(200 * MS);
        if (real) check("a new line in graphics mode: a row of dots", t, ROW, ROW + 10); else check("a new line in graphics mode: a row of dots", t, 0, 10);
        xdel(017, 2);
        xdel(014, 4); xdel(017, 4);    // text mode
        xdel(014, 0);                  // A: new page
        xfn(017, 4);
        t = until_ask(200 * MS);
        if (real) check("a new page", t, LINE, LINE + 10); else check("a new page", t, 0, 10);
        xdel(017, 2);
        same("what was printed", printed == std::vector<uint8_t>({'A', 'B', 'C', 'D', '\n', '\n', 0x0C}));

        // ---- the channel pair: eight parcels each way
        auto spacing = [](const std::vector<long> &at, long &low, long &high) {
            low = 1 << 30; high = 0;
            for (size_t n = 1; n < at.size(); n++) { long d = at[n] - at[n - 1]; if (d < low) low = d; if (d > high) high = d; }
        };
        long low, high;
        for (int slow = 0; slow < 2; slow++) {
            for (int n = 0; n < 8; n++) lmem[0x100 + n] = 0xA000 + 16 * slow + n;
            ready_at.clear(); sent.clear();
            resume_delay = slow ? 40 : 0;
            top->l_function = 2; top->l_a = 8; top->l_out = 1; clock();
            top->l_function = 1; top->l_a = 0x100; top->l_out = 1; clock();
            for (long n = 0; !(top->l_done >> 1 & 1) && n < 10000; n++) clock();
            spacing(ready_at, low, high);
            same("eight parcels are sent", sent.size() == 8);
            for (size_t n = 0; n < sent.size(); n++) same("a parcel sent", sent[n] == 0xA000 + 16 * slow + n);
            if (slow) { check("from parcel to parcel, the mainframe answering after 40 clocks: least", low, 41, 48); check("    and most", high, 41, 48); }
            else if (real) { check("from parcel to parcel to the mainframe: least", low, LINK, LINK); check("    and most", high, LINK, LINK); }
            else { check("from parcel to parcel to the mainframe: least", low, 2, 8); check("    and most", high, 2, 8); }
            top->l_function = 0; top->l_out = 1; clock();
        }
        resume_delay = 0;
        resume_at.clear();
        top->l_function = 2; top->l_a = 8; top->l_in = 1; clock();
        top->l_function = 1; top->l_a = 0x200; top->l_in = 1; clock();
        for (int n = 0; n < 8; n++) to_send.push_back(0xB000 + n);
        for (long n = 0; !(top->l_done & 1) && n < 10000; n++) clock();
        wait(2);                       // the last answer comes with Done
        spacing(resume_at, low, high);
        same("eight parcels are answered", resume_at.size() == 8);
        for (int n = 0; n < 8; n++) same("a parcel taken", lmem[0x200 + n] == 0xB000 + n);
        if (real) { check("from answer to answer to the mainframe's parcels: least", low, LINK, LINK + 1); check("    and most", high, LINK, LINK + 1); }
        else { check("from answer to answer to the mainframe's parcels: least", low, 2, 8); check("    and most", high, 2, 8); }
        top->l_function = 0; top->l_in = 1; clock();
    }

    printf("%s\n", failed ? "the times bench FAILED" : "the times bench passed");
    delete top;
    return failed ? 1 : 0;
}

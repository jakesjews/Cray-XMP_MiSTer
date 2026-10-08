// The disk drives of the BIOP (rtl/ios/ios_disks.v) by themselves: what they
// read and write, and how long a seek and a sector take, as fast drives and
// with the DD-29's own times.
//
//   Vios_disks
//
// The bench is the BIOP's Local Memory and the images, served as the MiSTer
// framework serves them.  It reserves a drive, seeks, and reads and writes
// sectors, and counts the clocks from a function to the Done flag.  With the
// DD-29's times a seek has to take 15 ms and 65 ms more across all 822
// cylinders, and a sector has to be done when it has next passed under the
// heads: the sectors of a track follow each other 0.922 ms apart, and one
// that has just passed takes a revolution of 16.6 ms.
// Exit status: 0 if everything was as it should be, 1 if not.
#include "Vios_disks.h"
#include "verilated.h"
#include "ios_media.h"

#include <cstdint>
#include <cstdio>
#include <vector>

static const long MS = 80000;                    // clocks, as the module is built
static const long SECTOR = 166 * MS / 180;       // one of 18 sectors in 16.6 ms
static const long SETTLE = 200;

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    Vios_disks *top = new Vios_disks;
    SectorDisks sd(9, 4096);
    std::vector<uint16_t> mem(1 << 16, 0);
    long clocks = 0;
    int failed = 0;

    top->clk = 0; top->rst = 1; top->i_real = 0; top->i_strobe = 0;
    top->i_drive = 0; top->i_function = 0; top->i_a = 0;
    top->i_dma_ack = 0; top->i_dma_rdata = 0;
    top->i_sd_ack = 0; top->i_sd_buff_addr = 0; top->i_sd_buff_dout = 0; top->i_sd_buff_wr = 0;
    top->eval();

    auto clock = [&]() {
        top->eval();
        // Local Memory: a request is taken at once, a parcel read is there in the next clock
        bool req = top->o_dma_req, we = top->o_dma_we; uint16_t addr = top->o_dma_addr, wdata = top->o_dma_wdata;
        unsigned rd = top->o_sd_rd, wr = top->o_sd_wr; uint32_t lba = top->o_sd_lba; uint8_t din = top->o_sd_buff_din;
        top->i_dma_ack = req;
        top->eval();
        top->clk = 1; top->eval();
        if (req) { if (we) mem[addr] = wdata; else top->i_dma_rdata = mem[addr]; }
        sd.clock(rd, wr, lba, din);
        top->i_sd_ack = sd.ack; top->i_sd_buff_addr = sd.buff_addr; top->i_sd_buff_dout = sd.buff_dout; top->i_sd_buff_wr = sd.buff_wr;
        top->i_strobe = 0;
        top->eval();
        top->clk = 0; top->eval();
        clocks++;
    };
    // a function for a drive, in one clock
    auto function = [&](int drive, int fn, unsigned a) {
        top->i_drive = drive; top->i_function = fn; top->i_a = a; top->i_strobe = 1;
        clock();
    };
    // clocks until the drive's Done flag is up, counted from the clock behind the function
    auto until_done = [&](int drive, long limit) {
        long n = 0;
        while (!(top->o_done >> drive & 1) && n < limit) { clock(); n++; }
        return n;
    };
    auto check = [&](const char *what, long got, long low, long high) {
        bool ok = got >= low && got <= high;
        if (!ok) failed++;
        printf("%-58s %9ld clocks%s\n", what, got, ok ? "" : low == high ? "  WRONG" : "  OUT OF RANGE");
        if (!ok) printf("    it should be %ld%s%ld\n", low, low == high ? " = " : " to ", high);
    };
    // the image of a sector: every parcel tells drive, sector and place
    auto image = [&](int drive, long sector, uint16_t salt) {
        std::vector<uint8_t> bytes(4096);
        for (int n = 0; n < 2048; n++) { uint16_t p = (uint16_t)(sector * 31 + n * 7 + drive + salt); bytes[2 * n] = p >> 8; bytes[2 * n + 1] = p & 0xFF; }
        sd.written[drive][(uint32_t)(sector * 8)] = bytes;
    };
    auto sector_of = [](long cylinder, long head, long sector) { return (cylinder * 10 + head) * 18 + sector; };
    auto holds = [&](const char *what, int drive, long sector, uint16_t salt, uint16_t at) {
        for (int n = 0; n < 2048; n++)
            if (mem[(uint16_t)(at + n)] != (uint16_t)(sector * 31 + n * 7 + drive + salt)) {
                printf("%s: parcel %d of the sector is %04x\n", what, n, mem[(uint16_t)(at + n)]);
                failed++;
                return;
            }
    };

    for (int n = 0; n < 4; n++) clock();
    top->rst = 0;
    for (int n = 0; n < 4; n++) clock();

    for (int real = 0; real < 2; real++) {
        top->i_real = real;
        int d = real ? 5 : 2;
        printf("---- drive %d, %s\n", d, real ? "with the DD-29's times" : "fast");
        function(d, 0, 0);
        function(d, 1, 01000);                              // reserve the unit
        check("a mode selection", until_done(d, 1000), SETTLE - 2, SETTLE + 2);
        function(d, 4, 3);                                  // head group 3
        function(d, 5, 300);                                // from cylinder 0 to 300
        long seek = until_done(d, 10 * 80 * MS);
        if (real) check("a seek across 300 cylinders", seek, 15 * MS + 300 * (65 * MS / 822) - 2, 15 * MS + 300 * (65 * MS / 822) + 2);
        else check("a seek across 300 cylinders", seek, SETTLE - 2, SETTLE + 2);
        function(d, 5, 301);
        seek = until_done(d, 10 * 80 * MS);
        if (real) check("a seek to the next cylinder", seek, 15 * MS + 65 * MS / 822 - 2, 15 * MS + 65 * MS / 822 + 2);
        else check("a seek to the next cylinder", seek, SETTLE - 2, SETTLE + 2);
        function(d, 5, 301);
        check("a seek to the cylinder the heads are on", until_done(d, 10 * 80 * MS), SETTLE - 2, SETTLE + 2);
        function(d, 5, 822);
        seek = until_done(d, 10 * 80 * MS);
        if (real) check("a seek to the last cylinder", seek, 15 * MS + 521 * (65 * MS / 822) - 2, 15 * MS + 521 * (65 * MS / 822) + 2);
        function(d, 5, 0);
        seek = until_done(d, 10 * 80 * MS);
        if (real) check("a seek across the whole disk", seek, 80 * MS - 822, 80 * MS);
        function(d, 5, 301);
        until_done(d, 10 * 80 * MS);

        // four sectors that follow each other, each asked for when the one before is done
        for (int s = 4; s <= 8; s++) image(d, sector_of(301, 3, s), 0x100 * real);
        long done_at[5] = {0};
        for (int s = 4; s <= 7; s++) {
            function(d, 014, 0x1000 + 0x800 * (s - 4));
            function(d, 2, s);
            long took = until_done(d, 40 * SECTOR);
            done_at[s - 4] = clocks;
            if (s == 4) check("the first sector", took, 1, real ? 19 * SECTOR : SECTOR);
            else if (real) check("the next sector of the track, from Done to Done", done_at[s - 4] - done_at[s - 5], SECTOR, SECTOR);
            else check("the next sector of the track", took, 1, SECTOR);
            holds("read", d, sector_of(301, 3, s), 0x100 * real, 0x1000 + 0x800 * (s - 4));
        }
        // the sector just read again: it has passed, so a revolution less what the asking took
        function(d, 014, 0x4000);
        function(d, 2, 7);
        long again = until_done(d, 40 * SECTOR);
        if (real) check("the same sector again", again, 18 * SECTOR - 8, 18 * SECTOR);
        holds("read again", d, sector_of(301, 3, 7), 0x100 * real, 0x4000);
        // a sector written and read back
        for (int n = 0; n < 2048; n++) mem[0x6000 + n] = (uint16_t)(0xC000 + n * 3 + real);
        function(d, 014, 0x6000);
        function(d, 3, 9);
        long wrote = until_done(d, 40 * SECTOR);
        if (real) check("a sector written", wrote, 1, 19 * SECTOR);
        function(d, 014, 0x7000);
        function(d, 2, 9);
        until_done(d, 40 * SECTOR);
        for (int n = 0; n < 2048; n++)
            if (mem[0x7000 + n] != (uint16_t)(0xC000 + n * 3 + real)) { printf("written and read back: parcel %d is %04x\n", n, mem[0x7000 + n]); failed++; break; }
        // function 0 while a sector is awaited: neither Busy nor Done afterwards
        function(d, 2, 1);
        for (int n = 0; n < 50; n++) clock();
        function(d, 0, 0);
        for (long n = 0; n < 20 * SECTOR; n++) clock();
        if ((top->o_busy >> d & 1) || (top->o_done >> d & 1)) { printf("function 0 did not end the sector that was asked for\n"); failed++; }
    }
    // a drive that was never selected echoes its buffer, whatever the times
    for (int n = 0; n < 512; n++) mem[0x8000 + n] = (uint16_t)(0x5A00 + n);
    function(7, 014, 0x8000);
    function(7, 3, 0);
    check("an echo written, with the DD-29's times", until_done(7, 40 * SECTOR), 1, 5000);
    function(7, 014, 0x9000);
    function(7, 2, 0);
    check("an echo read", until_done(7, 40 * SECTOR), 1, 5000);
    for (int n = 0; n < 512; n++)
        if (mem[0x9000 + n] != (uint16_t)(0x5A00 + n)) { printf("echo: parcel %d is %04x\n", n, mem[0x9000 + n]); failed++; break; }

    printf("%ld clocks, %d wrong\n", clocks, failed);
    top->final();
    delete top;
    return failed ? 1 : 0;
}

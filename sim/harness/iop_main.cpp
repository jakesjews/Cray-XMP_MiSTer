// The I/O Processor (rtl/ios/iop_cpu.v) against a record of the reference
// model's steps.
//
//   Viop_cpu RECORD [--quiet]
//
// RECORD is what tools/crates/ios/src/replay.rs writes (the format is
// described there): Local Memory, then for every step what the channels
// answered and what the registers held afterwards.  The processor is given the
// same memory and the same answers and must take the same interrupts, send the
// same functions and end every step with the same registers after the same
// number of clocks; at the end its memory, operand registers and exit stack
// must be the model's.
// Exit status: 0 if it did, 1 at the first difference, 2 for a bad record.
#include "Viop_cpu.h"
#include "Viop_cpu___024root.h"
#include "verilated.h"

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

enum { INSTRUCTION, INTERRUPT, SET_MEMORY, MASTER_CLEAR, CHECK_MEMORY, CHECK_OPERAND, CHECK_EXIT_STACK, END, SET_OPERAND, SET_REGISTERS, IDLE };

struct Record {
    uint8_t kind, request;
    uint16_t p, parcel, channel, a_before, value, p_after, a_after, b_after;
    uint8_t e_after, flags_after, clocks;
};

static bool read_record(FILE *f, Record &r) {
    uint8_t b[20];
    if (fread(b, 1, sizeof b, f) != sizeof b) return false;
    auto w = [&](int n) { return (uint16_t)(b[n] | b[n + 1] << 8); };
    r = {b[0], b[1], w(2), w(4), w(6), w(8), w(10), w(12), w(14), w(16), (uint8_t)(b[18] & 15), b[19], (uint8_t)(b[18] >> 4)};
    return true;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    std::string path;
    bool quiet = false;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        if (a == "--quiet") quiet = true;
        else path = a;
    }
    FILE *f = path.empty() ? nullptr : fopen(path.c_str(), "rb");
    if (!f) { fprintf(stderr, "cannot read record '%s'\n", path.c_str()); return 2; }

    std::vector<uint16_t> mem(1 << 16, 0);
    Viop_cpu *top = new Viop_cpu;
    auto *in = top->rootp;
    top->clk = 0; top->rst = 1;
    top->i_mem_rdata = 0; top->i_data = 0; top->i_busy = 0; top->i_done = 0; top->i_request = 0;
    top->eval();

    long clocks = 0, steps = 0, interrupts = 0, functions = 0, checked = 0;
    bool strobed = false;           // a function was sent in the current step
    int waited = 0;                 // clocks since the current step began
    Record now{};                   // the step under way
    bool running = false, ended = false;

    auto clock = [&]() {
        uint16_t addr = top->o_mem_addr, wdata = top->o_mem_wdata; bool we = top->o_mem_we;
        top->clk = 1; top->eval();
        if (we) mem[addr] = wdata;
        top->i_mem_rdata = mem[addr];
        top->eval();
        top->clk = 0; top->eval();
        clocks++;
    };
    auto fail = [&](const char *what, unsigned got, unsigned want) {
        printf("step %ld (P %04x, parcel %06o%s): %s is %x, the model has %x\n", steps + 1, now.p, now.parcel,
               now.kind == INTERRUPT ? ", an interrupt" : "", what, got, want);
        return 1;
    };

    // registers that keep their value over Master Clear start as the model's: zero
    in->iop_cpu__DOT__a = 0; in->iop_cpu__DOT__b = 0; in->iop_cpu__DOT__c = 0;
    for (int i = 0; i < 512; i++) in->iop_cpu__DOT__or_mem[i] = 0;
    for (int i = 0; i < 4; i++) clock();

    while (!ended) {
        // records up to the next step are acted on at once
        while (!running && !ended) {
            Record r;
            if (!read_record(f, r)) { printf("the record ends without its end\n"); return 2; }
            switch (r.kind) {
            case SET_MEMORY: mem[r.p] = r.parcel; break;
            case SET_OPERAND: in->iop_cpu__DOT__or_mem[r.p & 511] = r.parcel; break;
            case SET_REGISTERS: in->iop_cpu__DOT__a = r.p; in->iop_cpu__DOT__b = r.parcel & 0777; in->iop_cpu__DOT__c = r.parcel >> 15; break;
            case MASTER_CLEAR: top->rst = 1; clock(); clock(); break;
            case IDLE:
                // held by Master Clear with no channel asking: nothing may happen
                top->rst = 0; top->i_request = 0; top->eval();
                for (int n = 0; n < r.p; n++) {
                    clock();
                    if (top->o_step || top->o_strobe || top->o_mem_we) { printf("after %ld steps the processor does something while Master Clear holds it\n", steps); return 1; }
                }
                break;
            case CHECK_MEMORY:
                checked++;
                if (mem[r.p] != r.parcel) { printf("at the end Local Memory %04x is %04x, the model has %04x\n", r.p, mem[r.p], r.parcel); return 1; }
                break;
            case CHECK_OPERAND:
                checked++;
                if (in->iop_cpu__DOT__or_mem[r.p & 511] != r.parcel) { printf("at the end operand register %o is %04x, the model has %04x\n", r.p, in->iop_cpu__DOT__or_mem[r.p & 511], r.parcel); return 1; }
                break;
            case CHECK_EXIT_STACK:
                checked++;
                if (in->iop_cpu__DOT__xs[r.p & 15] != r.parcel) { printf("at the end exit stack location %o is %04x, the model has %04x\n", r.p, in->iop_cpu__DOT__xs[r.p & 15], r.parcel); return 1; }
                break;
            case END: ended = true; break;
            case INSTRUCTION: case INTERRUPT: {
                now = r; running = true; strobed = false; waited = 0;
                // what the channels answer during this step.  A flag test is
                // given the flag it found and the opposite on the other line.
                unsigned fn = r.parcel >> 9;
                bool busy_test = fn & 1;
                top->i_request = r.request;
                top->i_data = r.value;
                top->i_busy = busy_test ? (r.value & 1) : !(r.value & 1);
                top->i_done = busy_test ? !(r.value & 1) : (r.value & 1);
                break;
            }
            default: printf("record kind %d\n", r.kind); return 2;
            }
        }
        if (ended) break;
        top->rst = 0;
        top->eval();
        // a function goes out in the first clock of its instruction, which this is
        // when the step has just been given its answers
        bool strobe = top->o_strobe;
        unsigned s_channel = top->o_channel, s_function = top->o_function, s_a = top->o_a;
        clock();
        waited++;
        if (strobe) {
            functions++;
            if (strobed) return fail("a second function, on channel", s_channel, 0);
            strobed = true;
            if (now.kind != INSTRUCTION) return fail("a function sent, on channel", s_channel, 0);
            if (s_channel != now.channel) return fail("the channel of the function", s_channel, now.channel);
            if (s_function != ((now.parcel >> 9) & 017)) return fail("the function", s_function, (now.parcel >> 9) & 017);
            if (s_a != now.a_before) return fail("the accumulator sent", s_a, now.a_before);
        }
        if (top->o_step) {
            if ((bool)top->o_step_interrupt != (now.kind == INTERRUPT)) return fail("interrupt taken", top->o_step_interrupt, now.kind == INTERRUPT);
            unsigned fn = now.parcel >> 9;
            bool sends = now.kind == INSTRUCTION && fn >= 0140 && now.channel >= 4 && now.channel < 050;
            if (sends != strobed) return fail("function sent", strobed, sends);
            if (top->o_p != now.p_after) return fail("P", top->o_p, now.p_after);
            if (top->o_a != now.a_after) return fail("A", top->o_a, now.a_after);
            if (top->o_b != now.b_after) return fail("B", top->o_b, now.b_after);
            if (top->o_e != now.e_after) return fail("E", top->o_e, now.e_after);
            if (top->o_flags != now.flags_after) return fail("the flags (held, boundary, fetch request, I delayed, I, C)", top->o_flags, now.flags_after);
            if (waited != now.clocks) return fail("the clocks it took", waited, now.clocks);
            steps++;
            if (now.kind == INTERRUPT) interrupts++;
            running = false;
        } else if (waited > 32) {
            return fail("clocks without the end of the step", waited, 0);
        }
    }
    if (!quiet || steps == 0)
        printf("%ld steps (%ld interrupts, %ld functions) in %ld clocks, %ld values checked at the end: as the model\n",
               steps, interrupts, functions, clocks, checked);
    top->final();
    delete top;
    return steps > 0 ? 0 : 2;
}

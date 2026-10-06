// The bridges between the CPU's clock and the I/O Subsystem's
// (rtl/xmp_bridge.v) against what they promise.
//
//   Vxmp_bridge_tb [CASES] [--seed N]
//
// Each case gives the two sides clocks of its own: the machine's 9.524 and
// 12.5 ns, one clock for both, or two periods picked at random, with any
// phase between them.
//
// Pulses are sent each way, no closer than three periods of the slower clock.
// Each must come out once, one clock long, and soon.  A level must be seen on
// the other side once it has stood for a while.
//
// On the memory port one side asks for words to be read and written, one
// request behind the other or with pauses, and takes some requests back
// before they are acknowledged.  The other side answers after a random time,
// as rtl/mister/ddr3_ports.sv does.  Every request that is held must be
// carried out once, in order, with its address and data, and be acknowledged
// once with the word that was read.  A request that is taken back may be
// carried out, once, but is not acknowledged; the acknowledge that was already
// on its way in the clock the request went is let through.  Now and then both
// sides are reset in the middle of all that and must go on afterwards.
#include "Vxmp_bridge_tb.h"
#include "verilated.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <random>
#include <vector>

struct Op {
    bool we; uint32_t addr; uint64_t wdata, rdata;
    bool back = false, carried = false, acked = false;
};

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    int cases = 300; unsigned seed = 1;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--seed") && i + 1 < argc) seed = atoi(argv[++i]);
        else cases = atoi(argv[i]);
    }
    std::mt19937_64 rng(seed);
    auto pick = [&](long a, long b) { return a + (long)(rng() % (uint64_t)(b - a + 1)); };
    long pulses = 0, words = 0, taken_back = 0, resets = 0;

    for (int n = 0; n < cases; n++) {
        auto *top = new Vxmp_bridge_tb;
        // the clocks, in picoseconds: f is the CPU's side, s the I/O Subsystem's
        long pf, ps; int kind = (int)pick(0, 5);
        if (kind == 0) pf = ps = pick(3000, 30000);
        else if (kind == 1) { pf = 9524; ps = 12500; }
        else { pf = pick(3000, 30000); ps = pick(3000, 30000); }
        long tf = pick(1, pf), ts = kind == 0 ? tf : pick(1, ps), slow = pf > ps ? pf : ps;
        bool failed = false;
        auto fail = [&](long now, const char *what) { if (!failed) printf("case %d (clocks %ld and %ld ps) at %ld ps: %s\n", n, pf, ps, now, what); failed = true; };

        top->clk = 0; top->clk_ios = 0; top->rst = 1; top->rst_ios = 1;
        top->i_pulse_f = 0; top->i_pulse_s = 0; top->i_level_f = 0;
        top->i_req = 0; top->i_we = 0; top->i_addr = 0; top->i_wdata = 0; top->i_ack = 0; top->i_rdata = 0;
        top->eval();

        // pulses: when each was sent, for each direction, and when the next may go
        std::deque<long> sent_f, sent_s; long next_f = 0, next_s = 0, level_since = 0; int last_pulse_f = 0, last_pulse_s = 0;
        // the memory port
        std::vector<Op> ops; long last_carried = -1, first_live = 0;
        int rstate = 0, rwait = 0, rhold = -1; long cur = -1, crossed = -1;   // the side that asks: 0 idle, 1 asking
        int mstate = 0, mwait = 0; long mop = -1;                             // the side that answers: 0 idle, 1 waiting, 2 acknowledging
        // the run: clocks counted on the slower side; resets at reset_at
        long clocks_f = 0, clocks_s = 0, length = pick(2000, 12000), reset_at = pick(0, 2) ? pick(200, 1500) : -1;
        bool quiet = false, in_reset = true, ending = false; long reset_until = 8 * slow, end_at = 0;

        for (long guard = 0; guard < 4000000 && !failed; guard++) {
            long now = tf < ts ? tf : ts; bool ef = tf == now, es = ts == now;
            // what each side sees before its clock edge
            bool o_pulse_f = top->o_pulse_f, o_req = top->o_req, o_we = top->o_we; uint32_t o_addr = top->o_addr; uint64_t o_wdata = top->o_wdata;
            bool o_pulse_s = top->o_pulse_s, o_level_s = top->o_level_s, o_ack = top->o_ack; uint64_t o_rdata = top->o_rdata;
            if (ef) top->clk = 1;
            if (es) top->clk_ios = 1;
            top->eval();

            // resets: both sides, when no pulse is on its way
            if (in_reset && now >= reset_until) { in_reset = false; quiet = false; top->rst = 0; top->rst_ios = 0; next_f = next_s = now + 8 * slow; }
            if (!in_reset && !ending && reset_at >= 0 && clocks_s >= reset_at) quiet = true;
            if (quiet && !in_reset && sent_f.empty() && sent_s.empty() && now >= next_f && now >= next_s && pick(0, 20) == 0) {
                in_reset = true; reset_until = now + pick(4, 12) * slow; reset_at = pick(0, 1) ? clocks_s + pick(200, 1500) : -1; resets++;
                top->rst = 1; top->rst_ios = 1;
                // nothing asked before counts any more
                top->i_req = 0; top->i_ack = 0; rstate = 0; rwait = (int)pick(0, 6); cur = -1; crossed = -1; mstate = 0; mop = -1;
                first_live = (long)ops.size(); last_carried = first_live - 1;
            }

            if (ef) {
                clocks_f++;
                // a pulse from the other side
                if (o_pulse_f && clocks_f > 4) {
                    if (sent_s.empty()) fail(now, "a pulse to the CPU's side that was not sent");
                    else { if (now - sent_s.front() > ps + 5 * pf) fail(now, "a pulse to the CPU's side came late"); sent_s.pop_front(); pulses++; }
                    if (last_pulse_f) fail(now, "a pulse to the CPU's side two clocks long");
                }
                last_pulse_f = o_pulse_f;
                top->i_pulse_f = 0;
                if (!in_reset && !quiet && !ending && now >= next_f && pick(0, 3) == 0) {
                    top->i_pulse_f = 1; sent_f.push_back(now); next_f = now + 3 * slow + pf + (pick(0, 5) ? 0 : pick(0, 40) * pf);
                }
                if (!in_reset && pick(0, 60) == 0) { top->i_level_f = !top->i_level_f; level_since = now; }

                // the memory: one request at a time, not looked at in the clock of its acknowledge
                top->i_ack = 0;
                if (in_reset) { mstate = 0; }
                else if (mstate == 0) {
                    if (o_req) {
                        mop = -1;
                        for (long k = last_carried + 1; k < (long)ops.size(); k++)
                            if (ops[k].addr == o_addr) { mop = k; break; }
                        if (mop < 0) fail(now, "a request to memory that was not made, or made before");
                        else {
                            for (long k = last_carried + 1; k < mop; k++) if (!ops[k].back) fail(now, "a request was left out");
                            if (ops[mop].we != o_we || (o_we && ops[mop].wdata != o_wdata)) fail(now, "a request to memory with the wrong direction or data");
                            mstate = 1; mwait = (int)(pick(0, 9) ? pick(1, 12) : pick(13, 80));
                        }
                    }
                } else if (mstate == 1) {
                    if (!o_req || o_addr != ops[mop].addr || o_we != ops[mop].we) fail(now, "a request to memory was not held");
                    if (--mwait <= 0) {
                        ops[mop].carried = true; last_carried = mop; ops[mop].rdata = rng() | 1; words++;
                        top->i_ack = 1; top->i_rdata = ops[mop].rdata; mstate = 2;
                    }
                } else mstate = 0;
                tf += pf;
            }

            if (es) {
                clocks_s++;
                if (o_pulse_s && clocks_s > 4) {
                    if (sent_f.empty()) fail(now, "a pulse to the I/O Subsystem's side that was not sent");
                    else { if (now - sent_f.front() > pf + 5 * ps) fail(now, "a pulse to the I/O Subsystem's side came late"); sent_f.pop_front(); pulses++; }
                    if (last_pulse_s) fail(now, "a pulse to the I/O Subsystem's side two clocks long");
                }
                last_pulse_s = o_pulse_s;
                top->i_pulse_s = 0;
                if (!in_reset && !quiet && !ending && now >= next_s && pick(0, 3) == 0) {
                    top->i_pulse_s = 1; sent_s.push_back(now); next_s = now + 3 * slow + ps + (pick(0, 5) ? 0 : pick(0, 40) * ps);
                }
                if (!in_reset && now - level_since > pf + 4 * ps && o_level_s != top->i_level_f) fail(now, "the level did not arrive");

                // the side that asks
                bool again = false;
                if (o_ack && !in_reset && clocks_s > 4) {
                    if (rstate == 1 && ops[cur].carried && !ops[cur].acked) {
                        ops[cur].acked = true;
                        if (!ops[cur].we && o_rdata != ops[cur].rdata) fail(now, "the wrong word was read");
                        top->i_req = 0; rstate = 0; again = pick(0, 1); rwait = (int)(pick(0, 3) ? pick(0, 5) : pick(6, 60));
                    } else if (crossed >= 0 && ops[crossed].carried && !ops[crossed].acked) ops[crossed].acked = true;
                    else fail(now, "an acknowledge with no request standing that was carried out");
                }
                crossed = -1;
                if (in_reset) { }
                else if (rstate == 1 && !ops[cur].acked) {
                    if (rhold >= 0 && rhold-- == 0) {
                        // taken back; it stays away for a clock at least
                        ops[cur].back = true; taken_back++; crossed = cur;
                        top->i_req = 0; rstate = 0; rwait = (int)pick(1, 30);
                    }
                } else if (rstate == 0 && !ending && (again || rwait-- <= 0)) {
                    Op op; op.we = pick(0, 1); op.addr = (uint32_t)(ops.size() & 0xFFFFFF); op.wdata = rng(); op.rdata = 0;
                    ops.push_back(op); cur = (long)ops.size() - 1;
                    top->i_req = 1; top->i_we = op.we; top->i_addr = op.addr; top->i_wdata = op.wdata;
                    rstate = 1; rhold = pick(0, 4) ? -1 : (int)pick(0, 40);
                }
                ts += ps;
            }
            top->clk = 0; top->clk_ios = 0; top->eval();

            // the end: nothing new, and what is on its way arrives
            if (!ending && clocks_s >= length && !in_reset) { ending = true; end_at = now + 400 * slow; }
            if (ending && now >= end_at) break;
        }
        if (!failed) {
            if (!sent_f.empty() || !sent_s.empty()) fail(0, "a pulse was lost");
            for (long k = first_live; k < (long)ops.size(); k++)
                if (!ops[k].back && !(ops[k].carried && ops[k].acked)) { fail(0, "a request was not carried out and acknowledged"); break; }
        }
        top->final(); delete top;
        if (failed) return 1;
    }
    printf("%d cases: %ld pulses, %ld words read and written, %ld requests taken back, %ld resets\n", cases, pulses, words, taken_back, resets);
    return 0;
}

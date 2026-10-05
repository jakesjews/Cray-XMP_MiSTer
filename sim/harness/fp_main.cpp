// Unit bench for the floating point pipelines against vector files written by
// the reference model (tools/crates/fp, example gen_vectors).
//
//   Vfp_tb FILE.vec [...] [--gaps] [--max-print N]
// Each line is "OP A B RESULT FLAGS" in hex.  Vectors are fed one per clock, or
// with random idle clocks between them with --gaps, and every result is checked
// when it comes out of its unit.
#include "Vfp_tb.h"
#include "verilated.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <random>
#include <string>
#include <vector>

struct Vec { int unit; bool sub; int kind; uint64_t a, b, r; unsigned flags; std::string op; long line; };

// unit 0 add (6 clocks), 1 multiply (7), 2 reciprocal (14)
static int unit_of(const std::string &op, bool &sub, int &kind) {
    sub = (op == "sub"); kind = 0;
    if (op == "add" || op == "sub") return 0;
    if (op == "mul")   { kind = 0; return 1; }
    if (op == "mulh")  { kind = 1; return 1; }
    if (op == "mulr")  { kind = 2; return 1; }
    if (op == "mul2m") { kind = 3; return 1; }
    if (op == "recip") return 2;
    return -1;
}
static const int LATENCY[] = {6, 7, 14};

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    std::vector<std::string> files;
    bool gaps = false; int max_print = 10;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        if (a == "--gaps") gaps = true;
        else if (a == "--max-print") max_print = atoi(argv[++i]);
        else files.push_back(a);
    }
    std::vector<Vec> vecs; long skipped = 0;
    for (auto &path : files) {
        FILE *f = fopen(path.c_str(), "r");
        if (!f) { fprintf(stderr, "cannot read %s\n", path.c_str()); return 2; }
        char buf[512]; long ln = 0;
        while (fgets(buf, sizeof buf, f)) {
            ln++;
            if (buf[0] == '#' || buf[0] == '\n') continue;
            char op[32]; unsigned long long a, b, r; unsigned fl;
            if (sscanf(buf, "%31s %llx %llx %llx %x", op, &a, &b, &r, &fl) != 5) continue;
            Vec v; v.op = op; v.a = a; v.b = b; v.r = r; v.flags = fl; v.line = ln;
            v.unit = unit_of(v.op, v.sub, v.kind);
            if (v.unit < 0) { skipped++; continue; }
            vecs.push_back(v);
        }
        fclose(f);
    }

    Vfp_tb *top = new Vfp_tb;
    top->clk = 0; top->eval();                   // settle before the first clock edge
    std::mt19937 rng(1);
    struct Pending { long due; const Vec *v; };
    std::deque<Pending> pending;
    long t = 0, errors = 0, checked = 0;
    size_t next = 0;
    auto tick = [&]() { top->clk = 1; top->eval(); top->clk = 0; top->eval(); t++; };

    while (next < vecs.size() || !pending.empty()) {
        // operands presented during clock t
        if (next < vecs.size() && !(gaps && (rng() & 3) == 0)) {
            const Vec &v = vecs[next++];
            top->i_a = v.a; top->i_b = v.b; top->i_sub = v.sub; top->i_kind = v.kind;
            pending.push_back({t + LATENCY[v.unit], &v});
        } else {
            top->i_a = ((uint64_t)rng() << 32) | rng(); top->i_b = ((uint64_t)rng() << 32) | rng(); top->i_sub = rng() & 1; top->i_kind = rng() & 3;   // junk between vectors
        }
        tick();
        // latencies differ per unit, so results do not come out in the order they went in
        for (size_t i = 0; i < pending.size();) {
            if (pending[i].due != t) { i++; continue; }
            const Vec *v = pending[i].v; pending.erase(pending.begin() + i);
            uint64_t got = v->unit == 0 ? top->o_add : v->unit == 1 ? top->o_mul : top->o_recip;
            unsigned gf = v->unit == 0 ? top->o_add_err : v->unit == 1 ? top->o_mul_err : top->o_recip_err;
            checked++;
            if (got != v->r || gf != (v->flags & 1)) {
                if (errors++ < max_print)
                    printf("MISMATCH %s %016llx %016llx: got %016llx/%u want %016llx/%u (line %ld)\n", v->op.c_str(),
                           (unsigned long long)v->a, (unsigned long long)v->b, (unsigned long long)got, gf,
                           (unsigned long long)v->r, v->flags & 1, v->line);
            }
        }
    }
    printf("fp bench: %ld checked, %ld mismatches%s\n", checked, errors, gaps ? ", with gaps" : "");
    (void)skipped;
    delete top;
    return errors ? 1 : 0;
}

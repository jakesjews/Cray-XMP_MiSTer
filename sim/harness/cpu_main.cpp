// CPU-level simulation: cray_cpu against a behavioural memory.
//
//   Vcray_cpu --image FILE [--mem PROFILE] [--cycles N] [--seed N] [--step]
//             [--log-mem] [--dump START:COUNT] [--state OUT] [--expect FILE] [--quiet]
//             [--input TEXT] [--ctrl-c CYCLE[,CYCLE...]]
// PROFILE: 0, fixed:N, rand:A-B, ddr3, slow.  Addresses are octal.
// --ctrl-c: the console asks for its interrupt in those clocks, as CTRL-C does.
// Exit status: 0 when the program wrote TEST_EXIT with code 0, 1 otherwise.
#include "Vcray_cpu.h"
#include "verilated.h"
#include "cray_mem_model.h"

#include <cstdio>
#include <cstdlib>
#include <string>

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);
    std::string image, profile = "fixed:1", dump, state, expect;
    std::string input;
    long cycles = 200000;
    uint32_t seed = 1;
    bool single_step = false, log_mem = false, quiet = false;
    std::set<long> ctrl_c;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        auto next = [&]() { return std::string(i + 1 < argc ? argv[++i] : ""); };
        if (a == "--image") image = next();
        else if (a == "--mem") profile = next();
        else if (a == "--cycles") cycles = atol(next().c_str());
        else if (a == "--seed") seed = (uint32_t)atol(next().c_str());
        else if (a == "--step") single_step = true;
        else if (a == "--log-mem") log_mem = true;
        else if (a == "--dump") dump = next();
        else if (a == "--quiet") quiet = true;
        else if (a == "--state") state = next();
        else if (a == "--expect") expect = next();
        else if (a == "--input") input = next();
        else if (a == "--ctrl-c") {
            std::string list = next();
            for (size_t p = 0; p < list.size();) { ctrl_c.insert(atol(list.c_str() + p)); p = list.find(',', p); if (p == std::string::npos) break; p++; }
        }
    }
    Verilated::randSeed(seed);

    CrayMemModel mem(seed);
    mem.prof = MemProfile::parse(profile);
    mem.log = log_mem;
    mem.echo_console = !quiet;
    for (size_t i = 0; i < input.size(); i++) {          // console input, with \r \n \t \\ \xHH
        char c = input[i];
        if (c == '\\' && i + 1 < input.size()) {
            char e = input[++i];
            if (e == 'r') c = '\r'; else if (e == 'n') c = '\n'; else if (e == 't') c = '\t';
            else if (e == 'x' && i + 2 < input.size()) { c = (char)strtol(input.substr(i + 1, 2).c_str(), nullptr, 16); i += 2; }
            else c = e;
        }
        mem.console_in.push_back((uint8_t)c);
    }
    if (image.empty() || !mem.load(image)) { fprintf(stderr, "cannot read image '%s'\n", image.c_str()); return 2; }

    Vcray_cpu *top = new Vcray_cpu;
    top->rst = 1;
    top->i_single_step = single_step;
    top->i_mcu_int = 0;
    top->i_mem_ack = 0;
    top->i_mem_rdata = 0;
    top->i_ch_in_ready = 0;
    top->i_ch_in_data = 0;
    top->i_ch_in_disconnect = 0;
    top->i_ch_out_resume = 0;

    long t = 0;
    for (; t < cycles && !mem.exited && !Verilated::gotFinish(); t++) {
        if (t == 8) top->rst = 0;
        if (ctrl_c.count(t)) mem.ctrl_c();
        top->i_mcu_int = mem.con_int_req;
        // X-MP: each output channel is cabled to the input channel of its pair
        top->i_ch_in_ready = top->o_ch_out_ready;
        top->i_ch_in_data = top->o_ch_out_data;
        top->i_ch_in_disconnect = top->o_ch_out_disconnect;
        top->i_ch_out_resume = top->o_ch_in_resume;
        bool req = top->o_mem_req, we = top->o_mem_we, burst = top->o_mem_burst;
        uint32_t addr = top->o_mem_addr; uint64_t wdata = top->o_mem_wdata;
        top->clk = 1; top->eval();
        mem.step(req, we, burst, addr, wdata);
        top->i_mem_ack = mem.ack;
        top->i_mem_rdata = mem.rdata;
        top->eval();
        top->clk = 0; top->eval();
    }

    if (!state.empty()) mem.write_state(state);
    long expect_bad = 0;
    if (!expect.empty()) {
        // lines of "octal-address hex-value": memory words the program must have left behind
        FILE *f = fopen(expect.c_str(), "r");
        unsigned a; unsigned long long v; long n = 0;
        while (f && fscanf(f, "%o %llx", &a, &v) == 2) {
            n++;
            if (mem.mem[a % mem.WORDS] != v && expect_bad++ < 12)
                printf("EXPECT %07o: got %016llx want %016llx\n", a, (unsigned long long)mem.mem[a % mem.WORDS], v);
        }
        if (f) fclose(f);
        printf("expect: %ld words checked, %ld wrong\n", n, expect_bad);
    }
    if (!dump.empty()) {
        unsigned start = 0, count = 8;
        sscanf(dump.c_str(), "%o:%o", &start, &count);
        for (unsigned i = 0; i < count; i++) printf("%07o: %022llo\n", start + i, (unsigned long long)mem.mem[(start + i) % mem.WORDS]);
    }
    if (!quiet || !mem.exited)
        fprintf(stderr, "sim: %ld cycles, mem %s, %llu reads, %llu writes, %s\n", t, mem.prof.name.c_str(),
                (unsigned long long)mem.reads, (unsigned long long)mem.writes,
                mem.exited ? ("exit code " + std::to_string(mem.exit_code)).c_str() : "NO EXIT (time-out)");
    top->final();
    delete top;
    return (mem.exited && mem.exit_code == 0 && !mem.bad && expect_bad == 0) ? 0 : 1;
}

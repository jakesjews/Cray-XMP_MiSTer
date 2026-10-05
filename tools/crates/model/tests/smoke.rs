//! The CAL smoke tests of `tests/smoke`, assembled with `cray1-asm` and run
//! on the model under every runtime they list, and tests of the runtimes
//! themselves and of the `cray1-run` binary.
//!
//! A smoke test is written against `rt_direct.cal`.  Its `* RUNTIMES:` line
//! lists the start-up conventions it can run under:
//!
//! * `direct`: as written;
//! * `exch`: `INCLUDE "rt_exch.cal"` instead (the same text otherwise);
//! * `user`: `rt_exch.cal` and `TUSER 0` after the INCLUDE: a user program
//!   with base address 0;
//! * `reloc`: `rt_exch.cal` and `TUSER 1000`: a user program relocated to
//!   word 20000.

use cray1_model::{report, Machine, RunResult};
use std::path::{Path, PathBuf};

const STATUS_WORD: u32 = 0o100;
const DONE_WORD: u32 = 0o101;
const DUMP: u32 = 0o4000;
const CRAYDONE: u64 = u64::from_be_bytes(*b"CRAYDONE");
/// Base address register for the `reloc` runtime, and the base it gives.
const RELOC_BA: u32 = 0o1000;
const RELOC_BASE: u32 = RELOC_BA * 16;
const MAX_STEPS: u64 = 200_000;

fn tests_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests");
    assert!(
        dir.join("rt/rt_direct.cal").is_file(),
        "the test runtime is missing: {}",
        dir.join("rt").display()
    );
    dir
}

/// The source of a test under a runtime.
fn for_runtime(source: &str, runtime: &str) -> String {
    assert!(
        source.contains("INCLUDE \"rt_direct.cal\""),
        "a smoke test includes rt_direct.cal"
    );
    let exch = source.replace("INCLUDE \"rt_direct.cal\"", "INCLUDE \"rt_exch.cal\"");
    let with_tuser = |ba: u32| {
        exch.replace(
            "INCLUDE \"rt_exch.cal\"\n",
            &format!("INCLUDE \"rt_exch.cal\"\n         TUSER   {:o}\n", ba),
        )
    };
    match runtime {
        "direct" => source.to_string(),
        "exch" => exch,
        "user" => with_tuser(0),
        "reloc" => with_tuser(RELOC_BA),
        other => panic!("unknown runtime `{}`", other),
    }
}

/// Assemble CAL text; INCLUDE files come from tests/rt.
fn assemble(name: &str, source: &str) -> Vec<u8> {
    let rt = tests_dir().join("rt");
    let mut include = |file: &str, _from: &str| -> Result<(String, String), String> {
        let path = rt.join(file);
        std::fs::read_to_string(&path)
            .map(|text| (path.display().to_string(), text))
            .map_err(|e| e.to_string())
    };
    let assembly = cray1_asm::assemble(name, source, &mut include);
    let messages: Vec<String> = assembly.diagnostics.iter().map(|d| d.to_string()).collect();
    assert!(
        messages.is_empty(),
        "{} does not assemble cleanly:\n{}",
        name,
        messages.join("\n")
    );
    assembly.image()
}

fn run(name: &str, source: &str, input: &[u8]) -> (Machine, RunResult) {
    let mut machine = Machine::with_image(&assemble(name, source)).unwrap();
    machine.push_input(input);
    let result = machine.run(MAX_STEPS);
    (machine, result)
}

/// The runtimes a smoke test lists.
fn runtimes(source: &str) -> Vec<String> {
    let line = source
        .lines()
        .find_map(|l| l.strip_prefix("* RUNTIMES:"))
        .expect("a smoke test has a `* RUNTIMES:` line");
    line.split_whitespace().map(str::to_string).collect()
}

fn smoke_source(name: &str) -> String {
    let path = tests_dir().join("smoke").join(format!("{}.cal", name));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// Run a smoke test under every runtime it lists; it must pass.  Returns
/// the machines for further checks.
fn pass(name: &str, expect_runtimes: &[&str]) -> Vec<(String, Machine)> {
    let source = smoke_source(name);
    assert_eq!(runtimes(&source), expect_runtimes, "{}: runtimes", name);
    let mut out = Vec::new();
    for runtime in runtimes(&source) {
        let label = format!("{} ({})", name, runtime);
        let (machine, result) = run(&label, &for_runtime(&source, &runtime), b"");
        assert_eq!(
            result,
            RunResult::Exit(0),
            "{}\n{}",
            label,
            report::state_text(&machine, &result)
        );
        // the status word and the end marker, at absolute 100 and 101
        assert_eq!(machine.mem(STATUS_WORD), Some(1), "{}: status word", label);
        assert_eq!(
            machine.mem(DONE_WORD),
            Some(CRAYDONE),
            "{}: end marker",
            label
        );
        if runtime == "reloc" {
            assert_eq!(
                machine.mem(RELOC_BASE + STATUS_WORD),
                Some(1),
                "{}: the test's own status word",
                label
            );
            assert_eq!(machine.ba(), 0, "{}: back in the start-up", label);
        }
        if runtime != "direct" {
            // the start-up's package was restored and the test's is at 20
            assert!(machine.monitor_mode());
            let f = machine.mem(0o23).map(|w| (w >> 24) & 0o777);
            let user = matches!(runtime.as_str(), "user" | "reloc");
            assert_eq!(f, Some(user as u64), "{}: F of the test's package", label);
            let ba = machine.mem(0o21).map(|w| (w >> 28) as u32);
            assert_eq!(
                ba,
                Some(if runtime == "reloc" { RELOC_BA } else { 0 }),
                "{}: BA of the test's package",
                label
            );
        }
        out.push((runtime, machine));
    }
    out
}

const ALL: &[&str] = &["direct", "exch", "user", "reloc"];
const MONITOR: &[&str] = &["direct", "exch"];

#[test]
fn every_smoke_test_is_run() {
    let mut names: Vec<String> = std::fs::read_dir(tests_dir().join("smoke"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".cal"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "branch.cal",
            "chan.cal",
            "dump.cal",
            "exchange.cal",
            "float.cal",
            "hello.cal",
            "int.cal",
            "mem.cal",
            "range.cal",
            "recur.cal",
            "shift.cal",
            "vector.cal"
        ],
        "a smoke test was added or removed: give it a test function here"
    );
}

#[test]
fn smoke_hello() {
    let expect = b"HELLO, CRAY-1\n0000000000000001234567\n";
    for (runtime, machine) in pass("hello", &["direct", "exch", "user"]) {
        assert_eq!(machine.console(), expect, "{}", runtime);
    }
    // with console input waiting it is echoed
    let source = smoke_source("hello");
    for runtime in runtimes(&source) {
        let (machine, result) = run("hello", &for_runtime(&source, &runtime), b"ok\n");
        assert_eq!(result, RunResult::Exit(0));
        assert_eq!(
            machine.console(),
            [&expect[..], b"ok\n"].concat(),
            "{}",
            runtime
        );
    }
}

#[test]
fn smoke_int() {
    pass("int", ALL);
}

#[test]
fn smoke_branch() {
    pass("branch", ALL);
}

#[test]
fn smoke_chan() {
    pass("chan", &["direct", "exch", "user"]);
}

#[test]
fn smoke_shift() {
    pass("shift", ALL);
}

#[test]
fn smoke_mem() {
    pass("mem", ALL);
}

#[test]
fn smoke_vector() {
    pass("vector", ALL);
}

#[test]
fn smoke_float() {
    pass("float", ALL);
    // the quotient the test expects is the divide sequence of cray1-fp
    let f = |v: f64| cray1_fp::from_f64(v).unwrap();
    let quotient = cray1_fp::fdiv(f(6.0), f(3.0), cray1_fp::Profile::Cray1);
    assert_eq!(
        (quotient.value, quotient.range_error),
        (0x4002_8000_0000_0000, false)
    );
    assert_eq!(quotient.value, f(2.0));
}

#[test]
fn smoke_exchange() {
    for (runtime, machine) in pass("exchange", MONITOR) {
        // the user program's result, in its field at absolute 10000
        assert_eq!(machine.mem(0o10042), Some(301), "{}", runtime);
        // its package at 3000 after the second exit: the normal exit flag
        assert_eq!(
            machine.mem(0o3003).map(|w| (w >> 24) & 0o777),
            Some(1),
            "{}",
            runtime
        );
    }
}

#[test]
fn smoke_range() {
    for (runtime, machine) in pass("range", MONITOR) {
        assert_eq!(
            machine.mem(0o10100),
            Some(123456),
            "{}: the word beyond the limit",
            runtime
        );
        assert!(!machine.mem_written(0o10100), "{}", runtime);
        assert_eq!(machine.mem(0o10077), Some(77), "{}", runtime);
        // the user's package holds the program range flag
        assert_eq!(
            machine.mem(0o3003).map(|w| (w >> 24) & 0o777),
            Some(0o20),
            "{}",
            runtime
        );
    }
}

#[test]
fn smoke_dump() {
    for (runtime, machine) in pass("dump", ALL) {
        let base = if runtime == "reloc" { RELOC_BASE } else { 0 };
        let dump = |offset: u32| machine.mem(base + DUMP + offset);
        assert_eq!(dump(0o1), Some(11), "{}: A1", runtime);
        assert_eq!(dump(0o17), Some(7777), "{}: S7", runtime);
        assert_eq!(dump(0o20), Some(3), "{}: vector length", runtime);
        assert_eq!(dump(0o21), Some(0xa5 << 56), "{}: VM", runtime);
        assert_eq!(dump(0o105), Some(55), "{}: B05", runtime);
        assert_eq!(dump(0o205), Some(555), "{}: T05", runtime);
        assert_eq!(dump(0o300), Some(10), "{}: V0 element 0", runtime);
        assert_eq!(dump(0o1277), Some(6363), "{}: V7 element 63", runtime);
        // registers the test never set are dumped as undefined
        assert_eq!(dump(0o106), None, "{}: B06", runtime);
        assert_eq!(dump(0o500), None, "{}: V2 element 0", runtime);
        assert_eq!(
            machine.vl(),
            Some(if runtime == "direct" { 0o100 } else { 0 }),
            "{}: VL",
            runtime
        );
    }
}

#[test]
fn smoke_recur() {
    pass("recur", ALL);
}

// ---- the runtimes themselves

fn runtime_test(body: &str, runtime: &str, input: &[u8]) -> (Machine, RunResult) {
    let source = format!(
        "         INCLUDE \"rt_direct.cal\"\n         TBEGIN\n{}\n         END\n",
        body
    );
    run(runtime, &for_runtime(&source, runtime), input)
}

#[test]
fn a_failed_check_gives_its_number() {
    let cases = [
        ("         S1      5\n         CHKS    S1,6,7\n         TPASS\n         TEND", 7),
        ("         A1      5\n         CHKA    A1,6,D'9\n         TPASS\n         TEND", 9),
        ("         TFAIL   D'200\n         TEND", 200),
        ("         S0      5\n         CHKS    S0,6,3\n         TPASS\n         TEND", 3),
        // the same checks pass with the right value: S0 and S7 too
        ("         S0      5\n         CHKS    S0,5,3\n         S7      -1\n         CHKS    S7,-1,4\n         TPASS\n         TEND", 0),
        // CHKA compares 24 bits; CHKS and CHKA keep S6 and S7
        ("         A1      -1\n         CHKA    A1,-1,2\n         S6      D'66\n         S7      D'77\n         CHKA    A1,77777777,3\n         CHKS    S6,D'66,4\n         CHKS    S7,D'77,5\n         TPASS\n         TEND", 0),
    ];
    for (body, code) in cases {
        for runtime in ALL {
            let (machine, result) = runtime_test(body, runtime, b"");
            assert_eq!(result, RunResult::Exit(code), "{}:\n{}", runtime, body);
            let status = if code == 0 { 1 } else { code };
            assert_eq!(machine.mem(STATUS_WORD), Some(status), "{}", runtime);
            assert_eq!(machine.mem(DONE_WORD), Some(CRAYDONE), "{}", runtime);
        }
    }
}

#[test]
fn a_test_without_a_verdict_fails() {
    // the status word starts as 777 (octal): exit code 511, status 255
    for runtime in ALL {
        let (machine, result) = runtime_test("         TEND", runtime, b"");
        assert_eq!(result, RunResult::Exit(0o777), "{}", runtime);
        assert_eq!(result.exit_status(), 255);
        assert_eq!(machine.mem(STATUS_WORD), Some(0o777));
    }
}

#[test]
fn the_exchange_runtime_reports_an_interrupted_test() {
    // A user program that passes and then stores outside memory: operand
    // range (flag 40), reported as 1000 + F.
    let body = "         TPASS\n         S1      5\n         A1      -1\n         0,A1    S1\n         TEND";
    for runtime in ["user", "reloc"] {
        let (machine, result) = runtime_test(body, runtime, b"");
        assert_eq!(result, RunResult::Exit(0o1040), "{}", runtime);
        assert_eq!(machine.mem(STATUS_WORD), Some(0o1040));
        assert_eq!(machine.mem(DONE_WORD), Some(CRAYDONE));
    }
    // an error exit (flag 2) in a user program
    for runtime in ["user", "reloc"] {
        let (_, result) = runtime_test("         TPASS\n         ERR\n         TEND", runtime, b"");
        assert_eq!(result, RunResult::Exit(0o1002), "{}", runtime);
    }
    // In monitor mode no flag sets: the store is dropped and the test goes
    // on (HRM pages 3-36 and 3-43).
    for runtime in ["direct", "exch"] {
        let (_, result) = runtime_test(body, runtime, b"");
        assert_eq!(result, RunResult::Exit(0), "{}", runtime);
    }
    // The console is out of reach of a relocated program.
    let (machine, result) = runtime_test(
        "         TPASS\n         PUTCH   'A'\n         TEND",
        "reloc",
        b"",
    );
    assert_eq!(result, RunResult::Exit(0o1040));
    assert!(machine.console().is_empty());
}

#[test]
fn dumpas_stores_the_a_and_s_registers() {
    let body = "         A0      D'10\n         A7      D'17\n         S0      D'20\n         S7      D'27\n         TPASS\n         TEND    DUMPAS";
    for runtime in ALL {
        let (machine, result) = runtime_test(body, runtime, b"");
        assert_eq!(result, RunResult::Exit(0), "{}", runtime);
        let base = if *runtime == "reloc" { RELOC_BASE } else { 0 };
        let dump = |offset: u32| machine.mem(base + DUMP + offset);
        assert_eq!((dump(0), dump(7)), (Some(10), Some(17)), "{}", runtime);
        // TPASS clobbers S0: it is 1 when the dump is taken
        assert_eq!((dump(0o10), dump(0o17)), (Some(1), Some(27)), "{}", runtime);
        assert_eq!(dump(1), Some(0), "A1 is 0 from the start-up package");
        assert!(
            !machine.mem_written(base + DUMP + 0o20),
            "DUMPAS stops after S7"
        );
    }
}

#[test]
fn console_macros() {
    let body = "         PUTCH   'x'\n         S1      -1\n         PUTOCT\n         PUTNL\n         S1      0\n         PUTOCT\n         PUTNL\n         S1      TEXT,0\n         PUTOCT\n         PUTNL\n         PUTS    TEXT\n         PUTS    EIGHT\n         PUTNL\n         TPASS\n         TEND\nTEXT     DATA    'abc'\nEIGHT    DATA    '12345678',0";
    let abc = u64::from_be_bytes(*b"abc\0\0\0\0\0");
    let expect = format!(
        "x1777777777777777777777\n0000000000000000000000\n{:022o}\nabc12345678\n",
        abc
    );
    for runtime in ["direct", "exch", "user"] {
        let (machine, result) = runtime_test(body, runtime, b"");
        assert_eq!(result, RunResult::Exit(0), "{}", runtime);
        assert_eq!(
            String::from_utf8_lossy(machine.console()),
            expect,
            "{}",
            runtime
        );
    }
}

// ---- the cray1-run binary

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cray1-run-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn cray1_run(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_cray1-run"))
        .args(args)
        .output()
        .expect("cray1-run starts")
}

#[test]
fn binary_runs_an_image() {
    let image = scratch("hello.img");
    std::fs::write(&image, assemble("hello", &smoke_source("hello"))).unwrap();
    let (state, trace) = (scratch("hello.state"), scratch("hello.trace"));
    let out = cray1_run(&[
        image.to_str().unwrap(),
        "--input",
        "hi\\n",
        "--state",
        state.to_str().unwrap(),
        "--trace",
        trace.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"HELLO, CRAY-1\n0000000000000001234567\nhi\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("cray1-run: exit 0 after "), "{}", stderr);

    let state = std::fs::read_to_string(&state).unwrap();
    let console: String = b"HELLO, CRAY-1\n0000000000000001234567\nhi\n"
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    assert!(
        state.starts_with(&format!("exit 0\nconsole {}\nmem 0000000 undef\n", console)),
        "{}",
        state
    );
    assert!(
        state.contains(
            "\nmem 0000017 undef\nmem 0000100 0000000000000001\nmem 0000101 43524159444f4e45\n"
        ),
        "{}",
        state
    );
    assert!(state.contains("\nM 1\nF 000\n"), "{}", state);
    // the state file is what the library gives
    let mut machine = Machine::with_image(&std::fs::read(&image).unwrap()).unwrap();
    machine.push_input(b"hi\n");
    let result = machine.run(10_000_000);
    assert_eq!(state, report::state_text(&machine, &result));

    let trace = std::fs::read_to_string(&trace).unwrap();
    let lines: Vec<&str> = trace.lines().collect();
    assert_eq!(lines[0], "X start 00");
    assert_eq!(lines[1], "M 0000000 undef");
    assert!(lines.contains(&"X done"));
    assert!(lines.contains(&"C 48"), "the H of HELLO");
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("I ")).count() as u64,
        machine.instructions()
    );
    // the run ends with the write to TEST_EXIT
    assert_eq!(
        lines[lines.len() - 2],
        format!("I {:08o} 130117 177762  3777762,0 S1", machine.p() - 2)
    );
    assert_eq!(*lines.last().unwrap(), "E 0");

    // --quiet: no console copy, no summary
    let out = cray1_run(&[image.to_str().unwrap(), "--quiet"]);
    assert_eq!(
        (out.status.code(), out.stdout.len(), out.stderr.len()),
        (Some(0), 0, 0)
    );
}

#[test]
fn binary_exit_statuses() {
    // a failed check: the exit code is the status
    let source = "         INCLUDE \"rt_direct.cal\"\n         TBEGIN\n         TFAIL   D'42\n         TEND\n         END\n";
    let image = scratch("fail.img");
    std::fs::write(&image, assemble("fail", source)).unwrap();
    let out = cray1_run(&[image.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(42));
    // codes above 255 saturate
    let source =
        "         INCLUDE \"rt_direct.cal\"\n         TBEGIN\n         TEND\n         END\n";
    let image = scratch("noverdict.img");
    std::fs::write(&image, assemble("noverdict", source)).unwrap();
    let state = scratch("noverdict.state");
    let out = cray1_run(&[image.to_str().unwrap(), "--state", state.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(255));
    assert!(std::fs::read_to_string(&state)
        .unwrap()
        .starts_with("exit 511\nconsole\n"));
    // no exit within --max: 2
    let source = "         INCLUDE \"rt_direct.cal\"\n         TBEGIN\nLOOP     J       LOOP\n         TEND\n         END\n";
    let image = scratch("loop.img");
    std::fs::write(&image, assemble("loop", source)).unwrap();
    let state = scratch("loop.state");
    let out = cray1_run(&[
        image.to_str().unwrap(),
        "--max",
        "1000",
        "--state",
        state.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no exit after 1000 steps"));
    assert!(std::fs::read_to_string(&state)
        .unwrap()
        .starts_with("exit none\nconsole\n"));
    // a test error: 3, reported even with --quiet
    let source = "         INCLUDE \"rt_direct.cal\"\n         TBEGIN\n         A0      B5\n         JAZ     T$END\n         TEND\n         END\n";
    let image = scratch("undef.img");
    std::fs::write(&image, assemble("undef", source)).unwrap();
    let out = cray1_run(&[image.to_str().unwrap(), "--quiet"]);
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with(
            "cray1-run: undefined value used: A0 (branch condition) at P=00001001 (010000 "
        ),
        "{}",
        stderr
    );
    // usage and file errors
    assert_eq!(cray1_run(&[]).status.code(), Some(64));
    assert_eq!(cray1_run(&["--bogus", "x"]).status.code(), Some(64));
    assert_eq!(
        cray1_run(&[scratch("missing.img").to_str().unwrap()])
            .status
            .code(),
        Some(66)
    );
}

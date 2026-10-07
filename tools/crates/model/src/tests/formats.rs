//! The trace and state text formats of `cray-xmp-run`.

use super::*;
use crate::report::{state_text, trace_line};

#[test]
fn trace_lines() {
    let line = |e: Event| trace_line(&e);
    assert_eq!(
        line(Event::Issue {
            p: 0o1000,
            parcel0: 0o030123,
            parcel1: None
        }),
        "I 00001000 030123         A1        A2+A3"
    );
    assert_eq!(
        line(Event::Issue {
            p: 0o17777776,
            parcel0: 0o040100,
            parcel1: Some(0o1234)
        }),
        "I 17777776 040100 001234  S1        1234"
    );
    assert_eq!(
        line(Event::A {
            i: 3,
            value: Some(0xab_cdef)
        }),
        "A3 abcdef"
    );
    assert_eq!(line(Event::A { i: 0, value: None }), "A0 undef");
    assert_eq!(
        line(Event::S {
            i: 7,
            value: Some(0x1234)
        }),
        "S7 0000000000001234"
    );
    assert_eq!(line(Event::S { i: 7, value: None }), "S7 undef");
    assert_eq!(
        line(Event::B {
            jk: 0o5,
            value: Some(1)
        }),
        "B05 000001"
    );
    assert_eq!(
        line(Event::B {
            jk: 0o77,
            value: None
        }),
        "B77 undef"
    );
    assert_eq!(
        line(Event::T {
            jk: 0o70,
            value: Some(u64::MAX)
        }),
        "T70 ffffffffffffffff"
    );
    assert_eq!(
        line(Event::V {
            i: 2,
            elem: 63,
            value: Some(1)
        }),
        "V2[63] 0000000000000001"
    );
    assert_eq!(
        line(Event::V {
            i: 2,
            elem: 0,
            value: None
        }),
        "V2[0] undef"
    );
    assert_eq!(line(Event::Vl(Some(0o100))), "VL 40");
    assert_eq!(line(Event::Vl(None)), "VL undef");
    assert_eq!(line(Event::Vm(Some(1 << 63))), "VM 8000000000000000");
    assert_eq!(
        line(Event::Mem {
            addr: 0o1234,
            value: Some(0xff)
        }),
        "M 0001234 00000000000000ff"
    );
    assert_eq!(
        line(Event::Mem {
            addr: 0o3777757,
            value: None
        }),
        "M 3777757 undef"
    );
    assert_eq!(line(Event::ExchangeStart { xa: 0x1f }), "X start 1f");
    assert_eq!(line(Event::ExchangeEnd), "X done");
    assert_eq!(line(Event::Console(b'A')), "C 41");
    // the lines beyond the basic set
    assert_eq!(line(Event::Xa(1)), "XA 01");
    assert_eq!(line(Event::Mode(5)), "MODE 5");
    assert_eq!(line(Event::Flags(0o40)), "F 020");
    assert_eq!(line(Event::P(0o1000)), "P 000200");
    assert_eq!(line(Event::Ba(0o100)), "BA 00040");
    assert_eq!(line(Event::La(LA_MAX)), "LA 3ffff");
    assert_eq!(line(Event::Rtc(Some(0))), "RT 0000000000000000");
    assert_eq!(line(Event::Exit(511)), "E 511");
}

#[test]
fn events_of_one_instruction() {
    let mut m = monitor_cal("S1 1000,0; A2 A2+1; 1001,0 S1; B7 A2; T7 S1; VL A2; VM S1");
    m.load_words(0o1000, &[0x77]).unwrap();
    let events = record(&mut m);
    steps(&mut m, 7);
    let lines: Vec<String> = events.borrow().iter().map(trace_line).collect();
    assert_eq!(
        lines,
        [
            "I 00001000 120100 001000  S1        1000,0",
            "S1 0000000000000077",
            "I 00001002 030220         A2        A2+1",
            "A2 000001",
            "I 00001003 130100 001001  1001,0    S1",
            "M 0001001 0000000000000077",
            "I 00001005 025207         B07       A2",
            "B07 000001",
            "I 00001006 075107         T07       S1",
            "T07 0000000000000077",
            "I 00001007 002002         VL        A2",
            "VL 01",
            "I 00001010 003010         VM        S1",
            "VM 0000000000000077",
        ]
    );
}

#[test]
fn state_file() {
    // a user program that prints, stores, and exits to a monitor that ends
    // the run
    let mut m = user(&cal("3777761,0 S1; 300,0 S2; 301,0 S3; EX"), 0, LA_MAX);
    m.load_words(0o100, &words(&cal("S1 D'300; 3777762,0 S1; J 400")))
        .unwrap();
    m.set_s(1, Some(b'Z' as u64));
    m.set_s(2, Some(0xdead_beef));
    m.set_s(3, None);
    m.set_a(7, None);
    m.set_b(0o12, Some(0o1234));
    m.set_t(0o77, Some(5));
    m.set_v(6, 2, Some(6));
    m.set_vm(Some(1 << 63));
    let result = m.run(100);
    assert_eq!(result, RunResult::Exit(300));
    assert_eq!(result.exit_status(), 255);
    let expect = "\
exit 300
console 5a
mem 0000000 0000000207000000
mem 0000001 0000000000000000
mem 0000002 00003ffff0000000
mem 0000003 0000000001000000
mem 0000004 0000000000000000
mem 0000005 0000000000000000
mem 0000006 0000000000000000
mem 0000007 undef
mem 0000010 0000000000000000
mem 0000011 000000000000005a
mem 0000012 00000000deadbeef
mem 0000013 undef
mem 0000014 0000000000000000
mem 0000015 0000000000000000
mem 0000016 0000000000000000
mem 0000017 0000000000000000
mem 0000300 00000000deadbeef
mem 0000301 undef
A0 000000
A1 000000
A2 000000
A3 000000
A4 000000
A5 000000
A6 000000
A7 000000
S0 0000000000000000
S1 000000000000012c
S2 0000000000000000
S3 0000000000000000
S4 0000000000000000
S5 0000000000000000
S6 0000000000000000
S7 0000000000000000
VL 00
VM 8000000000000000
P 000104
BA 00000
LA 3ffff
XA 00
M 1
F 000
B12 00029c
T77 0000000000000005
V6[2] 0000000000000006
";
    assert_eq!(state_text(&m, &result), expect);

    // no exit, no console output, undefined registers
    let mut m = Machine::new();
    m.load_words(CODE, &words(&cal("J 1000"))).unwrap();
    m.start_at(CODE * 4, 0, LA_MAX, mode::MONITOR);
    let result = m.run(10);
    let text = state_text(&m, &result);
    assert!(
        text.starts_with("exit none\nconsole\nA0 undef\n"),
        "{}",
        text
    );
    assert!(
        text.contains("\nS7 undef\nVL undef\nVM undef\nP 000200\n"),
        "{}",
        text
    );
    assert!(text.ends_with("\nM 1\nF 000\n"), "{}", text);
}

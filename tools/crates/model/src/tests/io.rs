//! The I/O page (not in the manual: the project's console, exit and cycle
//! counter words at the top of memory).

use super::*;

#[test]
fn io_page_addresses() {
    assert_eq!(MEMORY_WORDS, 1 << 20);
    assert_eq!(
        (IO_PAGE, CON_STAT, CON_DATA, TEST_EXIT, CYCLES),
        (0xffff0, 0xffff0, 0xffff1, 0xffff2, 0xffff3)
    );
    assert_eq!(IO_PAGE, 0o3777760);
}

#[test]
fn console_output() {
    let mut m = monitor_cal("S1 3777760,0; 3777761,0 S2; 3777761,0 A3; S4 3777760,0");
    m.set_s(2, Some(0x4141_4141_4141_4148)); // the low 8 bits are written
    m.set_a(3, Some(0x69));
    let events = record(&mut m);
    steps(&mut m, 4);
    assert_eq!(m.s(1), Some(2), "output ready, no input waiting");
    assert_eq!(m.s(4), Some(2));
    assert_eq!(m.console(), b"Hi");
    let out: Vec<Event> = events
        .borrow()
        .iter()
        .filter(|e| matches!(e, Event::Console(_)))
        .cloned()
        .collect();
    assert_eq!(out, [Event::Console(b'H'), Event::Console(b'i')]);
    assert!(m.written_words().is_empty(), "the I/O page is not memory");
    assert!(!m.mem_written(CON_DATA));
    assert_eq!(m.mem(CON_DATA), None);
}

#[test]
fn console_input() {
    let mut m = monitor_cal("S1 3777760,0; S2 3777761,0; A3 3777761,0; S4 3777760,0; S5 3777761,0");
    m.push_input(b"ok");
    steps(&mut m, 5);
    assert_eq!(
        m.s(1),
        Some(3),
        "output ready and an input character waiting"
    );
    assert_eq!(m.s(2), Some(b'o' as u64));
    assert_eq!(m.a(3), Some(b'k' as u32));
    assert_eq!(m.s(4), Some(2));
    assert_eq!(m.s(5), Some(0), "no character: 0");
}

#[test]
fn test_exit_ends_the_run() {
    let mut m = monitor_cal("S1 3777762,0; 3777762,0 S2; A1 5");
    m.set_s(2, Some(7));
    steps(&mut m, 1);
    assert_eq!(m.s(1), Some(0), "TEST_EXIT reads 0");
    assert_eq!(m.step(), StepResult::Exit(7));
    assert_eq!(m.exit_code(), Some(7));
    // the machine has stopped
    assert_eq!(m.step(), StepResult::Exit(7));
    assert_eq!(m.run(10), RunResult::Exit(7));
    assert_eq!(m.a(1), Some(0));
    assert_eq!(m.instructions(), 2);

    let mut m = monitor_cal("3777762,0 S2");
    m.set_s(2, Some(0));
    assert_eq!(m.run(10), RunResult::Exit(0));
}

#[test]
fn exit_status_saturates() {
    assert_eq!(RunResult::Exit(0).exit_status(), 0);
    assert_eq!(RunResult::Exit(1).exit_status(), 1);
    assert_eq!(RunResult::Exit(255).exit_status(), 255);
    assert_eq!(RunResult::Exit(256).exit_status(), 255);
    assert_eq!(RunResult::Exit(0o777).exit_status(), 255);
    assert_eq!(RunResult::Exit(u64::MAX).exit_status(), 255);
    assert_eq!(RunResult::Limit.exit_status(), 2);
    let e = TestError {
        kind: ErrorKind::UndefinedValue,
        p: 0,
        parcels: None,
        cpu: cray1_isa::Cpu::Cray1,
        detail: String::new(),
    };
    assert_eq!(RunResult::Error(e).exit_status(), 3);
}

#[test]
fn other_io_words_read_zero_and_ignore_writes() {
    let mut m = monitor_cal("3777764,0 S1; S2 3777764,0; 3777777,0 S1; S3 3777777,0; 3777760,0 S1; S4 3777760,0; 3777763,0 S1");
    m.set_s(1, Some(0x55));
    for i in 2..5 {
        m.set_s(i, Some(9));
    }
    steps(&mut m, 7);
    // CON_STAT keeps bit 0 of what was written as its bit 2: the console
    // interrupt enable
    assert_eq!((m.s(2), m.s(3), m.s(4)), (Some(0), Some(0), Some(6)));
    assert!(m.console().is_empty());
    assert!(m.written_words().is_empty());
    let mut m = monitor_cal("3777760,0 S1; S2 3777760,0; 3777760,0 S3; S4 3777760,0");
    m.set_s(1, Some(1));
    m.set_s(3, Some(0o776));
    steps(&mut m, 4);
    assert_eq!((m.s(2), m.s(4)), (Some(6), Some(2)));
}

#[test]
fn the_io_page_is_reached_like_memory() {
    // After base relocation and the limit check: with BA = 100 the console
    // data word is at relative 3777761 - 2000.
    let mut m = user(&cal("0,A1 S2; S3 -1,A1; EX"), 0o100, LA_MAX);
    m.set_a(1, Some(CON_DATA - 0o2000));
    m.set_s(2, Some(b'!' as u64));
    steps(&mut m, 2);
    assert_eq!(m.console(), b"!");
    assert_eq!(m.s(3), Some(2), "CON_STAT");
    assert_eq!(m.f(), 0);
    // a limit below the page keeps a user program out
    let mut m = user(&cal("3777761,0 S2"), 0, 0o177777);
    m.set_s(2, Some(b'!' as u64));
    steps(&mut m, 1);
    assert!(m.console().is_empty());
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);
    // block and vector transfers go through the page too
    let mut m = monitor_cal(",A0,A2 V1");
    m.set_a(0, Some(CON_DATA));
    m.set_a(2, Some(0)); // the same address three times
    m.set_vl(Some(3));
    for (e, c) in b"abc".iter().enumerate() {
        m.set_v(1, e, Some(*c as u64));
    }
    steps(&mut m, 1);
    assert_eq!(m.console(), b"abc");
}

#[test]
fn images() {
    let mut m = Machine::new();
    assert!(
        m.load_image(&[0; 7]).is_err(),
        "not a whole number of words"
    );
    m.load_image(&[
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0, 0, 0, 0, 0, 0, 0, 1,
    ])
    .unwrap();
    assert_eq!(
        (m.mem(0), m.mem(1), m.mem(2)),
        (Some(0x1234_5678_9abc_def0), Some(1), None)
    );
    assert!(!m.mem_written(0), "the image is not written by the run");
    assert!(m.load_words(IO_PAGE - 1, &[1]).is_ok());
    assert!(
        m.load_words(IO_PAGE - 1, &[1, 2]).is_err(),
        "the I/O page is not loadable"
    );
}

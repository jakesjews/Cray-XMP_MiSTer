//! The project's rules for undefined values: what is undefined, how it
//! spreads, and where its use is a test error.

use super::*;

/// A started machine in monitor mode with everything else as at reset.
fn bare(source: &str) -> Machine {
    let mut m = Machine::new();
    m.load_words(CODE, &words(&cal(source))).unwrap();
    m.start_at(CODE * 4, 0, LA_MAX, mode::MONITOR);
    m
}

#[test]
fn reset_state_is_undefined() {
    let m = Machine::new();
    for i in 0..8 {
        assert_eq!((m.a(i), m.s(i)), (None, None));
        for e in 0..64 {
            assert_eq!(m.v(i, e), None);
        }
    }
    for jk in 0..64 {
        assert_eq!((m.b(jk), m.t(jk)), (None, None));
    }
    assert_eq!((m.vl(), m.vm(), m.rtc()), (None, None, None));
    assert_eq!(
        (m.mem(0), m.mem(0o200), m.mem(IO_PAGE - 1)),
        (None, None, None)
    );
    assert!(m.written_words().is_empty());
}

#[test]
fn clock_and_cycle_counter_read_undefined() {
    // 072 (page 4-45) reads a counter of clock periods; CYCLES is the
    // project's equivalent in the I/O page.
    let mut m = monitor_cal("S1 RT; S2 3777763,0; RT S3; S4 RT");
    m.set_s(1, Some(1));
    m.set_s(2, Some(2));
    m.set_s(3, Some(0));
    m.set_s(4, Some(4));
    steps(&mut m, 4);
    assert_eq!((m.s(1), m.s(2), m.s(4)), (None, None, None));
}

#[test]
fn undefined_operands_give_undefined_results() {
    // A2, S2, S6, B6, T6 and VM are undefined; the results start defined.
    let mut m = bare("A1 A2+A3; S1 S2+S3; S4 S2&S5; A4 PS2; S7 +A2; B5 A2; T5 S2; A5 B6; S5 T6; S3 VM; VM S2; S6 S6<3; S0 S6>3");
    for i in [1, 3, 4, 5] {
        m.set_a(i, Some(1));
    }
    for i in [1, 3, 4, 5, 7] {
        m.set_s(i, Some(1));
    }
    m.set_b(5, Some(1));
    m.set_t(5, Some(1));
    steps(&mut m, 13);
    assert_eq!(
        (m.a(1), m.s(1), m.s(4), m.a(4), m.s(7)),
        (None, None, None, None, None)
    );
    assert_eq!(
        (m.b(5), m.t(5), m.a(5), m.s(5), m.s(3), m.vm()),
        (None, None, None, None, None, None)
    );
    assert_eq!((m.s(6), m.s(0)), (None, None));
}

#[test]
fn stores_and_loads_carry_undefined_values() {
    let mut m = bare("1000,0 S1; S2 1000,0; S3 1005,0; 1002,0 A1; ,A0 B0,A2; T10,A2 ,A0");
    m.load_words(0o1000, &[5, 6, 7]).unwrap();
    m.set_s(2, Some(0));
    m.set_s(3, Some(0));
    steps(&mut m, 2);
    assert_eq!(m.mem(0o1000), None);
    assert!(m.mem_written(0o1000));
    assert_eq!(m.written_words(), [(0o1000, None)]);
    assert_eq!(m.s(2), None);
    steps(&mut m, 2);
    assert_eq!(m.s(3), None, "word 1005 was never loaded or stored");
    assert_eq!(m.mem(0o1002), None);
    // block transfers: the address and length are defined, the data is not
    m.set_a(0, Some(0o1000));
    m.set_a(2, Some(2));
    m.set_b(1, Some(9));
    steps(&mut m, 1);
    assert_eq!((m.mem(0o1000), m.mem(0o1001)), (None, Some(9)));
    steps(&mut m, 1);
    assert_eq!((m.t(0o10), m.t(0o11)), (None, Some(9)));
}

#[test]
fn vector_elements_are_undefined_one_by_one() {
    let mut m = bare("V3 V1+V2; VM V3,Z; V4 V1!V2&VM; VM V1,N; V5 V1!V2&VM; S1 V3,A1; S2 V3,A2");
    m.set_vl(Some(3));
    for (e, v) in [1u64, 2, 3].into_iter().enumerate() {
        m.set_v(1, e, Some(v));
    }
    m.set_v(2, 0, Some(10));
    m.set_v(2, 2, Some(30));
    m.set_a(1, Some(0));
    m.set_a(2, Some(1));
    steps(&mut m, 1);
    assert_eq!(
        [m.v(3, 0), m.v(3, 1), m.v(3, 2)],
        [Some(11), None, Some(33)]
    );
    // 175 on an undefined element: the whole mask is undefined
    steps(&mut m, 1);
    assert_eq!(m.vm(), None);
    // a merge under an undefined mask is undefined
    steps(&mut m, 1);
    assert_eq!([m.v(4, 0), m.v(4, 1), m.v(4, 2)], [None, None, None]);
    // a merge takes the definedness of the operand it selects
    steps(&mut m, 1);
    assert_eq!(m.vm(), Some(0b111 << 61));
    steps(&mut m, 1);
    assert_eq!(
        [m.v(5, 0), m.v(5, 1), m.v(5, 2)],
        [Some(1), Some(2), Some(3)]
    );
    steps(&mut m, 2);
    assert_eq!((m.s(1), m.s(2)), (Some(11), None));
}

#[test]
fn the_exchange_package_words_follow_their_registers() {
    // A package word is undefined if a register in it is: P shares word 0
    // with A0, VL shares word 3 with XA, F and A3.
    let mut m = user(&cal("EX"), 0, LA_MAX);
    m.set_a(1, None);
    m.set_s(4, None);
    steps(&mut m, 1);
    for addr in 0..16 {
        assert_eq!(
            m.mem(addr).is_none(),
            addr == 1 || addr == 12,
            "word {}",
            addr
        );
    }
    let mut m = user(&cal("EX"), 0, LA_MAX);
    m.set_vl(None);
    steps(&mut m, 1);
    assert_eq!((m.mem(2).is_some(), m.mem(3)), (true, None));
}

fn assert_undefined(e: &TestError, what: &str) {
    assert_eq!(e.kind, ErrorKind::UndefinedValue);
    assert!(
        e.detail.contains(what),
        "`{}` does not mention `{}`",
        e.detail,
        what
    );
    assert!(e.to_string().starts_with("undefined value used: "), "{}", e);
}

#[test]
fn control_flow_on_an_undefined_value_is_a_test_error() {
    let mut m = bare("JAZ 1234");
    let e = error_of(&mut m);
    assert_undefined(&e, "A0");
    assert_eq!((e.p, e.parcels), (CODE * 4, Some((0o010000, Some(0o1234)))));
    assert_eq!(
        e.to_string(),
        "undefined value used: A0 (branch condition) at P=00001000 (010000 001234  JAZ       1234)"
    );
    // the machine stays stopped and nothing changed
    assert_eq!(m.step(), StepResult::Error(e.clone()));
    assert_eq!(m.run(5), RunResult::Error(e));
    assert_eq!(m.p(), CODE * 4 + 2);

    for source in ["JAN 1234", "JAP 1234", "JAM 1234"] {
        assert_undefined(&error_of(&mut bare(source)), "A0");
    }
    for source in ["JSZ 1234", "JSN 1234", "JSP 1234", "JSM 1234"] {
        assert_undefined(&error_of(&mut bare(source)), "S0");
    }
    assert_undefined(&error_of(&mut bare("J B5")), "Bjk");
    // an unconditional branch needs nothing
    let mut m = bare("J 1234");
    steps(&mut m, 1);
}

#[test]
fn addresses_counts_and_lengths_must_be_defined() {
    let cases = [
        ("S1 0,A2", "Ah"),
        ("A1 5,A2", "Ah"),
        ("0,A2 S1", "Ah"),
        ("0,A2 A1", "Ah"),
        ("B0,A1 ,A0", "A0"),
        (",A0 T0,A1", "A0"),
        ("S1 S1,S2<A3", "shift count"),
        ("S1 S2,S1>A3", "shift count"),
        ("S1 V2,A3", "element number"),
        ("V2,A3 S1", "element number"),
        ("XA A3", "XA"),
    ];
    for (source, what) in cases {
        assert_undefined(&error_of(&mut bare(source)), what);
    }
    // the block length
    let mut m = bare("B0,A1 ,A0");
    m.set_a(0, Some(0o1000));
    assert_undefined(&error_of(&mut m), "length");
    // h = 0, k = 0: the register is not read
    let mut m = bare("S1 1000,0; S2 S2,S3<1");
    steps(&mut m, 2);
}

#[test]
fn vector_instructions_need_vl_and_their_addresses() {
    for source in [
        "V1 V2+V3",
        "V1 S2&V3",
        "V1 V2<A3",
        "VM V1,Z",
        "V1 ,A0,1",
        ",A0,1 V1",
        "V1 /HV2",
        "V1 S2!V3&VM",
    ] {
        let mut m = bare(source);
        m.set_a(0, Some(0o1000));
        m.set_a(3, Some(1));
        assert_undefined(&error_of(&mut m), "VL");
    }
    let mut m = bare("V1 V2<A3");
    m.set_vl(Some(1));
    assert_undefined(&error_of(&mut m), "shift count");
    let mut m = bare("V1 ,A0,1");
    m.set_vl(Some(1));
    assert_undefined(&error_of(&mut m), "A0");
    let mut m = bare(",A0,A2 V1");
    m.set_vl(Some(1));
    m.set_a(0, Some(0o1000));
    assert_undefined(&error_of(&mut m), "increment");
    // setting VL from an undefined register is not an error by itself
    let mut m = bare("VL A1; A2 5");
    steps(&mut m, 2);
    assert_eq!(m.vl(), None);
}

#[test]
fn executing_undefined_memory_is_a_test_error() {
    let mut m = bare("J 4000");
    steps(&mut m, 1);
    let e = error_of(&mut m);
    assert_undefined(&e, "instruction fetch");
    assert_eq!((e.p, e.parcels), (0o4000, None));
    assert_eq!(
        e.to_string(),
        "undefined value used: instruction fetch from undefined memory word 1000 at P=00004000"
    );

    // the second parcel of an instruction in an undefined word
    let mut m = Machine::new();
    m.load_words(CODE, &[0o040100]).unwrap(); // S1 exp in the last parcel
    m.start_at(CODE * 4 + 3, 0, LA_MAX, mode::MONITOR);
    let e = error_of(&mut m);
    assert_undefined(&e, "instruction fetch");
    assert_eq!(e.parcels, Some((0o040100, None)));

    // a word made undefined by the program
    let mut m = bare("1000,0 S1; J 4000");
    m.load_words(0o1000, &words(&cal("A1 1"))).unwrap();
    steps(&mut m, 2);
    assert_undefined(&error_of(&mut m), "instruction fetch");

    // the I/O page cannot be executed
    let mut m = bare("J 17777700");
    steps(&mut m, 1);
    assert_undefined(&error_of(&mut m), "I/O page");
}

#[test]
fn io_writes_must_be_defined() {
    assert_undefined(&error_of(&mut bare("3777761,0 S1")), "console");
    assert_undefined(&error_of(&mut bare("3777762,0 S1")), "TEST_EXIT");
    assert_undefined(&error_of(&mut bare("3777762,0 A1")), "TEST_EXIT");
    // the unused words of the page ignore writes of anything
    let mut m = bare("3777770,0 S1; 3777763,0 S1");
    steps(&mut m, 2);
}

#[test]
fn an_exchange_needs_defined_control_words() {
    // The dead start package.
    let mut m = Machine::new();
    m.load_words(0, &[0, 0, 0]).unwrap(); // word 3 (XA, VL, F) is missing
    let e = error_of(&mut m);
    assert_undefined(&e, "word 3 of the exchange package at 0");
    // An exit into a package the program left undefined.
    let mut m = monitor_cal("A1 400; XA A1; EX");
    steps(&mut m, 2);
    let e = error_of(&mut m);
    assert_undefined(&e, "word 0 of the exchange package at 400");
    assert_eq!(m.mem(0o400), None, "nothing was stored");
    // Words 4 to 15 may be undefined: the registers just are.
    let mut m = monitor_cal("A1 400; XA A1; EX");
    let mut package = [0u64; 4];
    package[0] = 0o3000 << 24;
    package[2] = (LA_MAX as u64) << 28 | (mode::MONITOR as u64) << 24;
    m.load_words(0o400, &package).unwrap();
    steps(&mut m, 3);
    assert_eq!(
        (m.p(), m.a(3), m.a(4), m.s(0)),
        (0o3000, Some(0), None, None)
    );
}

#[test]
fn floating_point_operands_with_the_interrupt_enabled() {
    // If the interrupt is enabled in a user program an undefined operand
    // decides whether the program is interrupted: a test error.
    let mut m = user(&cal("EFI; S1 S2+FS3"), 0, LA_MAX);
    m.set_s(2, None);
    steps(&mut m, 1);
    assert_undefined(&error_of(&mut m), "floating point operand");
    let mut m = user(&cal("EFI; V1 V2*FV3"), 0, LA_MAX);
    m.set_vl(Some(1));
    steps(&mut m, 1);
    assert_undefined(&error_of(&mut m), "floating point operand");
    // disabled, or in monitor mode: just an undefined result
    let mut m = user(&cal("S1 S2+FS3; S4 /HS2"), 0, LA_MAX);
    m.set_s(2, None);
    steps(&mut m, 2);
    assert_eq!((m.s(1), m.s(4)), (None, None));
    let mut m = monitor_cal("EFI; S1 S2+FS3");
    m.set_s(2, None);
    steps(&mut m, 2);
    assert_eq!(m.s(1), None);
}

#[test]
fn constants_the_manual_lists_are_defined() {
    // Page 4-33: 044 "Si is cleared if the j designator is zero"; 045 "Si
    // is cleared if the j and k designators have the same value or if the j
    // designator is zero".  Page 4-34: 046 "Si is cleared if the j and k
    // designators have the same nonzero value"; 047 "Si is set to all ones
    // if the j and k designators have the same nonzero value".  Page 4-26:
    // 032 "(Ai) = 0 if j = 0".  Appendix D: 140i00 "Clear Vi".
    let mut m =
        bare("S1 S0&S7; S2 #S7&S7; S3 #S7&S0; S4 S7\\S7; S5 #S7\\S7; A1 A0*A7; V1 0; S6 S7&S7");
    m.set_vl(Some(2));
    steps(&mut m, 8);
    assert_eq!(
        (m.s(1), m.s(2), m.s(3), m.s(4), m.s(5)),
        (Some(0), Some(0), Some(0), Some(0), Some(u64::MAX))
    );
    assert_eq!(m.a(1), Some(0));
    assert_eq!((m.v(1, 0), m.v(1, 1), m.v(1, 2)), (Some(0), Some(0), None));
    assert_eq!(m.s(6), None, "(S7) and (S7) is (S7), which is undefined");
}

#[test]
fn a_register_less_itself_is_zero_whatever_it_holds() {
    // No manual lists these, but the result does not depend on the register:
    // 031 and 061 with j = k not 0, and the vector forms 145 and 157 with
    // j = k.  `Vi Vi\\Vi` (145iii) is how CAL clears a vector register from
    // 1982 on, also one that was never written.
    let mut m = bare("A1 A7-A7; S1 S7-S7; V1 V2\\V2; V3 V4-V4; V5 V5\\V5; V6 V6-V6; V7 V2\\V4");
    m.set_vl(Some(2));
    steps(&mut m, 7);
    assert_eq!((m.a(1), m.s(1)), (Some(0), Some(0)));
    for i in [1, 3, 5, 6] {
        assert_eq!(
            (m.v(i, 0), m.v(i, 1), m.v(i, 2)),
            (Some(0), Some(0), None),
            "V{}",
            i
        );
    }
    // different registers: undefined as ever
    assert_eq!(m.v(7, 0), None);
    // register 0 named as j and k is a pair of constants, not a register
    let mut m = bare("A1 A0-A0; S1 S0-S0");
    steps(&mut m, 2);
    assert_eq!((m.a(1), m.s(1)), (Some(A_MASK), Some(1 << 63)));
}

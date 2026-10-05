//! Dead start, the exchange sequence, exits, flags, monitor functions and
//! branches.

use super::*;
use cray1_fp::{pack, unpack};

#[test]
fn exchange_package_layout() {
    // Figure 3-8, page 3-37 (bit 0 is the leftmost bit of a word):
    //   word n    P  bits 18-39                              A0 bits 40-63
    //   word n+1  BA bits 18-35                              A1
    //   word n+2  LA bits 18-35, M bits 36-39                A2
    //   word n+3  XA bits 16-23, VL bits 24-30, F bits 31-39 A3
    //   n+4 to n+7   A4 to A7;  n+8 to n+15   S0 to S7
    // A field that ends at bit b is shifted left 63 - b places.
    let mut m = Machine::new();
    let ones: [u64; 16] = [u64::MAX; 16];
    m.load_words(0, &ones).unwrap();
    steps(&mut m, 1); // dead start with an all-ones package
    assert_eq!(m.p(), 0x3f_ffff); // 22 bits
    assert_eq!(m.ba(), 0x3_ffff); // 18 bits
    assert_eq!(m.la(), 0x3_ffff);
    assert_eq!(m.m(), 0xf); // 4 bits
    assert_eq!(m.xa(), 0xff); // 8 bits
    assert_eq!(m.vl(), Some(0x7f)); // 7 bits
    assert_eq!(m.f(), 0x1ff); // 9 bits
    for i in 0..8 {
        assert_eq!(m.a(i), Some(0xff_ffff));
        assert_eq!(m.s(i), Some(u64::MAX));
    }
    let w = m.exchange_package();
    assert_eq!(w[0], Some(0x3f_ffff << 24 | 0xff_ffff)); // bits 18 to 63
    assert_eq!(w[1], Some(0x3_ffff << 28 | 0xff_ffff)); // 18-35, 40-63
    assert_eq!(w[2], Some(0x3_ffff << 28 | 0xf << 24 | 0xff_ffff)); // 18-63
    assert_eq!(
        w[3],
        Some(0xff << 40 | 0x7f << 33 | 0x1ff << 24 | 0xff_ffff)
    ); // 16-63
    assert_eq!(w[4], Some(0xff_ffff));
    assert_eq!(w[15], Some(u64::MAX));

    // each field alone
    let mut m = Machine::new();
    let mut package = [0u64; 16];
    package[0] = 0o1234567 << 24 | 0o100;
    package[1] = 0o654321 << 28 | 0o101;
    package[2] = 0o123456 << 28 | 0o5 << 24 | 0o102;
    package[3] = 0o321 << 40 | 0o143 << 33 | 0o0 << 24 | 0o103;
    for n in 4..16 {
        package[n] = 0o100 + n as u64;
    }
    m.load_words(0, &package).unwrap();
    steps(&mut m, 1);
    assert_eq!((m.p(), m.ba(), m.la()), (0o1234567, 0o654321, 0o123456));
    assert_eq!((m.m(), m.xa(), m.vl(), m.f()), (0o5, 0o321, Some(0o143), 0));
    for i in 0..8 {
        assert_eq!(m.a(i), Some(0o100 + i as u32));
        assert_eq!(m.s(i), Some(0o110 + i as u64));
    }
    assert_eq!(m.exchange_package().map(|w| w.unwrap()), package);
}

#[test]
fn mode_and_flag_bit_assignments() {
    // Page 3-35: M bit 36 correctable memory error mode, 37 floating point
    // error mode, 38 uncorrectable memory error mode, 39 monitor mode.
    // Figure 3-8 (rev F): F bit 31 programmable clock, 32 MCU interrupt, 33 floating
    // point error, 34 operand range, 35 program range, 36 memory error,
    // 37 I/O interrupt, 38 error exit, 39 normal exit.
    // M occupies bits 36-39 and F bits 31-39 of their words, so the bit
    // numbered b has the value 2**(39 - b) in the register.
    assert_eq!(mode::CORRECTABLE_MEMORY, 1 << (39 - 36));
    assert_eq!(mode::FLOATING_POINT, 1 << (39 - 37));
    assert_eq!(mode::UNCORRECTABLE_MEMORY, 1 << (39 - 38));
    assert_eq!(mode::MONITOR, 1 << (39 - 39));
    assert_eq!(flag::PROGRAMMABLE_CLOCK, 1 << (39 - 31));
    assert_eq!(flag::MCU_INTERRUPT, 1 << (39 - 32));
    assert_eq!(flag::FLOATING_POINT, 1 << (39 - 33));
    assert_eq!(flag::OPERAND_RANGE, 1 << (39 - 34));
    assert_eq!(flag::PROGRAM_RANGE, 1 << (39 - 35));
    assert_eq!(flag::MEMORY_ERROR, 1 << (39 - 36));
    assert_eq!(flag::IO_INTERRUPT, 1 << (39 - 37));
    assert_eq!(flag::ERROR_EXIT, 1 << (39 - 38));
    assert_eq!(flag::NORMAL_EXIT, 1 << (39 - 39));
}

#[test]
fn dead_start() {
    // Page 3-40: the dead start forces XA to zero and an error exit: the
    // package at address zero moves into the registers; what is stored at
    // address zero "is largely noise".  Page 3-44: registers and memory are
    // noisy after power on.
    let mut image = vec![0u64; 0o201];
    image[0] = (CODE as u64 * 4) << 24 | 7;
    image[2] = (LA_MAX as u64) << 28 | (mode::MONITOR as u64) << 24;
    image[3] = 0o20 << 40 | 0o10 << 33;
    image[9] = 0x1111;
    image[CODE as usize] = words(&cal("A1 5"))[0];
    let bytes: Vec<u8> = image.iter().flat_map(|w| w.to_be_bytes()).collect();
    let mut m = Machine::with_image(&bytes).unwrap();
    assert!(!m.started());
    assert_eq!(
        (
            m.a(0),
            m.s(0),
            m.b(0),
            m.t(0),
            m.v(0, 0),
            m.vl(),
            m.vm(),
            m.rtc()
        ),
        (None, None, None, None, None, None, None, None)
    );
    assert_eq!(m.mem(0o201), None, "memory beyond the image is undefined");
    assert_eq!(m.mem(5), Some(0));
    let events = record(&mut m);
    steps(&mut m, 1);
    assert!(m.started());
    assert_eq!(
        (m.p(), m.ba(), m.la(), m.m(), m.xa(), m.vl(), m.f()),
        (CODE * 4, 0, LA_MAX, mode::MONITOR, 0o20, Some(0o10), 0)
    );
    assert_eq!(
        (m.a(0), m.a(1), m.s(1), m.s(0)),
        (Some(7), Some(0), Some(0x1111), Some(0))
    );
    assert_eq!(
        (m.b(0), m.t(0), m.v(0, 0), m.vm()),
        (None, None, None, None)
    );
    for addr in 0..16 {
        assert_eq!(
            m.mem(addr),
            None,
            "word {} holds the reset-state package",
            addr
        );
        assert!(m.mem_written(addr));
    }
    assert_eq!(m.instructions(), 0);
    {
        let events = events.borrow();
        assert_eq!(events[0], Event::ExchangeStart { xa: 0 });
        for n in 0..16 {
            assert_eq!(
                events[1 + n],
                Event::Mem {
                    addr: n as u32,
                    value: None
                }
            );
        }
        assert_eq!(events[17], Event::P(CODE * 4));
        assert_eq!(*events.last().unwrap(), Event::ExchangeEnd);
    }
    steps(&mut m, 1);
    assert_eq!(m.a(1), Some(5));
    assert_eq!(m.instructions(), 1);
    assert_eq!(m.steps(), 2);
}

#[test]
fn normal_exit_of_a_user_program() {
    // Page 4-13: an exchange occurs to the package designated by XA; if
    // monitor mode is not in effect the normal exit flag sets; the program
    // address stored is advanced one count from the address of the exit.
    // Page 3-38: B, T and V are not swapped.
    let mut m = user(&cal("A1 5; EX; A1 6"), 0o100, 0o200);
    m.set_s(3, Some(0xabc));
    m.set_b(5, Some(55));
    m.set_vl(Some(0o33));
    let events = record(&mut m);
    steps(&mut m, 2);
    // the monitor package of words 0-17 is active
    assert!(m.monitor_mode());
    assert_eq!(
        (m.p(), m.ba(), m.la(), m.xa(), m.f(), m.vl()),
        (0o100 * 4, 0, LA_MAX, 0, 0, Some(0))
    );
    assert_eq!((m.a(1), m.s(3)), (Some(0), Some(0)));
    assert_eq!(m.b(5), Some(55));
    // the user's package is at word 0
    let (p, ba, la, mode, xa, vl, f) = package_fields(&m, 0);
    assert_eq!(p, CODE * 4 + 2, "one parcel past the EX at parcel 1");
    assert_eq!((ba, la, mode, xa, vl), (0o100, 0o200, 0, 0, 0o33));
    assert_eq!(f, flag::NORMAL_EXIT);
    assert_eq!(m.mem(1).unwrap() & 0xff_ffff, 5, "A1");
    assert_eq!(m.mem(8 + 3), Some(0xabc), "S3");
    let events = events.borrow();
    let flags = events
        .iter()
        .position(|e| *e == Event::Flags(flag::NORMAL_EXIT))
        .unwrap();
    let start = events
        .iter()
        .position(|e| *e == Event::ExchangeStart { xa: 0 })
        .unwrap();
    assert!(flags < start);
    assert_eq!(*events.last().unwrap(), Event::ExchangeEnd);
}

#[test]
fn error_exit_of_a_user_program() {
    // Page 4-7: as the normal exit, with the error exit flag.
    let mut m = user(&[0o000000], 0, LA_MAX);
    steps(&mut m, 1);
    let (p, _, _, _, _, _, f) = package_fields(&m, 0);
    assert_eq!(f, flag::ERROR_EXIT);
    assert_eq!(p, CODE * 4 + 1);
    // the ignored fields do not matter: 000xxx
    let mut m = user(&[0o000777], 0, LA_MAX);
    steps(&mut m, 1);
    assert_eq!(package_fields(&m, 0).6, flag::ERROR_EXIT);
}

#[test]
fn exits_in_monitor_mode_exchange_without_a_flag() {
    // Pages 3-36 and 3-41: the flag sets "providing the currently active
    // exchange package is not in monitor mode"; the exchange happens
    // either way (it is how the monitor starts a program).
    for parcel in [0o004000u16, 0o000000] {
        let mut m = monitor(&[0o022120, 0o001310, parcel]); // A1 20; XA A1; exit
        let mut package = [0u64; 16];
        package[0] = 0o3000 << 24;
        package[2] = (LA_MAX as u64) << 28; // a user program
        package[3] = 0o1 << 40;
        m.load_words(0o20, &package).unwrap();
        steps(&mut m, 3);
        assert_eq!((m.p(), m.m(), m.xa(), m.f()), (0o3000, 0, 1, 0));
        let (p, _, _, mode, xa, _, f) = package_fields(&m, 0o20);
        assert_eq!((p, mode, xa, f), (CODE * 4 + 3, mode::MONITOR, 1, 0));
    }
}

#[test]
fn a_package_that_arrives_with_a_flag_set_exchanges_at_once() {
    // Page 3-36: "If any of the bits is set, another exchange will occur
    // immediately."
    let mut m = monitor_cal("A1 400; XA A1; EX; A2 7");
    let mut package = [0u64; 16];
    package[0] = 0o3000 << 24;
    package[2] = (LA_MAX as u64) << 28; // a user program
    package[3] = 0o20 << 40 | (flag::NORMAL_EXIT as u64) << 24; // XA back to 400
    m.load_words(0o400, &package).unwrap();
    steps(&mut m, 3);
    assert_eq!(
        (m.p(), m.f(), m.monitor_mode()),
        (0o3000, flag::NORMAL_EXIT, false)
    );
    let issued = m.instructions();
    steps(&mut m, 1); // no instruction: the exchange back
    assert_eq!(m.instructions(), issued);
    assert!(m.monitor_mode());
    assert_eq!(m.p(), CODE * 4 + 4, "one parcel past the EX at parcel 3");
    assert_eq!(
        package_fields(&m, 0o400).0,
        0o3000,
        "the user program did not run"
    );
    steps(&mut m, 1);
    assert_eq!(m.a(2), Some(7));

    // In monitor mode all interrupts but memory errors are inhibited
    // (page 3-35): a monitor package with a flag set just runs.
    let mut m = monitor_cal("A1 400; XA A1; EX");
    package[2] |= (mode::MONITOR as u64) << 24;
    m.load_words(0o400, &package).unwrap();
    m.load_words(0o3000 / 4, &words(&cal("A3 3"))).unwrap();
    steps(&mut m, 4);
    assert_eq!((m.a(3), m.f()), (Some(3), flag::NORMAL_EXIT));
}

#[test]
fn monitor_functions() {
    // Page 4-8: 0013 transmits bits 2**11 to 2**4 of (Aj) to XA; XA is
    // cleared if j = 0.  0014 enters the clock.  Page 4-9: 0010 to 0012 are
    // no-ops here (no channels); i = 5, 6, 7 pass.
    let mut m = monitor(&[
        0o001310, 0o001000, 0o001100, 0o001200, 0o001500, 0o001677, 0o001777, 0o001420, 0o001300,
    ]);
    m.set_a(1, Some(0xab_cdef));
    m.set_s(2, Some(99));
    let events = record(&mut m);
    steps(&mut m, 1);
    assert_eq!(m.xa(), 0xde);
    steps(&mut m, 7);
    assert_eq!(m.xa(), 0xde);
    assert!(events.borrow().contains(&Event::Rtc(Some(99))));
    assert_eq!(m.rtc(), None, "the clock keeps counting clock periods");
    steps(&mut m, 1);
    assert_eq!(m.xa(), 0);
    assert_eq!(m.p(), CODE * 4 + 9);
}

#[test]
fn monitor_functions_pass_in_a_user_program() {
    // Page 4-9: "If the program is not in monitor mode, instruction becomes
    // a no-op".
    let mut m = user(&[0o001310, 0o001420, 0o001300], 0, LA_MAX);
    m.set_a(1, Some(0xab_cdef));
    let events = record(&mut m);
    steps(&mut m, 3);
    assert_eq!(m.xa(), 0);
    assert!(!events
        .borrow()
        .iter()
        .any(|e| matches!(e, Event::Xa(_) | Event::Rtc(_))));
    assert_eq!((m.p(), m.f()), (CODE * 4 + 3, 0));
}

#[test]
fn vector_length_register() {
    // Page 4-10: the low seven bits of (Ak) enter VL; (Ak) = 1 if k = 0;
    // the number of operations is ((VL) - 1) in six bits, plus one: 64 for
    // VL = 0 and for VL = 100 octal.
    let mut m = monitor(&[0o002001, 0o002000, 0o002002]);
    m.set_a(1, Some(0o1234));
    m.set_a(2, Some(0));
    m.set_a(0, Some(50));
    steps(&mut m, 1);
    assert_eq!(m.vl(), Some(0o34));
    assert_eq!(m.vector_length(), Some(0o34));
    steps(&mut m, 1);
    assert_eq!(m.vl(), Some(1));
    steps(&mut m, 1);
    assert_eq!(m.vl(), Some(0));
    assert_eq!(m.vector_length(), Some(64));
    assert_eq!(vl_count(0o100), 64);
    assert_eq!(vl_count(0), 64);
    assert_eq!(vl_count(1), 1);
    assert_eq!(vl_count(0o77), 63);
    assert_eq!(vl_count(0o101), 1);
    assert_eq!(vl_count(0o177), 63);
}

#[test]
fn floating_point_mode_flag_and_vector_mask() {
    // Page 4-11: 0021 sets and 0022 clears the floating point mode flag in
    // M (bit 37, page 3-35), in any mode: "can be set or cleared by an
    // object program" (page 3-21).  Page 4-12: 003 enters (Sj) into VM, 0
    // if j = 0.
    for user_mode in [false, true] {
        let code = cal("EFI; DFI; VM S3; VM 0");
        let mut m = if user_mode {
            user(&code, 0, LA_MAX)
        } else {
            monitor(&code)
        };
        let base = m.m();
        m.set_s(3, Some(0xf00f));
        m.set_s(0, Some(0x1234));
        steps(&mut m, 1);
        assert_eq!(m.m(), base | mode::FLOATING_POINT);
        steps(&mut m, 1);
        assert_eq!(m.m(), base);
        steps(&mut m, 1);
        assert_eq!(m.vm(), Some(0xf00f));
        steps(&mut m, 1);
        assert_eq!(m.vm(), Some(0));
    }
}

#[test]
fn unconditional_branches() {
    // Page 4-15: 006 sets P to the low 22 bits of ijkm.  Page 4-14: 005 sets
    // P to (Bjk).  Page 4-16: 007 sets B00 to the address of the following
    // parcel, then branches.
    let mut m = monitor_cal(
        "J 1010; ERR; ERR; ERR; ERR; ERR; ERR; R 1020; ERR; ERR; ERR; ERR; ERR; ERR; J B0; ERR",
    );
    steps(&mut m, 1);
    assert_eq!(m.p(), 0o1010);
    steps(&mut m, 1);
    assert_eq!(m.p(), 0o1020);
    assert_eq!(m.b(0), Some(0o1012));
    steps(&mut m, 1);
    assert_eq!(m.p(), 0o1012);

    let mut m = monitor_cal("J B77");
    m.set_b(0o77, Some(0o2345));
    steps(&mut m, 1);
    assert_eq!(m.p(), 0o2345);
}

#[test]
fn conditional_branches_on_a0() {
    // Page 4-17: 010 (A0) = 0, 011 (A0) != 0, 012 (A0) positive, 013 (A0)
    // negative; "(A0) = 0 is considered a positive condition".
    let taken = |op: u16, a0: u32| {
        let mut m = monitor(&[op << 9, 0o1234]);
        m.set_a(0, Some(a0));
        steps(&mut m, 1);
        match m.p() {
            0o1234 => true,
            p if p == CODE * 4 + 2 => false,
            p => panic!("P = {:o}", p),
        }
    };
    let cases = [
        (0u32, [true, false, true, false]),
        (1, [false, true, true, false]),
        (0x7f_ffff, [false, true, true, false]),
        (0x80_0000, [false, true, false, true]),
        (0xff_ffff, [false, true, false, true]),
    ];
    for (a0, expect) in cases {
        for (n, op) in [0o010u16, 0o011, 0o012, 0o013].into_iter().enumerate() {
            assert_eq!(taken(op, a0), expect[n], "{:03o} with A0 = {:x}", op, a0);
        }
    }
}

#[test]
fn conditional_branches_on_s0() {
    // Page 4-18: 014 (S0) = 0, 015 (S0) != 0, 016 (S0) positive, 017 (S0)
    // negative; "(S0) = 0 is considered a positive condition".
    let taken = |op: u16, s0: u64| {
        let mut m = monitor(&[op << 9, 0o1234]);
        m.set_s(0, Some(s0));
        steps(&mut m, 1);
        m.p() == 0o1234
    };
    let cases = [
        (0u64, [true, false, true, false]),
        (1, [false, true, true, false]),
        (1 << 62, [false, true, true, false]),
        (1 << 63, [false, true, false, true]),
        (u64::MAX, [false, true, false, true]),
    ];
    for (s0, expect) in cases {
        for (n, op) in [0o014u16, 0o015, 0o016, 0o017].into_iter().enumerate() {
            assert_eq!(taken(op, s0), expect[n], "{:03o} with S0 = {:x}", op, s0);
        }
    }
}

#[test]
fn two_parcel_instruction_across_a_word_boundary() {
    // Page 4-1: "A two-parcel instruction that begins in the fourth parcel
    // of a word ends in the first parcel of the next word."
    let mut m = monitor_cal("A1 1; A2 2; A3 3; S1 1234567; A4 4");
    steps(&mut m, 5);
    assert_eq!(m.s(1), Some(0o1234567));
    assert_eq!(m.a(4), Some(4));
    assert_eq!(m.p(), CODE * 4 + 6);
}

#[test]
fn branch_address_bits_above_p() {
    // Page 4-4: "A program range error occurs if either of the two low-order
    // bits of i is set; the high-order bit of i is ignored."  Pages 4-15 to
    // 4-18: P gets the low order 22 bits.
    // 006400 001234: only the ignored bit of i is set
    let mut m = user(&[0o006400, 0o001234], 0, LA_MAX);
    steps(&mut m, 1);
    assert_eq!((m.p(), m.f()), (0o1234, 0));
    // 006100 001234: bit 2**22 of the address
    for i in [0o100u16, 0o200, 0o300] {
        let mut m = user(&[0o006000 | i, 0o001234], 0, LA_MAX);
        steps(&mut m, 1);
        let (p, _, _, _, _, _, f) = package_fields(&m, 0);
        assert_eq!(f, flag::PROGRAM_RANGE);
        assert_eq!(p, 0o1234);
    }
    // a conditional branch that is not taken raises nothing (model choice)
    let mut m = user(&[0o011100, 0o001234, 0o004000], 0, LA_MAX);
    steps(&mut m, 1);
    assert_eq!((m.p(), m.f()), (CODE * 4 + 2, 0));
    // 005 with a 24-bit (Bjk)
    let mut m = user(&cal("J B1"), 0, LA_MAX);
    m.set_b(1, Some(0x40_0000 | 0o1234));
    steps(&mut m, 1);
    assert_eq!(package_fields(&m, 0).6, flag::PROGRAM_RANGE);
    // in monitor mode no flag sets and execution continues at the 22 bits
    let mut m = monitor(&[0o006300, 0o001234]);
    m.load_words(0o1234 / 4, &words(&cal("A1 5"))).unwrap();
    steps(&mut m, 2);
    assert_eq!((m.a(1), m.f()), (Some(5), 0));
}

#[test]
fn program_range_error_on_fetch() {
    // Pages 3-43 and 3-44: the final address that can be executed is
    // (LA) * 16 - 1; beyond it a user program gets the program range flag
    // (bit 35).  BA = 100, LA = 112: relative words 0 to 237.
    let mut m = user(&cal("J 1174"), 0o100, 0o112);
    // the last two parcels of the field and what lies beyond
    m.load_words(
        0o2237,
        &[
            words(&cal("PASS; PASS; A1 1; A2 2"))[0],
            words(&cal("A3 3"))[0],
        ],
    )
    .unwrap();
    steps(&mut m, 5);
    assert_eq!((m.a(1), m.a(2), m.f()), (Some(1), Some(2), 0));
    assert_eq!(m.p(), 0o1200);
    steps(&mut m, 1);
    assert!(m.monitor_mode());
    let (p, _, _, _, _, _, f) = package_fields(&m, 0);
    assert_eq!(f, flag::PROGRAM_RANGE);
    assert_eq!(p, 0o1200, "the address that could not be fetched");
    assert_eq!(m.mem(3).unwrap() & 0xff_ffff, 0, "A3 3 did not execute");

    // the second parcel of an instruction beyond the limit: model choice,
    // the saved P is the address of the instruction
    let mut m = user(&cal("J 1177"), 0o100, 0o112);
    m.load_words(0o2237, &[0o040100, 0x0005_0000_0000_0000])
        .unwrap(); // S1 5 across the limit
    steps(&mut m, 2);
    let (p, _, _, _, _, _, f) = package_fields(&m, 0);
    assert_eq!((p, f), (0o1177, flag::PROGRAM_RANGE));
    assert_eq!(m.mem(8 + 1), Some(0), "S1 was not loaded");
}

#[test]
fn program_range_error_in_monitor_mode_stops_the_model() {
    // Page 3-36: the flag cannot set in monitor mode.  What the machine
    // then executes is not defined by the manual.
    let mut m = Machine::new();
    m.load_words(CODE, &words(&cal("J 2000"))).unwrap();
    m.load_words(0o400, &words(&cal("A1 1"))).unwrap();
    m.start_at(CODE * 4, 0, 0o20, mode::MONITOR); // limit: word 400
    define_registers(&mut m);
    steps(&mut m, 1);
    let e = error_of(&mut m);
    assert_eq!(e.kind, ErrorKind::NotDefinedByManual);
    assert_eq!(e.p, 0o2000);
    assert_eq!(m.run(10), RunResult::Error(e));
}

#[test]
fn undefined_opcodes_stop_the_model() {
    // 0023xx to 0027xx are not in the manual (page 4-11 ends at 0022).
    for parcel in [0o002300u16, 0o002477, 0o002700] {
        let mut m = monitor(&[parcel]);
        let e = error_of(&mut m);
        assert_eq!(e.kind, ErrorKind::NotDefinedByManual);
        assert_eq!((e.p, e.parcels), (CODE * 4, Some((parcel, None))));
    }
}

#[test]
fn floating_point_error_flag() {
    // Page 3-21: overflow "will initiate an interrupt if the floating point
    // mode flag is set in the mode register and monitor mode is not in
    // effect".  Page 3-22: for the multiply unit, if the sum of the
    // exponents is 60000 or more "the floating point error flag is set and
    // an exponent of 60000 is sent to the result register".
    let big = pack(false, 0o57777, 0x8000_0000_0000);
    // user program, mode flag set: flag and exchange
    let mut m = user(&cal("EFI; S3 S1*FS2; A1 5"), 0, LA_MAX);
    m.set_s(1, Some(big));
    m.set_s(2, Some(big));
    steps(&mut m, 2);
    assert!(m.monitor_mode());
    let (p, _, _, mode, _, _, f) = package_fields(&m, 0);
    assert_eq!(f, flag::FLOATING_POINT);
    assert_eq!(mode, mode::FLOATING_POINT);
    assert_eq!(
        p,
        CODE * 4 + 2,
        "model choice: the instruction after the error"
    );
    assert_eq!(unpack(m.mem(8 + 3).unwrap()).exp, 0o60000);
    // user program, mode flag clear: no interrupt
    let mut m = user(&cal("DFI; S3 S1*FS2; A1 5"), 0, LA_MAX);
    m.set_s(1, Some(big));
    m.set_s(2, Some(big));
    steps(&mut m, 3);
    assert_eq!((m.f(), m.a(1), m.monitor_mode()), (0, Some(5), false));
    assert_eq!(unpack(m.s(3).unwrap()).exp, 0o60000);
    // monitor mode, mode flag set: no interrupt
    let mut m = monitor_cal("EFI; S3 S1*FS2; A1 5");
    m.set_s(1, Some(big));
    m.set_s(2, Some(big));
    steps(&mut m, 3);
    assert_eq!((m.f(), m.a(1)), (0, Some(5)));
    // a vector instruction: every element is done, then the interrupt
    let mut m = user(&cal("EFI; V3 V1*FV2; A1 5"), 0, LA_MAX);
    m.set_vl(Some(3));
    let one = cray1_fp::from_f64(1.0).unwrap();
    for (e, x) in [one, big, one].into_iter().enumerate() {
        m.set_v(1, e, Some(x));
        m.set_v(2, e, Some(x));
    }
    steps(&mut m, 2);
    assert!(m.monitor_mode());
    assert_eq!(package_fields(&m, 0).6, flag::FLOATING_POINT);
    assert_eq!(m.v(3, 0), Some(one));
    assert_eq!(unpack(m.v(3, 1).unwrap()).exp, 0o60000);
    assert_eq!(m.v(3, 2), Some(one));
}

/// A machine in monitor mode running `source`, with a user package at word
/// 0 (XA = 0) whose program at word 300 is `A1 5`.
fn monitor_then_user(source: &str) -> Machine {
    let mut m = monitor_cal(source);
    let mut package = [0u64; 16];
    package[0] = (0o300u64 * 4) << 24;
    package[2] = (LA_MAX as u64) << 28;
    m.load_words(0, &package).unwrap();
    m.load_words(0o300, &words(&cal("A1 5; A2 6"))).unwrap();
    m
}

#[test]
fn programmable_clock() {
    // Rev F pages 4-10 and 6-23.  0014j4 enters the interval, 0014j5 clears
    // the request, 0014j6 enables and 0014j7 disables it; all four only in
    // monitor mode.  Nothing of the clock can be read by a program, and "a
    // request set in monitor mode is held until the system switches to user
    // mode": in monitor mode the instructions run on whatever the clock does.
    let mut m = monitor_cal("PCI S1; ECI; CCI; DCI; ECI; A1 5");
    steps(&mut m, 6);
    assert_eq!((m.a(1), m.f()), (Some(5), 0));
    // Outside monitor mode they are passes (page 4-11): the clock stays off.
    let mut m = user(&cal("PCI S1; ECI; A1 5; A2 6"), 0, LA_MAX);
    steps(&mut m, 4);
    assert_eq!((m.a(2), m.f()), (Some(6), 0));
    // Not enabled: a user program runs.  The same after the monitor has
    // disabled the request and cleared one that may be set.
    for source in ["PCI S1; EX", "ECI; DCI; CCI; EX", "ECI; CCI; DCI; CCI; EX"] {
        let mut m = monitor_then_user(source);
        let n = source.split(';').count();
        steps(&mut m, n + 1);
        assert!(!m.monitor_mode(), "{}", source);
        assert_eq!(m.a(1), Some(5), "{}", source);
    }
    // Enabled, or disabled with a request that may still be set (it "remains
    // set until a 0014j5"): the interrupt can come at any clock period,
    // which the model cannot count.  It stops at the first user instruction.
    for source in ["ECI; EX", "ECI; CCI; EX", "ECI; DCI; EX"] {
        let mut m = monitor_then_user(source);
        let n = source.split(';').count();
        steps(&mut m, n);
        assert!(!m.monitor_mode(), "{}", source);
        let e = error_of(&mut m);
        assert_eq!(e.kind, ErrorKind::TimeDependent, "{}", source);
        assert_eq!(e.p, 0o300 * 4);
    }
    // 0014jk with k = 1, 2 or 3 is a pass and does not enter the clock
    let mut m = monitor(&[0o001411, 0o001422, 0o001433, 0o020100, 5]);
    let events = record(&mut m);
    steps(&mut m, 4);
    assert_eq!(m.a(1), Some(5));
    assert!(!events.borrow().iter().any(|e| matches!(e, Event::Rtc(_))));
}

#[test]
fn run_stops_at_the_step_limit() {
    let mut m = monitor_cal("J 1000");
    assert_eq!(m.run(100), RunResult::Limit);
    assert_eq!(m.steps(), 100);
    assert_eq!(RunResult::Limit.exit_status(), 2);
}

/// `monitor_then_user` in timed mode at one clock period a step, with a user
/// program at word 300 that counts in A1 for ever.
fn timed(source: &str) -> Machine {
    let mut m = monitor_then_user(source);
    m.load_words(0o300, &words(&cal("A1 A1+1; J 1400")))
        .unwrap();
    m.set_timing(1);
    m
}

#[test]
fn timed_mode_clocks() {
    // The real-time clock counts clock periods (page 3-34): entered by 0014,
    // read by 072.  Here a step is three of them.
    let mut m = monitor_cal("RT S1; S2 RT; S3 RT; RT S0; S4 RT");
    m.set_timing(3);
    m.set_s(1, Some(1000));
    assert_eq!(m.rtc(), Some(0));
    steps(&mut m, 5);
    assert_eq!((m.s(2), m.s(3), m.s(4)), (Some(1003), Some(1006), Some(3)));
    assert_eq!(m.time(), 15);

    // The programmable clock (rev F pages 4-10 and 6-24).  PCI in the step
    // at time 1 with an interval of 20: the countdown is zero 20 clock
    // periods on and the request is made in the next, at time 22.  The user
    // program has run steps 4 to 21 by then: nine times round its loop.
    let mut m = timed("PCI S1; ECI; EX; CCI; DCI; A2 3");
    m.set_s(1, Some(20));
    steps(&mut m, 21);
    assert_eq!((m.monitor_mode(), m.a(1), m.f()), (false, Some(9), 0));
    steps(&mut m, 1);
    assert!(m.monitor_mode());
    assert_eq!(package_fields(&m, 0).6, flag::PROGRAMMABLE_CLOCK);
    assert_eq!(m.mem(1).unwrap() & 0xff_ffff, 9, "A1 of the program");
    // the request stays until 0014j5; the monitor goes on undisturbed
    steps(&mut m, 3);
    assert_eq!((m.a(2), m.f()), (Some(3), 0));

    // A request made in monitor mode is held and taken by the first step
    // outside it, before an instruction runs.
    let mut m = timed("PCI S1; ECI; A2 1; A2 2; A2 3; A2 4; EX");
    m.set_s(1, Some(2));
    steps(&mut m, 7);
    assert_eq!((m.monitor_mode(), m.f()), (false, 0));
    steps(&mut m, 1);
    assert!(m.monitor_mode());
    assert_eq!(package_fields(&m, 0).6, flag::PROGRAMMABLE_CLOCK);
    assert_eq!(m.mem(1).unwrap() & 0xff_ffff, 0);

    // Not enabled: the countdown runs but no request is made.  Disabling
    // does not take back a request; clearing does.
    let mut m = timed("PCI S1; EX");
    m.set_s(1, Some(2));
    steps(&mut m, 40);
    assert!(!m.monitor_mode());
    let mut m = timed("PCI S1; ECI; A2 1; A2 2; A2 3; DCI; EX");
    m.set_s(1, Some(2));
    steps(&mut m, 8);
    assert!(m.monitor_mode());
    let mut m = timed("PCI S1; ECI; A2 1; A2 2; A2 3; DCI; CCI; EX");
    m.set_s(1, Some(2));
    steps(&mut m, 40);
    assert!(!m.monitor_mode());
}

#[test]
fn requests_from_outside() {
    // The MCU interrupt (flag bit 32) and the I/O interrupt (bit 37) set
    // outside monitor mode only.  The MCU request is one event, kept until
    // its flag sets; the I/O request is a level.
    let mut m = timed("A2 1; A2 2; EX; A2 3; A2 4");
    m.request_mcu_interrupt();
    steps(&mut m, 3);
    assert_eq!((m.monitor_mode(), m.f()), (false, 0));
    steps(&mut m, 1);
    assert!(m.monitor_mode());
    assert_eq!(package_fields(&m, 0).6, flag::MCU_INTERRUPT);
    assert_eq!(
        m.mem(1).unwrap() & 0xff_ffff,
        0,
        "taken before the first instruction"
    );
    // taken once: the monitor clears the flag in the package and resumes
    let word3 = m.mem(3).unwrap() & !(0o777 << 24);
    m.store(3, Some(word3));
    let mut m2 = timed("EX");
    m2.set_io_request(true);
    steps(&mut m2, 2);
    assert!(m2.monitor_mode());
    assert_eq!(package_fields(&m2, 0).6, flag::IO_INTERRUPT);
    // In the untimed model a request works the same way.
    let mut m = monitor_then_user("EX");
    m.request_mcu_interrupt();
    steps(&mut m, 2);
    assert_eq!(package_fields(&m, 0).6, flag::MCU_INTERRUPT);
    // dead start: the next step exchanges with the package at word 0 again
    assert!(m.running());
    m.dead_start();
    assert!(!m.running());
    steps(&mut m, 1);
    assert!(m.running());
}

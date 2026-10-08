//! Memory references: 10h to 13h, the block transfers 034 to 037, the
//! vector transfers 176 and 177, the base and limit addresses of the
//! instruction and data fields.

use super::*;

#[test]
fn scalar_loads_and_stores() {
    // Page 4-47: the address is (Ah) plus the signed jkm; (Ah) = 0 if h = 0.
    // A loads ignore the upper 40 bits; A stores zero them.
    let mut m = monitor_cal(
        "S1 1000,A2; A3 1000,A2; 1001,A2 S4; 1002,0 A5; S6 -1,A7; A1 1002,0; S7 1002,0",
    );
    m.load_words(0o1010, &[0xdead_beef_0123_4567]).unwrap();
    m.load_words(0o1002, &[u64::MAX]).unwrap();
    m.set_a(0, Some(0o7777)); // h = 0 never reads A0
    m.set_a(2, Some(0o10));
    m.set_s(4, Some(0x1111_2222_3333_4444));
    m.set_a(5, Some(0xff_ffff));
    m.set_a(7, Some(0o1011));
    steps(&mut m, 7);
    assert_eq!(m.s(1), Some(0xdead_beef_0123_4567));
    assert_eq!(m.a(3), Some(0x23_4567));
    assert_eq!(m.mem(0o1011), Some(0x1111_2222_3333_4444));
    assert_eq!(m.mem(0o1002), Some(0x0000_0000_00ff_ffff));
    assert_eq!(m.s(6), Some(0xdead_beef_0123_4567));
    assert_eq!(m.a(1), Some(0xff_ffff));
    assert_eq!(m.s(7), Some(0xff_ffff));
    assert!(m.mem_written(0o1011) && m.mem_written(0o1002) && !m.mem_written(0o1010));
}

#[test]
fn base_address_relocates_data() {
    // X 3-19: absolute addresses are formed by adding (DBA) * 32 to the
    // relative address.
    let mut m = user(&cal("210,0 S1; S2 210,0; EX"), 0o100, 0o200);
    m.set_s(1, Some(0x77));
    steps(&mut m, 2);
    assert_eq!(m.mem(0o100 * 32 + 0o210), Some(0x77));
    assert_eq!(m.mem(0o210), None);
    assert_eq!(m.s(2), Some(0x77));
}

#[test]
fn operand_range_error_in_a_user_program() {
    // X 3-19 to 3-21: the last word a program can reference is
    // (DLA) * 32 - 1, an absolute address.  A store beyond the limit does
    // not alter memory and, for a program not in monitor mode whose
    // interrupt-on-operand-range mode is on, sets the operand range flag
    // (bit 34 of the flags), which interrupts it.
    // DBA = 100 (base 4000), DLA = 112 (limit 4500): relative 0 to 477.
    let mut m = user(&cal("477,0 S1; S2 477,0; 500,0 S1; A3 5"), 0o100, 0o112);
    m.set_s(1, Some(0x55));
    m.load_words(0o4500, &[0x1234]).unwrap();
    steps(&mut m, 2);
    assert_eq!(m.mem(0o4477), Some(0x55));
    assert_eq!(m.s(2), Some(0x55));
    assert_eq!(m.f(), 0);
    let events = record(&mut m);
    steps(&mut m, 1);
    // the store did not happen, the flag set, the monitor at word 0 is in
    assert_eq!(m.mem(0o4500), Some(0x1234));
    assert!(!m.mem_written(0o4500));
    assert!(events.borrow().contains(&Event::Flags(flag::OPERAND_RANGE)));
    assert!(m.monitor_mode());
    assert_eq!(m.p(), 0o100 * 4);
    let (p, ba, la, mode, xa, _vl, f) = package_fields(&m, 0);
    assert_eq!(f, flag::OPERAND_RANGE);
    assert_eq!((ba, la, mode, xa), (0o100, 0o112, mode::OPERAND_RANGE, 0));
    assert_eq!(m.mem(4).unwrap() >> 29, 0o100, "DBA");
    assert_eq!(m.mem(5).unwrap() >> 29, 0o112, "DLA");
    // model choice: the interrupt is taken right after the instruction
    assert_eq!(p, CODE * 4 + 6);
    assert_eq!(m.a(3), Some(0), "A3 5 did not execute");
}

#[test]
fn range_errors_set_no_flag_in_monitor_mode() {
    // Page 3-36: in monitor mode the flag remains cleared and no exchange
    // sequence is initiated.  X 3-19: memory is not altered, a load gives
    // zero.
    let mut m = Machine::new();
    m.load_words(CODE, &words(&cal("1000,0 S1; S2 1000,0; A3 5")))
        .unwrap();
    m.load_words(0o1000, &[0x1234]).unwrap();
    m.start_at(CODE * 4, 0, 0o10, mode::MONITOR | mode::OPERAND_RANGE); // limit 400
    define_registers(&mut m);
    m.set_s(1, Some(0x55));
    steps(&mut m, 3);
    assert_eq!(m.mem(0o1000), Some(0x1234));
    assert!(!m.mem_written(0o1000));
    assert_eq!(m.s(2), Some(0));
    assert_eq!(m.a(3), Some(5));
    assert_eq!(m.f(), 0);
    assert!(m.monitor_mode());
}

#[test]
fn operand_addresses_have_22_bits() {
    // X 3-14: without extended addressing only the low 22 bits of an operand
    // address count.
    let mut m = user(&cal("S1 0,A1; EX"), 0, LA_MAX);
    m.load_words(0o1000, &[0x77]).unwrap();
    m.set_a(1, Some(0o40001000));
    steps(&mut m, 1);
    assert_eq!((m.s(1), m.f()), (Some(0x77), 0));

    // -1 with no index is the last word of memory, which is the last word
    // of the I/O page: in range
    let mut m = user(&cal("S1 -1,0; EX"), 0, LA_MAX);
    steps(&mut m, 1);
    assert_eq!((m.s(1), m.f()), (Some(0), 0));

    // base + relative address is not wrapped: nothing lies above the four
    // million words, whatever the limit is
    let mut m = user(&cal("S1 0,A1"), 0o100, LA_MAX);
    m.set_a(1, Some((1 << 22) - 0o4000));
    steps(&mut m, 1);
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);
}

#[test]
fn instruction_and_data_fields_are_separate() {
    // X 3-19 to 3-21: fetches use IBA and ILA, operands DBA and DLA, all in
    // units of 32 words; the limit is absolute.
    let mut m = Machine::new();
    let (iba, dba) = (0o100u32, 0o300u32); // words 4000 and 14000
    m.load_words(
        iba * 32 + CODE,
        &words(&cal("S1 5,0; 6,0 S1; S2 77,0; S3 100,0; A1 3")),
    )
    .unwrap();
    m.load_words(dba * 32 + 5, &[0o777]).unwrap();
    m.load_words(dba * 32 + 0o77, &[0o111]).unwrap();
    m.load_words(dba * 32 + 0o100, &[0o222]).unwrap();
    m.start_at(CODE * 4, iba, iba + 0o20, mode::MONITOR);
    m.set_data_field(dba, dba + 2); // relative words 0 to 77
    define_registers(&mut m);
    steps(&mut m, 5);
    assert_eq!(m.s(1), Some(0o777));
    assert_eq!(m.mem(dba * 32 + 6), Some(0o777));
    assert_eq!(m.s(2), Some(0o111), "the last word of the field");
    // beyond the limit: "a zero value is transferred" (X 3-19); no flag in
    // monitor mode
    assert_eq!(m.s(3), Some(0));
    assert_eq!((m.a(1), m.f()), (Some(3), 0));
    // the data pair does not move instructions and the other way round
    assert_eq!(m.translate(5), Some(dba * 32 + 5));
    assert_eq!(m.translate_fetch(CODE), Some(iba * 32 + CODE));
    assert_eq!(m.translate(0o100), None);
    assert_eq!(m.translate_fetch(0o20 * 32), None);
    // only the low 22 bits of an operand address count (X 3-14, EAM clear)
    assert_eq!(m.translate(0o40000005), Some(dba * 32 + 5));
    // nothing above the four million words
    m.set_data_field(LA_MAX, LA_MAX);
    assert_eq!(m.translate(0), None);
}

#[test]
fn operand_range_error_needs_its_mode_bit() {
    // X 3-21: the flag sets "if the Interrupt-on-operand Range Error mode
    // bit is set" and the program is not in monitor mode.  A store outside
    // the field does not happen, a load gives zero.
    for (enabled, source) in [(true, "S1 100,0; A1 5"), (false, "S1 100,0; A1 5")] {
        let mut m = user_cal(source);
        m.set_data_field(0, 2);
        if !enabled {
            m.start_at(CODE * 4, 0, LA_MAX, 0);
            m.set_data_field(0, 2);
        }
        m.set_s(1, Some(9));
        steps(&mut m, 1);
        assert_eq!(m.monitor_mode(), enabled);
        if enabled {
            assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);
            // the load completed with zero before the exchange
            assert_eq!(m.mem(9), Some(0), "S1 of the stored package");
        } else {
            assert_eq!((m.s(1), m.f()), (Some(0), 0));
            steps(&mut m, 1);
            assert_eq!(m.a(1), Some(5));
        }
    }
    let mut m = user_cal("100,0 S1; DRI; 101,0 S1; ERI; 102,0 S1");
    m.set_data_field(0, 2);
    m.load_words(0o100, &[1, 2, 3]).unwrap();
    m.set_s(1, Some(9));
    steps(&mut m, 1);
    // back in the monitor: nothing was stored
    assert!(m.monitor_mode());
    assert_eq!(m.mem(0o100), Some(1));
    // 0024 (DRI) and 0023 (ERI) switch the mode bit in any mode (X 5-15)
    let mut m = user_cal("DRI; 101,0 S1; ERI; 102,0 S1");
    m.set_data_field(0, 2);
    m.load_words(0o100, &[1, 2, 3]).unwrap();
    steps(&mut m, 2);
    assert_eq!((m.monitor_mode(), m.f()), (false, 0));
    assert_eq!(m.m() & mode::OPERAND_RANGE, 0);
    steps(&mut m, 2);
    assert!(m.monitor_mode());
    assert_eq!(m.mem(0o101), Some(2));
    assert_eq!(m.mem(0o102), Some(3));
}

#[test]
fn block_transfers_between_memory_and_b() {
    // Page 4-29: (Ai) words starting at (A0); registers are circular, B00
    // follows B77; only the low 24 bits are transmitted to B registers and
    // the upper 40 bits are zeroed on a store.  A0 is not altered.
    let mut m = monitor_cal("B76,A1 ,A0; A0 2000; ,A0 B76,A1");
    m.load_words(
        0o1000,
        &[0xffff_ffff_ff00_0001, 2, 0xabcd_ef01_2345_6789, 4],
    )
    .unwrap();
    m.set_a(0, Some(0o1000));
    m.set_a(1, Some(3));
    steps(&mut m, 1);
    assert_eq!(m.b(0o76), Some(1));
    assert_eq!(m.b(0o77), Some(2));
    assert_eq!(m.b(0), Some(0x45_6789));
    assert_eq!(m.b(1), None);
    assert_eq!(m.a(0), Some(0o1000));
    steps(&mut m, 2);
    assert_eq!(m.mem(0o2000), Some(1));
    assert_eq!(m.mem(0o2001), Some(2));
    assert_eq!(m.mem(0o2002), Some(0x45_6789));
    assert!(!m.mem_written(0o2003));
}

#[test]
fn block_transfers_between_memory_and_t() {
    // Page 4-29, 036 and 037.
    let mut m = monitor_cal("T77,A1 ,A0; A0 2000; ,A0 T77,A1");
    m.load_words(0o1000, &[0xffff_ffff_ff00_0001, 0xabcd_ef01_2345_6789])
        .unwrap();
    m.set_a(0, Some(0o1000));
    m.set_a(1, Some(2));
    steps(&mut m, 3);
    assert_eq!(m.t(0o77), Some(0xffff_ffff_ff00_0001));
    assert_eq!(m.t(0), Some(0xabcd_ef01_2345_6789));
    assert_eq!(m.mem(0o2000), Some(0xffff_ffff_ff00_0001));
    assert_eq!(m.mem(0o2001), Some(0xabcd_ef01_2345_6789));
}

#[test]
fn block_transfer_length() {
    // Page 4-30: the length is the lower seven bits of (Ai).  "(Ai) = 0
    // causes a zero block transfer"; above 177 the bits 2**7 to 2**23 are
    // truncated; "(A0) is used as the block length if i = 0".
    let source: Vec<u64> = (0..0o200).map(|n| 0o1000 + n).collect();
    let run = |parcel: u16, a0: u32, a1: u32| {
        let mut m = monitor(&[parcel]);
        m.load_words(0, &source).unwrap();
        m.load_words(0o1000, &source).unwrap();
        m.set_a(0, Some(a0));
        m.set_a(1, Some(a1));
        steps(&mut m, 1);
        m
    };
    let m = run(0o036100, 0o1000, 0);
    assert_eq!(m.t(0), None);
    let m = run(0o036100, 0o1000, 0o200);
    assert_eq!(m.t(0), None);
    let m = run(0o036100, 0o1000, 0o1202);
    assert_eq!((m.t(0), m.t(1), m.t(2)), (Some(0o1000), Some(0o1001), None));
    // i = 0: A0 = 3 is the address and the length
    let m = run(0o034010, 3, 99);
    assert_eq!(
        (m.b(0o10), m.b(0o11), m.b(0o12), m.b(0o13)),
        (Some(0o1003), Some(0o1004), Some(0o1005), None)
    );
}

#[test]
fn block_transfer_wrap_around() {
    // Page 4-30: "200 > (Ai) > 100 causes a wrap-around condition": more
    // than 64 registers are named, so the first ones are processed twice
    // (page 4-29: T00 is processed after T77).
    let source: Vec<u64> = (0..0o200).map(|n| 0o5000 + n).collect();
    let mut m = monitor_cal("T0,A1 ,A0; A0 2000; ,A0 T76,A1");
    m.load_words(0o1000, &source).unwrap();
    m.set_a(0, Some(0o1000));
    m.set_a(1, Some(0o102)); // 66 words
    steps(&mut m, 1);
    // T00 and T01 were loaded again by the 65th and 66th words
    assert_eq!(m.t(0), Some(0o5100));
    assert_eq!(m.t(1), Some(0o5101));
    assert_eq!(m.t(2), Some(0o5002));
    assert_eq!(m.t(0o77), Some(0o5077));
    steps(&mut m, 2);
    // the store names T76, T77, T00 ... T77: T76 and T77 are stored twice
    assert_eq!(m.mem(0o2000), Some(0o5076));
    assert_eq!(m.mem(0o2001), Some(0o5077));
    assert_eq!(m.mem(0o2002), Some(0o5100));
    assert_eq!(m.mem(0o2003), Some(0o5101));
    assert_eq!(m.mem(0o2004), Some(0o5002));
    assert_eq!(m.mem(0o2100), Some(0o5076));
    assert_eq!(m.mem(0o2101), Some(0o5077));
    assert!(!m.mem_written(0o2102));
}

#[test]
fn block_transfer_outside_the_field() {
    // X 3-19: a read beyond the limit "issues and completes, but a zero
    // value is transferred"; the transfer is not cut short.  In a user
    // program with the mode on, the operand range flag interrupts after it.
    // DBA = 100, DLA = 112: relative words 0 to 477.
    let mut m = user(&cal("B10,A1 ,A0"), 0o100, 0o112);
    m.load_words(0o4476, &[7, 8, 9]).unwrap();
    m.set_a(0, Some(0o476));
    m.set_a(1, Some(4));
    m.set_b(0o12, Some(1));
    m.set_b(0o13, Some(1));
    steps(&mut m, 1);
    assert_eq!(
        (m.b(0o10), m.b(0o11), m.b(0o12), m.b(0o13)),
        (Some(7), Some(8), Some(0), Some(0))
    );
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);

    // a store: the words inside the field are written, memory beyond it is
    // not altered
    let mut m = user(&cal(",A0 B10,A1"), 0o100, 0o112);
    m.load_words(0o4500, &[0x1234]).unwrap();
    m.set_a(0, Some(0o476));
    m.set_a(1, Some(4));
    for jk in 0o10..0o14 {
        m.set_b(jk, Some(jk as u32));
    }
    steps(&mut m, 1);
    assert_eq!(m.mem(0o4476), Some(0o10));
    assert_eq!(m.mem(0o4477), Some(0o11));
    assert_eq!(m.mem(0o4500), Some(0x1234));
    assert!(!m.mem_written(0o4500) && !m.mem_written(0o4501));
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);
}

#[test]
fn vector_load_and_store_with_increment() {
    // Page 4-70: memory addresses begin with (A0) and are incremented by
    // (Ak), a signed integer; (VL) words are transferred.  Page 4-71: the
    // increment is 1 if k = 0.
    let source: Vec<u64> = (0..0o100).map(|n| 0o7000 + n).collect();
    let mut m = monitor_cal("V1 ,A0,A2; V2 ,A0,1; A0 1017; V3 ,A0,A3; A0 2000; ,A0,A2 V1; A0 3003; ,A0,A3 V2; A0 3010; ,A0,1 V3");
    m.load_words(0o1000, &source).unwrap();
    m.set_a(0, Some(0o1000));
    m.set_a(2, Some(3));
    m.set_a(3, Some(0xff_ffff)); // -1
    m.set_vl(Some(4));
    steps(&mut m, 1);
    assert_eq!(
        [m.v(1, 0), m.v(1, 1), m.v(1, 2), m.v(1, 3)],
        [Some(0o7000), Some(0o7003), Some(0o7006), Some(0o7011)]
    );
    assert_eq!(m.v(1, 4), None);
    steps(&mut m, 1);
    assert_eq!(
        [m.v(2, 0), m.v(2, 1), m.v(2, 2), m.v(2, 3)],
        [Some(0o7000), Some(0o7001), Some(0o7002), Some(0o7003)]
    );
    steps(&mut m, 2);
    assert_eq!(
        [m.v(3, 0), m.v(3, 1), m.v(3, 2), m.v(3, 3)],
        [Some(0o7017), Some(0o7016), Some(0o7015), Some(0o7014)]
    );
    steps(&mut m, 6);
    assert_eq!(m.mem(0o2000), Some(0o7000));
    assert_eq!(m.mem(0o2003), Some(0o7003));
    assert_eq!(m.mem(0o2006), Some(0o7006));
    assert_eq!(m.mem(0o2011), Some(0o7011));
    assert!(!m.mem_written(0o2001) && !m.mem_written(0o2014));
    // a backward stream
    assert_eq!(
        [m.mem(0o3000), m.mem(0o3001), m.mem(0o3002), m.mem(0o3003)],
        [Some(0o7003), Some(0o7002), Some(0o7001), Some(0o7000)]
    );
    assert_eq!([m.mem(0o3010), m.mem(0o3013)], [Some(0o7017), Some(0o7014)]);
    assert_eq!(m.a(0), Some(0o3010), "A0 is not altered");
}

#[test]
fn vector_transfer_of_64_words_and_events_in_element_order() {
    // Page 4-10: VL = 0 gives 64 operations.
    let source: Vec<u64> = (0..0o100).collect();
    let mut m = monitor_cal("V5 ,A0,1; A0 2000; ,A0,1 V5");
    m.load_words(0o1000, &source).unwrap();
    m.set_a(0, Some(0o1000));
    m.set_vl(Some(0));
    let events = record(&mut m);
    steps(&mut m, 3);
    for e in 0..64 {
        assert_eq!(m.v(5, e), Some(e as u64));
        assert_eq!(m.mem(0o2000 + e as u32), Some(e as u64));
    }
    let events = events.borrow();
    let elements: Vec<u8> = events
        .iter()
        .filter_map(|e| match e {
            Event::V { i: 5, elem, .. } => Some(*elem),
            _ => None,
        })
        .collect();
    assert_eq!(elements, (0..64).collect::<Vec<u8>>());
    let stores: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            Event::Mem { addr, .. } => Some(*addr),
            _ => None,
        })
        .collect();
    assert_eq!(stores, (0o2000..0o2100).collect::<Vec<u32>>());
}

#[test]
fn vector_transfer_outside_the_field() {
    // As the block transfers.  DBA = 100, DLA = 112: relative words 0 to
    // 477.
    let mut m = user(&cal("V1 ,A0,1"), 0o100, 0o112);
    m.load_words(0o4476, &[7, 8, 9]).unwrap();
    m.set_a(0, Some(0o476));
    m.set_vl(Some(4));
    for e in 0..4 {
        m.set_v(1, e, Some(1));
    }
    steps(&mut m, 1);
    assert_eq!(
        [m.v(1, 0), m.v(1, 1), m.v(1, 2), m.v(1, 3)],
        [Some(7), Some(8), Some(0), Some(0)]
    );
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);

    // In monitor mode nothing interrupts the instruction: the references
    // inside the field are made, the others are not.
    let mut m = Machine::new();
    m.load_words(CODE, &words(&cal(",A0,A2 V1"))).unwrap();
    m.load_words(0o400, &[0x1234]).unwrap();
    m.start_at(CODE * 4, 0, 0o10, mode::MONITOR); // limit 400
    define_registers(&mut m);
    m.set_a(0, Some(0o400));
    m.set_a(2, Some(0xff_ffff)); // 400 is outside, then 377 and 376 inside
    m.set_vl(Some(3));
    for e in 0..3 {
        m.set_v(1, e, Some(e as u64 + 1));
    }
    steps(&mut m, 1);
    assert_eq!(m.mem(0o400), Some(0x1234));
    assert_eq!((m.mem(0o377), m.mem(0o376)), (Some(2), Some(3)));
    assert_eq!(m.f(), 0);
}

#[test]
fn block_and_vector_transfers_outside_the_field_go_on() {
    // X 3-19: a read beyond the limit "issues and completes, but a zero
    // value is transferred"; a write "is allowed to issue, but no write
    // occurs".  The transfer is not cut short as it may be on a CRAY-1.
    let mut m = monitor_cal("V1 ,A0,A2; ,A0,A2 V2; B10,A3 ,A0");
    m.set_data_field(0, 0o100); // relative words 0 to 3777
    for w in 0o3776..0o4002 {
        m.store(w, Some(w as u64));
    }
    m.set_a(0, Some(0o3776));
    m.set_a(2, Some(1));
    m.set_a(3, Some(4));
    m.set_vl(Some(4));
    for e in 0..4 {
        m.set_v(2, e, Some(0o70 + e as u64));
    }
    steps(&mut m, 3);
    assert_eq!(
        (0..4).map(|e| m.v(1, e)).collect::<Vec<_>>(),
        some(&[0o3776, 0o3777, 0, 0])
    );
    assert_eq!(
        (0o3776..0o4002).map(|w| m.mem(w)).collect::<Vec<_>>(),
        some(&[0o70, 0o71, 0o4000, 0o4001])
    );
    assert_eq!(
        (0o10..0o14).map(|jk| m.b(jk)).collect::<Vec<_>>(),
        vec![Some(0o70), Some(0o71), Some(0), Some(0)]
    );
    assert_eq!(m.f(), 0);
}

fn some(values: &[u64]) -> Vec<Option<u64>> {
    values.iter().map(|v| Some(*v)).collect()
}

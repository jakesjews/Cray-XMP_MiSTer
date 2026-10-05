//! Memory references: 10h to 13h, the block transfers 034 to 037, the
//! vector transfers 176 and 177, base and limit.

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
    // Page 3-44: absolute addresses are formed by adding (BA) * 16 to the
    // relative address.
    let mut m = user(&cal("210,0 S1; S2 210,0; EX"), 0o100, 0o200);
    m.set_s(1, Some(0x77));
    steps(&mut m, 2);
    assert_eq!(m.mem(0o100 * 16 + 0o210), Some(0x77));
    assert_eq!(m.mem(0o210), None);
    assert_eq!(m.s(2), Some(0x77));
}

#[test]
fn operand_range_error_in_a_user_program() {
    // Page 3-44: the final address a program can reference is (LA) * 16 - 1,
    // an absolute address.  Page 3-43: a reference beyond the limit does not
    // alter memory and, for a program not in monitor mode, sets the operand
    // range flag (bit 34 of the flags, figure 3-8), which interrupts it.
    // BA = 100 (base 2000), LA = 112 (limit 2240): relative 0 to 237.
    let mut m = user(&cal("237,0 S1; S2 237,0; 240,0 S1; A3 5"), 0o100, 0o112);
    m.set_s(1, Some(0x55));
    m.load_words(0o2240, &[0x1234]).unwrap();
    steps(&mut m, 2);
    assert_eq!(m.mem(0o2237), Some(0x55));
    assert_eq!(m.s(2), Some(0x55));
    assert_eq!(m.f(), 0);
    let events = record(&mut m);
    steps(&mut m, 1);
    // the store did not happen, the flag set, the monitor at word 0 is in
    assert_eq!(m.mem(0o2240), Some(0x1234));
    assert!(!m.mem_written(0o2240));
    assert!(events.borrow().contains(&Event::Flags(flag::OPERAND_RANGE)));
    assert!(m.monitor_mode());
    assert_eq!(m.p(), 0o100 * 4);
    let (p, ba, la, mode, xa, _vl, f) = package_fields(&m, 0);
    assert_eq!(f, flag::OPERAND_RANGE);
    assert_eq!((ba, la, mode, xa), (0o100, 0o112, 0, 0));
    // model choice: the interrupt is taken right after the instruction
    assert_eq!(p, CODE * 4 + 6);
    assert_eq!(m.a(3), Some(0), "A3 5 did not execute");
}

#[test]
fn operand_range_on_a_load_leaves_the_register_undefined() {
    // The manual does not say what the register receives.
    let mut m = user(&cal("S1 240,0"), 0o100, 0o112);
    m.set_s(1, Some(0x55));
    steps(&mut m, 1);
    let (_, _, _, _, _, _, f) = package_fields(&m, 0);
    assert_eq!(f, flag::OPERAND_RANGE);
    assert_eq!(m.mem(0o11), None, "the saved S1 is undefined");
    assert_eq!(m.mem(0o12), Some(0), "the saved S2 is not");
}

#[test]
fn range_errors_set_no_flag_in_monitor_mode() {
    // Page 3-36: in monitor mode the flag remains cleared and no exchange
    // sequence is initiated.  Page 3-43: memory is not altered.
    let mut m = Machine::new();
    m.load_words(CODE, &words(&cal("1000,0 S1; S2 1000,0; A3 5")))
        .unwrap();
    m.load_words(0o1000, &[0x1234]).unwrap();
    m.start_at(CODE * 4, 0, 0o20, mode::MONITOR); // limit 400
    define_registers(&mut m);
    m.set_s(1, Some(0x55));
    steps(&mut m, 3);
    assert_eq!(m.mem(0o1000), Some(0x1234));
    assert!(!m.mem_written(0o1000));
    assert_eq!(m.s(2), None);
    assert_eq!(m.a(3), Some(5));
    assert_eq!(m.f(), 0);
    assert!(m.monitor_mode());
}

#[test]
fn addresses_above_the_memory_size_are_out_of_range() {
    // Page 4-3: the upper two bits of the 22-bit address field are unused;
    // "an operand range error occurs if either bit is set".  The same holds
    // for any address of 2**20 or more, whatever LA is.
    let mut m = user(&cal("S1 4000000,0"), 0, LA_MAX);
    steps(&mut m, 1);
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);

    // -1 with no index: all 22 address bits set
    let mut m = user(&cal("S1 -1,0"), 0, LA_MAX);
    steps(&mut m, 1);
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);

    // the last word of memory is the last word of the I/O page: in range
    let mut m = user(&cal("S1 3777777,0; EX"), 0, LA_MAX);
    steps(&mut m, 1);
    assert_eq!(m.s(1), Some(0));
    assert_eq!(m.f(), 0);

    // base + relative address is not wrapped
    let mut m = user(&cal("S1 0,A1"), 0o100, LA_MAX);
    m.set_a(1, Some((1 << 20) - 0o2000));
    steps(&mut m, 1);
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);
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
    // Page 4-30: "an out-of-range memory reference will cause an interrupt
    // condition to occur".  Model choice: in a user program everything
    // from the first such reference on is undefined.
    // BA = 100, LA = 112: relative words 0 to 237.
    let mut m = user(&cal("B10,A1 ,A0"), 0o100, 0o112);
    m.load_words(0o2236, &[7, 8]).unwrap();
    m.set_a(0, Some(0o236));
    m.set_a(1, Some(4));
    m.set_b(0o12, Some(1));
    m.set_b(0o13, Some(1));
    steps(&mut m, 1);
    assert_eq!(
        (m.b(0o10), m.b(0o11), m.b(0o12), m.b(0o13)),
        (Some(7), Some(8), None, None)
    );
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);

    // a store: the words inside the field are written, memory beyond it is
    // not altered (page 3-43)
    let mut m = user(&cal(",A0 B10,A1"), 0o100, 0o112);
    m.load_words(0o2240, &[0x1234]).unwrap();
    m.set_a(0, Some(0o236));
    m.set_a(1, Some(4));
    for jk in 0o10..0o14 {
        m.set_b(jk, Some(jk as u32));
    }
    steps(&mut m, 1);
    assert_eq!(m.mem(0o2236), Some(0o10));
    assert_eq!(m.mem(0o2237), Some(0o11));
    assert_eq!(m.mem(0o2240), Some(0x1234));
    assert!(!m.mem_written(0o2240) && !m.mem_written(0o2241));
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
    // Page 4-71: "memory reference out of limits".  Model choice as for the
    // block transfers.  BA = 100, LA = 112: relative words 0 to 237.
    let mut m = user(&cal("V1 ,A0,1"), 0o100, 0o112);
    m.load_words(0o2236, &[7, 8]).unwrap();
    m.set_a(0, Some(0o236));
    m.set_vl(Some(4));
    for e in 0..4 {
        m.set_v(1, e, Some(1));
    }
    steps(&mut m, 1);
    assert_eq!(
        [m.v(1, 0), m.v(1, 1), m.v(1, 2), m.v(1, 3)],
        [Some(7), Some(8), None, None]
    );
    assert_eq!(package_fields(&m, 0).6, flag::OPERAND_RANGE);

    // In monitor mode nothing interrupts the instruction: the references
    // inside the field are made, the others are not.
    let mut m = Machine::new();
    m.load_words(CODE, &words(&cal(",A0,A2 V1"))).unwrap();
    m.load_words(0o400, &[0x1234]).unwrap();
    m.start_at(CODE * 4, 0, 0o20, mode::MONITOR); // limit 400
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

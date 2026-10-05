//! The X-MP mode (`Cpu::Xmp`): what a one-processor CRAY X-MP has that the
//! operating system COS needs.  Page numbers are those of the CRAY X-MP
//! Series Model 14 mainframe reference manual CSM-0111000 ("X").

use super::*;
use cray1_isa::Cpu;

/// The largest value of a 19-bit base or limit field.
const FIELD_MAX: u32 = 0x7ffff;

fn xcal(source: &str) -> Vec<u16> {
    let mut out = Vec::new();
    for line in source.split(';').map(str::trim).filter(|l| !l.is_empty()) {
        let mut fields = line.split_whitespace();
        let result = fields.next().unwrap();
        let operand = fields.next().unwrap_or("");
        let a = cray1_isa::assemble_numeric_cpu(Cpu::Xmp, result, operand)
            .unwrap_or_else(|e| panic!("`{}`: {}", line, e));
        out.extend(a.encoding.parcels());
    }
    out
}

/// An X-MP past the dead start in monitor mode, both fields from 0 to the
/// end of memory, cluster 0, `source` at word 200 and P there.
fn xmonitor(source: &str) -> Machine {
    let mut m = Machine::for_cpu(Cpu::Xmp);
    m.load_words(CODE, &words(&xcal(source))).unwrap();
    m.start_at(CODE * 4, 0, FIELD_MAX, mode::MONITOR);
    define_registers(&mut m);
    m
}

/// The same as a user program with the operand range interrupt enabled.
/// An exchange lands in monitor mode at word 100 (`J 400`) with the
/// package at 0.
fn xuser(source: &str) -> Machine {
    let mut m = Machine::for_cpu(Cpu::Xmp);
    let mut package = [0u64; 16];
    package[0] = (0o100u64 * 4) << 24;
    package[2] = (FIELD_MAX as u64) << 29 | (mode::MONITOR as u64) << 24;
    package[5] = (FIELD_MAX as u64) << 29;
    m.load_words(0, &package).unwrap();
    m.load_words(0o100, &words(&cal("J 400"))).unwrap();
    m.load_words(CODE, &words(&xcal(source))).unwrap();
    m.start_at(CODE * 4, 0, FIELD_MAX, mode::OPERAND_RANGE);
    define_registers(&mut m);
    m
}

#[test]
fn memory_and_the_io_page() {
    // Four million words (X 2-9); the core's I/O page is at their top.
    let m = Machine::for_cpu(Cpu::Xmp);
    assert_eq!((m.memory_words(), m.io_page()), (1 << 22, (1 << 22) - 16));
    assert_eq!(Machine::new().io_page(), IO_PAGE);
    // word 3777761 is ordinary memory here; the console is at 17777761
    let mut m = xmonitor("3777761,0 S1; 17777761,0 S1; S2 3777761,0; S3 17777760,0; 17777762,0 S4");
    m.set_s(1, Some(b'x' as u64));
    m.set_s(4, Some(7));
    steps(&mut m, 4);
    assert_eq!(m.console(), b"x");
    assert_eq!((m.s(2), m.s(3)), (Some(b'x' as u64), Some(2)));
    assert_eq!(m.mem(0o3777761), Some(b'x' as u64));
    assert_eq!(m.step(), StepResult::Exit(7));
}

#[test]
fn exchange_package_layout() {
    // X figure 3-3 and table 3-1.  The package at 0 is loaded by EX and
    // stored again at (XA) * 16 by the EX it runs.  Every field holds a
    // pattern, and every bit the machine does not keep is set on the way in.
    let (p, iba, ila, dba, dla) = (0o1000u64 * 4 + 2, 0o1u64, 0x7_5a5a, 0x1_2345, 0x6_abcd);
    let modes = (mode::OPERAND_RANGE
        | mode::CORRECTABLE_MEMORY
        | mode::FLOATING_POINT
        | mode::UNCORRECTABLE_MEMORY
        | mode::MONITOR) as u64;
    let m1 = (mode1::FP_STATUS | mode1::BIDIRECTIONAL | mode1::INTERRUPT_MONITOR) as u64;
    let (xa, vl, cln) = (0o40u64, 0o125u64, 2u64);
    let a = |i: u64| 0o1234567 + i;
    let kept = [
        p << 24 | a(0),
        iba << 29 | m1 << 24 | a(1),
        ila << 29 | modes << 24 | a(2),
        xa << 40 | vl << 33 | a(3),
        dba << 29 | 1 << 28 | cln << 24 | a(4),
        dla << 29 | a(5),
        a(6),
        a(7),
    ];
    // PN, E and S; R, CS, B, WS and the unused bit 38; VNU; ESVL; EAM
    let dropped = [
        0x7ff << 52,
        0xfff << 52 | 0o22 << 24,
        1 << 63,
        1 << 63,
        1 << 63,
        0,
        0,
        0,
    ];
    let mut m = xmonitor("EX");
    let mut package = [0u64; 16];
    for n in 0..8 {
        package[n] = kept[n] | dropped[n];
        package[8 + n] = 0x0123_4567_89ab_cdef ^ (n as u64) << 60;
    }
    m.load_words(0, &package).unwrap();
    // the program of the package: IBA is 1, so relative word 1000 is 1040
    m.load_words(0o1040, &words(&[0, 0, 0o004000])).unwrap();
    // and the package its exit exchanges with
    m.load_words(xa as u32 * 16, &[0; 16]).unwrap();
    steps(&mut m, 1);
    assert_eq!(m.p(), p as u32);
    assert_eq!((m.ba(), m.la()), (iba as u32, ila as u32));
    assert_eq!(m.data_field(), (dba as u32, dla as u32));
    assert_eq!(
        (m.m() as u64, m.m1(), m.cluster()),
        (modes, Some(m1 as u8), 2)
    );
    assert_eq!((m.xa(), m.vl(), m.f()), (xa as u8, Some(vl as u8), 0));
    steps(&mut m, 1);
    let stored: Vec<u64> = (0..16)
        .map(|n| m.mem(xa as u32 * 16 + n).unwrap())
        .collect();
    // P is the parcel after the exit; an exit in monitor mode sets no flag
    assert_eq!(stored[0], (p + 1) << 24 | a(0));
    assert_eq!(&stored[1..8], &kept[1..8]);
    assert_eq!(&stored[8..], &package[8..]);
    // the outgoing package of the first EX went to word 0 in the same layout
    let first = m.mem(0).unwrap();
    assert_eq!(first >> 24, (CODE as u64 * 4 + 1) & 0xff_ffff);
    assert_eq!(m.mem(2).unwrap() >> 29, FIELD_MAX as u64);
    assert_eq!(m.mem(5).unwrap() >> 29, FIELD_MAX as u64);
}

#[test]
fn instruction_and_data_fields_are_separate() {
    // X 3-19 to 3-21: fetches use IBA and ILA, operands DBA and DLA, all in
    // units of 32 words; the limit is absolute.
    let mut m = Machine::for_cpu(Cpu::Xmp);
    let (iba, dba) = (0o100u32, 0o300u32); // words 4000 and 14000
    m.load_words(
        iba * 32 + CODE,
        &words(&xcal("S1 5,0; 6,0 S1; S2 77,0; S3 100,0; A1 3")),
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
    m.set_data_field(FIELD_MAX, FIELD_MAX);
    assert_eq!(m.translate(0), None);
}

#[test]
fn operand_range_error_needs_its_mode_bit() {
    // X 3-21: the flag sets "if the Interrupt-on-operand Range Error mode
    // bit is set" and the program is not in monitor mode.  A store outside
    // the field does not happen, a load gives zero.
    for (enabled, source) in [(true, "S1 100,0; A1 5"), (false, "S1 100,0; A1 5")] {
        let mut m = xuser(source);
        m.set_data_field(0, 2);
        if !enabled {
            m.start_at(CODE * 4, 0, FIELD_MAX, 0);
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
    let mut m = xuser("100,0 S1; DRI; 101,0 S1; ERI; 102,0 S1");
    m.set_data_field(0, 2);
    m.load_words(0o100, &[1, 2, 3]).unwrap();
    m.set_s(1, Some(9));
    steps(&mut m, 1);
    // back in the monitor: nothing was stored
    assert!(m.monitor_mode());
    assert_eq!(m.mem(0o100), Some(1));
    // 0024 (DRI) and 0023 (ERI) switch the mode bit in any mode (X 5-15)
    let mut m = xuser("DRI; 101,0 S1; ERI; 102,0 S1");
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
fn block_and_vector_transfers_outside_the_field_go_on() {
    // X 3-19: a read beyond the limit "issues and completes, but a zero
    // value is transferred"; a write "is allowed to issue, but no write
    // occurs".  The transfer is not cut short as it may be on a CRAY-1.
    let mut m = xmonitor("V1 ,A0,A2; ,A0,A2 V2; B10,A3 ,A0");
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

#[test]
fn p_has_24_bits() {
    // X 3-6 and 5-21: P is 24 bits and a branch takes the low 24 bits of
    // ijkm; there is no range error from the address itself.
    let far = 0o5000000u32; // a word above the first million: P needs 23 bits
    let mut m = xmonitor("J 24000000; A1 1");
    m.load_words(far, &words(&xcal("A1 2; R 1000"))).unwrap();
    steps(&mut m, 3);
    assert_eq!((m.a(1), m.f()), (Some(2), 0));
    assert_eq!(m.b(0), Some(far * 4 + 3), "B00 holds all 24 bits");
    assert_eq!(m.p(), 0o1000);
    // a fetch beyond the limit is the range error, in a user program
    let mut m = xuser("J 24000000");
    m.start_at(CODE * 4, 0, 0o100, 0);
    steps(&mut m, 2);
    assert!(m.monitor_mode());
    assert_eq!(package_fields(&m, 0).6, flag::PROGRAM_RANGE);
    assert_eq!(m.mem(0).unwrap() >> 24 & 0xff_ffff, 0o24000000);
    // 010 to 017 with the high bit of i are `Ah exp` on an X-MP: not here
    let mut m = xmonitor("PASS");
    m.load_words(CODE, &words(&[0o010400, 0])).unwrap();
    assert_eq!(error_of(&mut m).kind, ErrorKind::NotDefinedByManual);
}

#[test]
fn cluster_number_and_shared_registers() {
    // X 2-17, 5-11, 5-32, 5-34, 5-59.  0014j3 enters the cluster number in
    // monitor mode only.  Clusters 1 to 3 each have eight SB, eight ST and 32
    // semaphores; in cluster 0 the stores do nothing and the loads give 0.
    let source = "SB3 A1; ST5 S1; SM S1; A2 SB3; S2 ST5; S3 SM; \
                  CLN 1; SB3 A1; ST5 S1; SM S1; A2 SB3; S2 ST5; S3 SM; \
                  CLN 2; A3 SB3; S4 ST5; SB3 A4; \
                  CLN 1; A5 SB3; CLN 0; A6 SB3; S5 SM";
    let mut m = xmonitor(source);
    m.set_a(1, Some(0o1234567));
    m.set_a(4, Some(0o7654321));
    m.set_s(1, Some(0xa5a5_0001_ffff_ffff));
    for i in [2, 3, 5, 6] {
        m.set_a(i, Some(9));
    }
    steps(&mut m, 6);
    assert_eq!((m.a(2), m.s(2), m.s(3)), (Some(0), Some(0), Some(0)));
    steps(&mut m, 7);
    assert_eq!(m.cluster(), 1);
    assert_eq!(
        (m.a(2), m.s(2)),
        (Some(0o1234567), Some(0xa5a5_0001_ffff_ffff))
    );
    // the semaphores are the high 32 bits, SM0 the sign bit; the low half is 0
    assert_eq!(m.s(3), Some(0xa5a5_0001_0000_0000));
    assert_eq!(
        (m.sm(1, 0), m.sm(1, 1), m.sm(1, 31)),
        (Some(true), Some(false), Some(true))
    );
    // cluster 2 has its own registers, never written: undefined in the model
    steps(&mut m, 4);
    assert_eq!((m.a(3), m.s(4)), (None, None));
    steps(&mut m, 2);
    assert_eq!(m.a(5), Some(0o1234567));
    assert_eq!(m.sb(2, 3), Some(0o7654321));
    steps(&mut m, 3);
    assert_eq!((m.cluster(), m.a(6), m.s(5)), (0, Some(0), Some(0)));
    // outside monitor mode 0014j3 is a pass; the shared registers are not
    // privileged
    let mut m = xuser("CLN 3; SB1 A1; A2 SB1");
    m.set_cluster(2);
    m.set_a(1, Some(5));
    steps(&mut m, 3);
    assert_eq!((m.cluster(), m.a(2), m.sb(2, 1)), (2, Some(5), Some(5)));
}

#[test]
fn semaphores_and_test_and_set() {
    // X 2-17 and 5-17.  0036 clears and 0037 sets one semaphore.  0034 sets
    // a clear one and goes on.  On a set one the instruction cannot issue:
    // outside monitor mode the deadlock flag sets, and the package stored
    // has P at the 0034 and the waiting-for-semaphore bit.
    let mut m = xuser("SM5 0; SM5 1,TS; SM7 1; SM7 0; SM37 1; A1 1; SM5 1,TS; A1 2");
    m.set_cluster(1);
    steps(&mut m, 6);
    assert_eq!(
        (m.sm(1, 5), m.sm(1, 7), m.sm(1, 31)),
        (Some(true), Some(false), Some(true))
    );
    assert_eq!((m.a(1), m.f(), m.monitor_mode()), (Some(1), 0, false));
    steps(&mut m, 1);
    assert!(m.monitor_mode());
    let w = |n: u32| m.mem(n).unwrap();
    assert_eq!(w(3) >> 48 & 1, 1, "deadlock flag, bit 15 of word 3");
    assert_eq!(w(3) >> 24 & 0o777, 0, "no other flag");
    assert_eq!(w(1) >> 28 & 1, 1, "waiting for semaphore");
    assert_eq!(
        w(0) >> 24 & 0xff_ffff,
        (CODE * 4 + 6) as u64,
        "P at the test and set"
    );
    assert_eq!(w(1) & 0xff_ffff, 1, "the instruction after it did not run");
    // cluster 0: all three are no-ops, even on a set semaphore
    let mut m = xuser("SM5 1,TS; SM5 1,TS; A1 2");
    steps(&mut m, 3);
    assert_eq!((m.a(1), m.f()), (Some(2), 0));
    // in monitor mode a set semaphore would wait for good: the model stops
    let mut m = xmonitor("CLN 2; SM5 1; SM5 1,TS");
    steps(&mut m, 2);
    assert_eq!(error_of(&mut m).kind, ErrorKind::NotDefinedByManual);
    // a semaphore never written is undefined: testing it is a test error
    let mut m = xmonitor("CLN 2; SM5 1,TS");
    steps(&mut m, 1);
    assert_eq!(error_of(&mut m).kind, ErrorKind::UndefinedValue);
}

#[test]
fn status_register() {
    // X 5-60: 073i01.  Bit 0 clustered, 6 program state, 12 floating point
    // error status, 13 floating point interrupt mode, 14 operand range mode,
    // 15 bidirectional memory, 30 and 31 the cluster number (0 outside
    // monitor mode); the low 32 bits are ones.
    let mut m = xmonitor("S1 SR0; CLN 3; EFI; ERI; EBM; S2 SR0; DFI; DRI; DBM; S3 SR0");
    steps(&mut m, 1);
    assert_eq!(m.s(1), Some(0xffff_ffff));
    steps(&mut m, 5);
    assert_eq!(
        m.s(2),
        Some(1 << 63 | 1 << 50 | 1 << 49 | 1 << 48 | 3 << 32 | 0xffff_ffff)
    );
    steps(&mut m, 4);
    assert_eq!(m.s(3), Some(1 << 63 | 3 << 32 | 0xffff_ffff));
    // a user program sees that it is clustered but not which cluster
    let mut m = xuser("S1 SR0");
    m.set_cluster(2);
    steps(&mut m, 1);
    assert_eq!(m.s(1), Some(1 << 63 | 1 << 49 | 0xffff_ffff));
    // the floating point error status sets whatever the interrupt mode is and
    // is cleared by 0021 and 0022 (X 5-15)
    let big = cray1_fp::pack(false, 0o57777, 0x8000_0000_0000);
    let mut m = xmonitor("S3 S1*FS2; S4 SR0; DFI; S5 SR0");
    m.set_s(1, Some(big));
    m.set_s(2, Some(big));
    steps(&mut m, 4);
    assert_eq!(m.s(4).unwrap() >> 51 & 1, 1);
    assert_eq!(m.s(5).unwrap() >> 51 & 1, 0);
    assert_eq!(m.f(), 0);
}

#[test]
fn a_vector_register_as_operand_and_result_is_not_recursive() {
    // X 3-33: "A V register can be used, however, as both an operand and
    // result in the same vector operation."  Nothing is said of the
    // CRAY-1's groups (HRM 3-14): each operation takes the element as it was
    // before the instruction.
    let mut m = xmonitor("V1 V1+V2; V3 V3,V3>A1");
    m.set_vl(Some(20));
    m.set_a(1, Some(4));
    for e in 0..20 {
        m.set_v(1, e, Some(100 + e as u64));
        m.set_v(2, e, Some(1000));
        m.set_v(3, e, Some(e as u64 + 1));
    }
    steps(&mut m, 1);
    for e in 0..20 {
        assert_eq!(m.v(1, e), Some(1100 + e as u64), "element {}", e);
    }
    // the double shift takes its neighbour as it was before, too
    steps(&mut m, 1);
    assert_eq!(m.v(3, 0), Some(0));
    for e in 1..20u64 {
        assert_eq!(
            m.v(3, e as usize),
            Some(e << 60 | (e + 1) >> 4),
            "element {}",
            e
        );
    }
}

#[test]
fn the_1982_options_and_the_rest_are_as_on_the_cray_1() {
    // the programmable clock, the population instructions, RT and VM
    let mut m = xmonitor("A1 QS1; V1 PV2; S2 VM; VM S1; S3 VM; PCI S1; ECI; DCI; CCI; CMR");
    m.set_s(1, Some(7));
    m.set_vl(Some(1));
    m.set_vm(Some(5));
    m.set_v(2, 0, Some(0xff));
    steps(&mut m, 10);
    assert_eq!(
        (m.a(1), m.v(1, 0), m.s(2), m.s(3)),
        (Some(1), Some(8), Some(5), Some(7))
    );
    // 003 with i other than 4, 6 and 7 is still `VM Sj`
    let mut m = xmonitor("PASS");
    m.load_words(CODE, &words(&[0o003510])).unwrap();
    m.set_s(1, Some(0o77));
    steps(&mut m, 1);
    assert_eq!(m.vm(), Some(0o77));
}

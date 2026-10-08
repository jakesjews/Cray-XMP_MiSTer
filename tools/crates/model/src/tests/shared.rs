//! The cluster number, the shared registers SB and ST, the semaphores and
//! the status register.  Page numbers are those of the CRAY X-MP Series
//! Model 14 mainframe reference manual CSM-0111000 ("X").

use super::*;

#[test]
fn cluster_number_and_shared_registers() {
    // X 2-17, 5-11, 5-32, 5-34, 5-59.  0014j3 enters the cluster number in
    // monitor mode only.  Clusters 1 to 3 each have eight SB, eight ST and 32
    // semaphores; in cluster 0 the stores do nothing and the loads give 0.
    let source = "SB3 A1; ST5 S1; SM S1; A2 SB3; S2 ST5; S3 SM; \
                  CLN 1; SB3 A1; ST5 S1; SM S1; A2 SB3; S2 ST5; S3 SM; \
                  CLN 2; A3 SB3; S4 ST5; SB3 A4; \
                  CLN 1; A5 SB3; CLN 0; A6 SB3; S5 SM";
    let mut m = monitor_cal(source);
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
    let mut m = user_cal("CLN 3; SB1 A1; A2 SB1");
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
    let mut m = user_cal("SM5 0; SM5 1,TS; SM7 1; SM7 0; SM37 1; A1 1; SM5 1,TS; A1 2");
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
    let mut m = user_cal("SM5 1,TS; SM5 1,TS; A1 2");
    steps(&mut m, 3);
    assert_eq!((m.a(1), m.f()), (Some(2), 0));
    // in monitor mode a set semaphore would wait for good: the model stops
    let mut m = monitor_cal("CLN 2; SM5 1; SM5 1,TS");
    steps(&mut m, 2);
    assert_eq!(error_of(&mut m).kind, ErrorKind::NotDefinedByManual);
    // a semaphore never written is undefined: testing it is a test error
    let mut m = monitor_cal("CLN 2; SM5 1,TS");
    steps(&mut m, 1);
    assert_eq!(error_of(&mut m).kind, ErrorKind::UndefinedValue);
}

#[test]
fn status_register() {
    // X 5-60: 073i01.  Bit 0 clustered, 6 program state, 12 floating point
    // error status, 13 floating point interrupt mode, 14 operand range mode,
    // 15 bidirectional memory, 30 and 31 the cluster number (0 outside
    // monitor mode); the low 32 bits are ones.
    let mut m = monitor_cal("S1 SR0; CLN 3; EFI; ERI; EBM; S2 SR0; DFI; DRI; DBM; S3 SR0");
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
    let mut m = user_cal("S1 SR0");
    m.set_cluster(2);
    steps(&mut m, 1);
    assert_eq!(m.s(1), Some(1 << 63 | 1 << 49 | 0xffff_ffff));
    // the floating point error status sets whatever the interrupt mode is and
    // is cleared by 0021 and 0022 (X 5-15)
    let big = cray_xmp_fp::pack(false, 0o57777, 0x8000_0000_0000);
    let mut m = monitor_cal("S3 S1*FS2; S4 SR0; DFI; S5 SR0");
    m.set_s(1, Some(big));
    m.set_s(2, Some(big));
    steps(&mut m, 4);
    assert_eq!(m.s(4).unwrap() >> 51 & 1, 1);
    assert_eq!(m.s(5).unwrap() >> 51 & 1, 0);
    assert_eq!(m.f(), 0);
}

#[test]
fn the_vector_mask_shares_003_with_the_semaphores() {
    // 0034, 0036 and 0037 are the semaphore instructions; 003 with any other
    // i is `VM Sj`.
    let mut m = monitor(&[0o003510]);
    m.set_s(1, Some(0o77));
    steps(&mut m, 1);
    assert_eq!(m.vm(), Some(0o77));
}

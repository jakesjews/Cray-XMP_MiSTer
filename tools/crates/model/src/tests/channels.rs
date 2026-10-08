//! The channels: 0010 to 0012 and 033.  Page numbers are those of the CRAY
//! X-MP Series Model 14 mainframe reference manual CSM-0111000 ("X").

use super::*;

#[test]
fn channel_pair_loops_data_through_memory() {
    // X 2-15 to 2-19 and appendix B.  Channel 11 sends three words from
    // 1000, and with its cable looped back channel 10 receives them at 2000.
    // CL is entered first; entering CA activates the channel.
    let source = "A1 10; A2 2000; A3 2003; CL,A1 A3; CA,A1 A2; \
                  A4 11; A5 1000; A6 1003; CL,A4 A6; CA,A4 A5; \
                  PASS; A7 CI; CI,A1; A7 CI; S1 2001,0; A2 CA,A1; A3 CA,A4; A5 CE,A4; CI,A4; A7 CI";
    let mut m = monitor_cal(source);
    m.set_channel_loopback(true);
    m.load_words(
        0o1000,
        &[
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
            0x9999_aaaa_bbbb_cccc,
        ],
    )
    .unwrap();
    steps(&mut m, 10);
    assert!(m.channel(0o10).active && m.channel(0o11).active);
    assert_eq!(m.channel(0o10).ca, 0o2000);
    steps(&mut m, 2);
    assert_eq!(m.a(7), Some(0o10), "the lowest numbered channel first");
    for n in 0..3 {
        assert_eq!(m.mem(0o2000 + n), m.mem(0o1000 + n), "word {}", n);
    }
    let done = m.channel(0o11);
    assert!(done.interrupt && !done.active && !done.error);
    steps(&mut m, 2);
    assert_eq!(m.a(7), Some(0o11));
    steps(&mut m, 4);
    assert_eq!(m.s(1), Some(0x5555_6666_7777_8888));
    assert_eq!(
        (m.a(2), m.a(3), m.a(5)),
        (Some(0o2003), Some(0o1003), Some(0))
    );
    steps(&mut m, 2);
    assert_eq!(m.a(7), Some(0), "no request left");
    assert_eq!(m.f(), 0, "no flag in monitor mode");
}

#[test]
fn channel_disconnect_held_ready_and_master_clear() {
    // The sender's Disconnect ends an input transfer short of its limit, and
    // a partly assembled word is stored with zeros in what did not come.
    let mut m = monitor_cal("A1 10; A2 2000; A3 2010; CL,A1 A3; CA,A1 A2; PASS");
    steps(&mut m, 5);
    for parcel in [0x1111u16, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666] {
        assert!(m.channel_input(0o10, parcel));
    }
    m.channel_disconnect(0o10);
    let c = m.channel(0o10);
    assert_eq!((c.ca, c.active, c.interrupt), (0o2001, false, true));
    assert_eq!(m.mem(0o2000), Some(0x1111_2222_3333_4444));
    assert_eq!(m.mem(0o2001), Some(0x5555_6666_0000_0000));
    // A Ready that finds the channel inactive is held and answered when the
    // channel is activated; 0012j1 forgets it instead.
    let mut m = monitor_cal("A1 10; A2 2000; A3 2010; CL,A1 A3; CA,A1 A2; PASS");
    assert!(!m.channel_input(0o10, 0xabcd));
    steps(&mut m, 4);
    assert!(!m.channel_resumed(0o10));
    steps(&mut m, 1);
    assert!(m.channel_resumed(0o10));
    assert!(!m.channel_resumed(0o10), "told once");
    for parcel in [1u16, 2, 3] {
        assert!(m.channel_input(0o10, parcel));
    }
    assert_eq!(m.mem(0o2000), Some(0xabcd_0001_0002_0003));
    let mut m = monitor_cal("A1 10; A2 2000; A3 2010; MC,A1; CL,A1 A3; CA,A1 A2; PASS");
    assert!(!m.channel_input(0o10, 0xabcd));
    steps(&mut m, 6);
    assert!(!m.channel_resumed(0o10));
    // 0012j1 raises the Master Clear line of an output channel and 0012j0
    // drops it; both stop the channel and clear its flags.
    let mut m =
        monitor_cal("A1 11; A2 1000; A3 1002; CL,A1 A3; CA,A1 A2; MC,A1; PASS; CI,A1; PASS");
    m.load_words(0o1000, &[1, 2]).unwrap();
    steps(&mut m, 5);
    assert_eq!(m.channel_output(0o11), Some(0));
    steps(&mut m, 1);
    let c = m.channel(0o11);
    assert_eq!((c.master_clear, c.active), (true, false));
    assert_eq!(m.channel_output(0o11), None);
    steps(&mut m, 2);
    assert!(!m.channel(0o11).master_clear);
}

#[test]
fn channel_instructions_special_cases() {
    // X 5-9 and 5-37.  0010 to 0012 are passes outside monitor mode, with
    // j = 0, and when the low four bits of (Aj) are below 10 octal; "30
    // through 37 are valid".  k = 0 enters 1.  033 is not privileged.
    let mut m =
        monitor_cal("A1 7; CA,A1 A2; A1 31; CL,A1 A2; CA,A1 A0; A3 CA,A1; A1 12; A4 CA,A1; A5 CI");
    m.set_a(2, Some(0o1234));
    steps(&mut m, 2);
    assert!((0o10..0o20).all(|n| !m.channel(n).active));
    steps(&mut m, 4);
    let c = m.channel(0o11);
    assert_eq!((c.cl, c.ca, c.active), (0o1234, 1, true));
    assert_eq!(m.a(3), Some(1));
    steps(&mut m, 3);
    assert_eq!((m.a(4), m.a(5)), (Some(0), Some(0)));
    // a user program cannot start a channel but can read its state, and a
    // channel's interrupt request raises the I/O flag there
    let mut m = user_cal("A1 10; CL,A1 A2; CA,A1 A2; A3 CA,A1; A4 CI; A5 5");
    m.set_a(2, Some(0o1234));
    steps(&mut m, 5);
    assert!(!m.channel(0o10).active);
    assert_eq!((m.a(3), m.a(4), m.f()), (Some(0), Some(0), 0));
    m.set_io_request(true);
    steps(&mut m, 1);
    assert!(m.monitor_mode());
    assert_eq!(package_fields(&m, 0).6, flag::IO_INTERRUPT);
    // a channel number the machine does not have reads zero, an undefined
    // one is a test error
    let mut m = monitor_cal("A1 5; A3 CA,A1; A4 CE,A1; A5 CA,A2");
    m.set_a(2, None);
    steps(&mut m, 3);
    assert_eq!((m.a(3), m.a(4)), (Some(0), Some(0)));
    assert_eq!(error_of(&mut m).kind, ErrorKind::UndefinedValue);
}

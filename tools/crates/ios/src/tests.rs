//! Unit tests of the I/O Processor.  The module `kernel` holds the 14 checks
//! of the spec, part 12: sequences the COS 1.17 IOS kernel executes during
//! every boot, at the kernel addresses given there.

use crate::{channel, disassemble, parcels, Channels, Iop, NoChannels, RTC_PERIOD};

/// One parcel: operation code `f` (write it in octal) and field `d`.
fn ins(f: u16, d: u16) -> u16 {
    assert!(f < 0o200 && d < 0o1000);
    f << 9 | d
}

/// Where test programs are loaded.
const START: u16 = 0x1000;
/// Where `exec` puts the instruction it executes.
const SCRATCH: u16 = 0xFF00;

/// Devices on channels 5 to 47: they record the functions, read `input`,
/// and keep Busy, Done and Interrupt Enable with the common meaning of
/// functions 0, 6 and 7.
struct Devices {
    functions: Vec<(u8, u8, u16)>,
    busy: [bool; 0o50],
    done: [bool; 0o50],
    enable: [bool; 0o50],
    input: u16,
}

impl Devices {
    fn new() -> Devices {
        Devices {
            functions: Vec::new(),
            busy: [false; 0o50],
            done: [false; 0o50],
            enable: [false; 0o50],
            input: 0,
        }
    }
    /// Devices with channel `n` requesting an interrupt.
    fn requesting(n: usize) -> Devices {
        let mut devices = Devices::new();
        devices.done[n] = true;
        devices.enable[n] = true;
        devices
    }
}

impl Channels for Devices {
    fn function(&mut self, _: &mut [u16], channel: u8, function: u8, a: u16) -> Option<u16> {
        assert!((5..0o50).contains(&channel));
        self.functions.push((channel, function, a));
        let n = channel as usize;
        match function {
            0 => {
                self.busy[n] = false;
                self.done[n] = false;
            }
            6 => self.enable[n] = false,
            7 => self.enable[n] = true,
            _ => {}
        }
        (0o10..=0o13).contains(&function).then_some(self.input)
    }
    fn busy(&mut self, channel: u8) -> bool {
        self.busy[channel as usize]
    }
    fn done(&mut self, channel: u8) -> bool {
        self.done[channel as usize]
    }
    fn interrupt(&mut self) -> Option<u8> {
        (5..0o50u8).find(|&n| self.done[n as usize] && self.enable[n as usize])
    }
}

/// A processor about to execute `program`, which is loaded at `START`.
/// System Interrupt Enable is clear.
fn iop(program: &[u16]) -> Iop {
    let mut iop = Iop::new();
    iop.memory_mut()[START as usize..][..program.len()].copy_from_slice(program);
    iop.start_at(START);
    iop
}

/// Execute `n` instructions with no devices.
fn run(iop: &mut Iop, n: usize) {
    for _ in 0..n {
        iop.step(&mut NoChannels);
    }
}

/// Execute one instruction that is not in the program, and leave P alone.
/// Not for branches.
fn exec(iop: &mut Iop, parcels: &[u16]) -> u32 {
    let p = iop.p();
    iop.memory_mut()[SCRATCH as usize..][..parcels.len()].copy_from_slice(parcels);
    iop.set_p(SCRATCH);
    let clock_periods = iop.step(&mut NoChannels);
    assert_eq!(iop.p(), SCRATCH + parcels.len() as u16);
    iop.set_p(p);
    clock_periods
}

/// Set E the way a program does: `PXS : 14`.  A and C are kept.
fn set_e(iop: &mut Iop, e: u16) {
    let (a, c) = (iop.a(), iop.c());
    iop.set_a(e);
    exec(iop, &[ins(0o154, 2)]);
    iop.set_a(a);
    iop.set_c(c);
}

/// Write an exit stack location: `PXS : 14`, `PXS : 15`.  E, A and C are
/// kept.
fn set_exit_stack(iop: &mut Iop, location: u16, value: u16) {
    let (a, c, e) = (iop.a(), iop.c(), iop.e());
    set_e(iop, location);
    iop.set_a(value);
    exec(iop, &[ins(0o155, 2)]);
    set_e(iop, e as u16);
    iop.set_a(a);
    iop.set_c(c);
}

/// Carry and accumulator after one instruction on the given carry and
/// accumulator, with B and operand register 5 holding `x` and the parcel of
/// memory that operand register 6 addresses holding `x` too.
fn ca(parcels: &[u16], c: bool, a: u16, x: u16) -> (bool, u16) {
    let mut iop = iop(&[]);
    iop.set_b(x);
    iop.set_operand(5, x);
    iop.set_operand(6, 0x2000);
    iop.memory_mut()[0x2000] = x;
    iop.set_a(a);
    iop.set_c(c);
    exec(&mut iop, parcels);
    (iop.c(), iop.a())
}

mod kernel {
    use super::*;

    /// 1 [kern 22FA]: `A = A > 16` moves the carry to bit 0 of A.
    #[test]
    fn check_01_shift_right_16_saves_the_carry() {
        for c in [false, true] {
            assert_eq!(ca(&[ins(0o004, 16)], c, 0xBEEF, 0), (false, c as u16));
        }
    }

    /// 2 [kern 2334-2336]: `A = x`, `A = A < 16`, `A = A + y` restores the
    /// carry x and the accumulator y.
    #[test]
    fn check_02_shift_left_16_restores_the_carry() {
        for x in [0, 1] {
            for y in [0, 1, 0x8000, 0xFFFF] {
                let mut iop = iop(&[ins(0o010, x), ins(0o005, 16), ins(0o022, 61)]);
                iop.set_operand(61, y);
                iop.set_c(x == 0);
                run(&mut iop, 3);
                assert_eq!((iop.c(), iop.a()), (x == 1, y));
            }
        }
    }

    /// 3 [kern 2300-2303]: three `A = A >> 17` change nothing.
    #[test]
    fn check_03_circular_shift_17_is_a_delay() {
        for c in [false, true] {
            let mut iop = iop(&[ins(0o006, 17); 3]);
            iop.set_a(0xA5C3);
            iop.set_c(c);
            for _ in 0..3 {
                run(&mut iop, 1);
                assert_eq!((iop.c(), iop.a()), (c, 0xA5C3));
            }
        }
    }

    /// 4 [kern 231C-231E]: `A = n`, `A = A - 42` sets the carry exactly
    /// when n is 42 or more, and `R = OR[52], C # 0` goes by it.
    #[test]
    fn check_04_subtract_sets_carry_when_not_below() {
        for n in [0, 1, 41, 42, 43, 511] {
            let mut iop = iop(&[ins(0o020, 46), ins(0o013, 42), ins(0o131, 52), ins(0, 62)]);
            iop.set_operand(46, n);
            iop.set_operand(52, 0x210F);
            iop.set_c(true);
            run(&mut iop, 2);
            assert_eq!(iop.c(), n >= 42, "n = {n}");
            assert_eq!(iop.a(), n.wrapping_sub(42));
            run(&mut iop, 1);
            assert_eq!(iop.p(), if n >= 42 { 0x210F } else { START + 3 });
        }
    }

    /// 5 [kern 41EA-41EF]: `B = B + 1` from 511 gives B = 0 and A = 512,
    /// and `A = B` then reads 0.
    #[test]
    fn check_05_b_wraps_from_511() {
        let mut iop = iop(&[ins(0o056, 0), ins(0o050, 0)]);
        iop.set_b(511);
        iop.set_c(true);
        run(&mut iop, 1);
        assert_eq!((iop.b(), iop.a(), iop.c()), (0, 512, false));
        run(&mut iop, 1);
        assert_eq!(iop.a(), 0);
    }

    /// The loop itself [kern 41E8-41EF] clears operand registers 1 to 511
    /// and leaves register 0 alone.
    #[test]
    fn check_05_the_kernel_clears_the_operand_registers() {
        let program = [
            ins(0o010, 1), // A = 1
            ins(0o054, 0), // B = A
            ins(0o050, 0), // A = B
            ins(0o102, 5), // P = P + 5, A = 0
            ins(0o010, 0), // A = 0
            ins(0o064, 0), // (B) = A
            ins(0o056, 0), // B = B + 1
            ins(0o071, 5), // P = P - 5
        ];
        let mut iop = iop(&program);
        for d in 0..512 {
            iop.set_operand(d, 0x5A5A);
        }
        while iop.p() != START + 8 {
            run(&mut iop, 1);
            assert!(iop.counters().instructions < 4000);
        }
        assert_eq!(iop.operand(0), 0x5A5A);
        assert!(iop.operands()[1..].iter().all(|&r| r == 0));
    }

    /// 6 [kern 41FC-41FE, 41F5]: `PXS : 14` keeps 4 bits, and `PXS : 10`
    /// reads E with zeros above and clears the carry.
    #[test]
    fn check_06_e_has_four_bits() {
        let mut iop = iop(&[ins(0o154, 2), ins(0o150, 2)]);
        set_e(&mut iop, 15);
        iop.set_a(16);
        iop.set_c(true);
        run(&mut iop, 1);
        assert_eq!((iop.e(), iop.a(), iop.c()), (0, 16, true));
        run(&mut iop, 1);
        assert_eq!((iop.a(), iop.c()), (0, false));
        set_e(&mut iop, 0xFFFF);
        assert_eq!(iop.e(), 15);
        exec(&mut iop, &[ins(0o150, 2)]);
        assert_eq!(iop.a(), 15);
    }

    /// The loop itself [kern 41F0-4208]: set E to 1, clear locations 1 to
    /// 15, and write the handler address to location 0.
    #[test]
    fn check_06_the_kernel_clears_the_exit_stack() {
        let shift = ins(0o006, 17);
        let program = [
            ins(0o010, 1), // A = 1
            ins(0o154, 2), // PXS : 14
            shift,
            shift,
            shift,
            ins(0o150, 2),  // PXS : 10
            ins(0o102, 13), // P = P + 13, A = 0
            ins(0o010, 0),  // A = 0
            ins(0o155, 2),  // PXS : 15
            shift,
            shift,
            shift,
            ins(0o150, 2), // PXS : 10
            ins(0o012, 1), // A = A + 1
            ins(0o154, 2), // PXS : 14
            shift,
            shift,
            shift,
            ins(0o071, 13), // P = P - 13
            ins(0o014, 0),  // A = 0x22F7
            0x22F7,
            ins(0o155, 2), // PXS : 15
        ];
        let mut iop = iop(&program);
        for location in 0..16 {
            set_exit_stack(&mut iop, location, 0x7777);
        }
        while iop.p() != START + program.len() as u16 {
            run(&mut iop, 1);
            assert!(iop.counters().instructions < 1000);
        }
        let mut expected = [0; 16];
        expected[0] = 0x22F7;
        assert_eq!(iop.exit_stack(), &expected);
        assert_eq!(iop.e(), 0);
    }

    /// 7 [kern 3F91-3F92]: `OR[x] = OR[x] - 1` leaves the result in A for
    /// `P = P - n, A # 0`.
    #[test]
    fn check_07_decrement_leaves_the_result_in_a() {
        let mut iop = iop(&[ins(0o027, 502), ins(0o107, 1)]);
        iop.set_operand(502, 3);
        iop.set_a(0x1234);
        for left in [2, 1] {
            run(&mut iop, 2);
            assert_eq!((iop.a(), iop.operand(502)), (left, left));
            assert_eq!(iop.p(), START);
        }
        run(&mut iop, 2);
        assert_eq!((iop.a(), iop.operand(502)), (0, 0));
        assert_eq!(iop.p(), START + 2);
    }

    /// 8 [kern 2303-2304, 211C]: `R = OR[52]` with a PASS that carries a
    /// code after it; the routine finds the address of that PASS with
    /// `PXS : 11`.
    #[test]
    fn check_08_return_address_points_at_the_code_parcel() {
        let mut iop = iop(&[ins(0o132, 52), ins(0, 70)]);
        iop.memory_mut()[0x211C] = ins(0o151, 2);
        iop.set_operand(52, 0x211C);
        iop.set_a(0);
        set_e(&mut iop, 3);
        run(&mut iop, 1);
        assert_eq!((iop.p(), iop.e()), (0x211C, 4));
        assert_eq!(iop.exit_stack()[4], START + 1);
        run(&mut iop, 1);
        assert_eq!(iop.a(), START + 1);
        // the code is the d field of the parcel there
        assert_eq!(iop.memory()[iop.a() as usize] & 0o777, 70);
    }

    /// 9 [kern 0005]: `P = OR[0] + k` with zero in operand register 0 goes
    /// to k and sets the Program Fetch Request flag, which does not
    /// interrupt: channel 1 is not enabled after Master Clear.
    #[test]
    fn check_09_jump_through_register_0_does_not_interrupt() {
        let mut iop = Iop::new();
        iop.memory_mut()[..6].copy_from_slice(&[
            ins(0o070, 3),
            ins(0o076, 52),
            ins(0, 7),
            ins(0o010, 0),
            ins(0o024, 0),
            ins(0o075, 0),
        ]);
        iop.memory_mut()[6] = 0x41E4;
        iop.memory_mut()[0x41E4] = ins(0o003, 0);
        iop.set_operand(0, 0x1234);
        // the dead start interrupt
        let mut devices = Devices::requesting(5);
        iop.step(&mut devices);
        assert_eq!((iop.p(), iop.e(), iop.held()), (0, 1, false));
        for _ in 0..4 {
            iop.step(&mut devices);
        }
        assert_eq!(iop.p(), 0x41E4);
        assert!(iop.done(channel::PFR));
        assert_eq!(iop.pfr_register(), 0);
        assert!(!iop.enabled(channel::PFR));
        // with interrupts enabled and no other request nothing happens
        devices.done[5] = false;
        assert_eq!(iop.interrupt_request(&mut devices), None);
        for _ in 0..4 {
            iop.step(&mut devices);
        }
        assert!(iop.interrupt_enable());
        assert_eq!(iop.counters().interrupts, 1);
        assert_eq!(iop.p(), 0x41E8);
    }

    /// 10 [kern 4246-424E]: `IOB : 6`, `IOB : 0` on channels 3 to 39 with
    /// no devices does nothing.
    #[test]
    fn check_10_clearing_every_channel_is_harmless() {
        let program = [
            ins(0o010, 3),  // A = 3
            ins(0o054, 0),  // B = A
            ins(0o050, 0),  // A = B
            ins(0o013, 40), // A = A - 40
            ins(0o102, 5),  // P = P + 5, A = 0
            ins(0o166, 0),  // IOB : 6
            ins(0o160, 0),  // IOB : 0
            ins(0o056, 0),  // B = B + 1
            ins(0o071, 6),  // P = P - 6
        ];
        let mut iop = iop(&program);
        set_exit_stack(&mut iop, 0, 0x22F7);
        set_e(&mut iop, 1);
        let stack = *iop.exit_stack();
        while iop.p() != START + 9 {
            run(&mut iop, 1);
            assert!(iop.counters().instructions < 1000);
        }
        assert_eq!((iop.b(), iop.a(), iop.e()), (40, 0, 1));
        assert_eq!(iop.exit_stack(), &stack);
        for n in 1..5 {
            assert!(!iop.enabled(n) && !iop.done(n));
        }

        // with devices: each of channels 5 to 39 gets function 6, then 0,
        // with the kernel's A = B - 40
        let mut iop = super::iop(&program);
        let mut devices = Devices::new();
        while iop.p() != START + 9 {
            iop.step(&mut devices);
        }
        let expected: Vec<(u8, u8, u16)> = (5..40u8)
            .flat_map(|n| [6, 0].map(|f| (n, f, (n as u16).wrapping_sub(40))))
            .collect();
        assert_eq!(devices.functions, expected);
    }

    /// 11 [kern 2337-2338]: after `I = 1`, `EXIT` with a request waiting,
    /// the interrupt comes after the EXIT at the earliest: here after the
    /// first instruction of the program returned to.
    #[test]
    fn check_11_interrupt_waits_for_the_exit() {
        const HANDLER: u16 = 0x22F7;
        const RETURN: u16 = 0x3000;
        let mut iop = iop(&[ins(0o003, 0), ins(0o001, 0)]);
        set_exit_stack(&mut iop, 0, HANDLER);
        set_exit_stack(&mut iop, 1, RETURN);
        set_e(&mut iop, 1);
        let mut devices = Devices::requesting(0o20);
        iop.step(&mut devices);
        assert_eq!((iop.p(), iop.interrupt_enable()), (START + 1, false));
        iop.step(&mut devices);
        assert_eq!(
            (iop.p(), iop.e(), iop.interrupt_enable()),
            (RETURN, 0, false)
        );
        // one instruction of the interrupted program (a PASS), then the
        // interrupt
        iop.step(&mut devices);
        assert_eq!((iop.p(), iop.interrupt_enable()), (RETURN + 1, true));
        assert_eq!(iop.counters().interrupts, 0);
        iop.step(&mut devices);
        assert_eq!(
            (iop.p(), iop.e(), iop.interrupt_enable()),
            (HANDLER, 1, false)
        );
        assert_eq!(iop.exit_stack()[1], RETURN + 1);
        assert_eq!(iop.counters().interrupts, 1);
    }

    /// 12 [kern 230C-2312]: `IOR : 10` with nothing waiting reads 0.
    #[test]
    fn check_12_ior_reads_zero_when_idle() {
        let mut iop = iop(&[ins(0o150, 0)]);
        iop.set_a(0xFFFF);
        iop.set_c(true);
        run(&mut iop, 1);
        assert_eq!((iop.a(), iop.c()), (0, false));
    }

    /// 13 [kern 36DA-36E0]: read E, subtract 1, `PXS : 14`, three shifts
    /// and a jump throw away the return address on top of the stack.
    #[test]
    fn check_13_dropping_a_return_address() {
        let shift = ins(0o006, 17);
        let program = [
            ins(0o150, 2), // PXS : 10
            ins(0o013, 1), // A = A - 1
            ins(0o154, 2), // PXS : 14
            shift,
            shift,
            shift,
            ins(0o075, 0), // P = OR[0] + 0x2339
            0x2339,
        ];
        let mut iop = iop(&program);
        set_exit_stack(&mut iop, 2, 0x1111);
        set_exit_stack(&mut iop, 3, 0x2222);
        set_e(&mut iop, 3);
        iop.memory_mut()[0x2339] = ins(0o001, 0);
        run(&mut iop, 7);
        assert_eq!((iop.p(), iop.e()), (0x2339, 2));
        run(&mut iop, 1);
        assert_eq!((iop.p(), iop.e()), (0x1111, 1));
    }

    /// 14 [kern 4A0B]: `R = P - 263` goes 263 parcels back from the
    /// instruction and returns to the parcel after it.
    #[test]
    fn check_14_long_backward_return_jump() {
        let mut iop = Iop::new();
        iop.memory_mut()[0x4A0B] = ins(0o073, 263);
        iop.start_at(0x4A0B);
        run(&mut iop, 1);
        assert_eq!((iop.p(), iop.e()), (0x4904, 1));
        assert_eq!(iop.exit_stack()[1], 0x4A0C);
    }
}

// ---- arithmetic and the carry

#[test]
fn load_and_logical_product_clear_the_carry() {
    assert_eq!(ca(&[ins(0o010, 0o777)], true, 0xFFFF, 0), (false, 0o777));
    assert_eq!(ca(&[ins(0o011, 0o707)], true, 0xFFFF, 0), (false, 0o707));
    assert_eq!(ca(&[ins(0o014, 0), 0xABCD], true, 0, 0), (false, 0xABCD));
    assert_eq!(
        ca(&[ins(0o015, 0), 0xF0F0], true, 0xFF00, 0),
        (false, 0xF000)
    );
    assert_eq!(ca(&[ins(0o020, 5)], true, 0, 0x8001), (false, 0x8001));
    assert_eq!(ca(&[ins(0o021, 5)], true, 0xFF00, 0x0FF0), (false, 0x0F00));
    assert_eq!(ca(&[ins(0o030, 6)], true, 0, 0x8001), (false, 0x8001));
    assert_eq!(ca(&[ins(0o031, 6)], true, 0xFF00, 0x0FF0), (false, 0x0F00));
    assert_eq!(ca(&[ins(0o050, 0)], true, 0xFFFF, 0o123), (false, 0o123));
    assert_eq!(ca(&[ins(0o051, 0)], true, 0xFFFF, 0o123), (false, 0o123));
}

/// Add complements the carry on a carry out of bit 15; it does not set it
/// [HW 6-13].
#[test]
fn add_toggles_the_carry() {
    for (f, k) in [
        (0o012, None),
        (0o016, Some(1)),
        (0o022, None),
        (0o032, None),
        (0o052, None),
    ] {
        let parcels = match k {
            Some(k) => vec![ins(f, 0), k],
            None => vec![ins(f, 1)],
        };
        // operand 1 everywhere: d of 012, k of 016, register 5, memory, B
        let parcels = if f == 0o022 {
            vec![ins(f, 5)]
        } else if f == 0o032 {
            vec![ins(f, 6)]
        } else {
            parcels
        };
        assert_eq!(ca(&parcels, false, 0xFFFE, 1), (false, 0xFFFF), "{f:o}");
        assert_eq!(ca(&parcels, true, 0xFFFE, 1), (true, 0xFFFF), "{f:o}");
        assert_eq!(ca(&parcels, false, 0xFFFF, 1), (true, 0), "{f:o}");
        assert_eq!(ca(&parcels, true, 0xFFFF, 1), (false, 0), "{f:o}");
    }
    assert_eq!(
        ca(&[ins(0o016, 0), 0xFFFF], false, 0xFFFF, 0),
        (true, 0xFFFE)
    );
    assert_eq!(ca(&[ins(0o022, 5)], true, 0x8000, 0x8000), (false, 0));
}

/// Subtract complements the carry when the accumulator is not below the
/// operand, a zero operand included [HW 4-2, 6-14].
#[test]
fn subtract_toggles_the_carry_without_a_borrow() {
    assert_eq!(ca(&[ins(0o013, 3)], false, 5, 0), (true, 2));
    assert_eq!(ca(&[ins(0o013, 3)], true, 5, 0), (false, 2));
    assert_eq!(ca(&[ins(0o013, 5)], false, 5, 0), (true, 0));
    assert_eq!(ca(&[ins(0o013, 6)], false, 5, 0), (false, 0xFFFF));
    assert_eq!(ca(&[ins(0o013, 6)], true, 5, 0), (true, 0xFFFF));
    assert_eq!(ca(&[ins(0o013, 0)], false, 0, 0), (true, 0));
    assert_eq!(ca(&[ins(0o013, 0)], true, 7, 0), (false, 7));
    assert_eq!(ca(&[ins(0o017, 0), 0xFFFF], false, 0xFFFF, 0), (true, 0));
    assert_eq!(
        ca(&[ins(0o017, 0), 0xFFFF], false, 0xFFFE, 0),
        (false, 0xFFFF)
    );
    assert_eq!(ca(&[ins(0o023, 5)], false, 0x8000, 0x7FFF), (true, 1));
    assert_eq!(ca(&[ins(0o033, 6)], false, 0x7FFF, 0x8000), (false, 0xFFFF));
    assert_eq!(ca(&[ins(0o053, 0)], false, 0o777, 0o777), (true, 0));
    assert_eq!(ca(&[ins(0o053, 0)], false, 0o776, 0o777), (false, 0xFFFF));
}

/// The store forms: 024 and 034 change nothing but the operand; 025 and
/// 035 add and store; increment and decrement start from a cleared carry
/// and leave the result in A [HW 6-24 to 6-26, 6-32, 6-33].
#[test]
fn register_and_memory_stores() {
    let mut iop = iop(&[]);
    iop.set_operand(6, 0x2000);
    iop.set_a(0x1234);
    iop.set_c(true);
    exec(&mut iop, &[ins(0o024, 5)]);
    exec(&mut iop, &[ins(0o034, 6)]);
    assert_eq!((iop.operand(5), iop.memory()[0x2000]), (0x1234, 0x1234));
    assert_eq!((iop.a(), iop.c()), (0x1234, true));

    // add and store: 0x1234 + 0x1234, then + 0xF000 with a carry out
    exec(&mut iop, &[ins(0o025, 5)]);
    assert_eq!((iop.operand(5), iop.a(), iop.c()), (0x2468, 0x2468, true));
    iop.set_a(0xF000);
    exec(&mut iop, &[ins(0o035, 6)]);
    assert_eq!(
        (iop.memory()[0x2000], iop.a(), iop.c()),
        (0x0234, 0x0234, false)
    );

    for (increment, decrement, register) in [(0o026, 0o027, true), (0o036, 0o037, false)] {
        let mut check = |f: u16, before: u16, after: u16, carry: bool| {
            let d = if register { 5 } else { 6 };
            if register {
                iop.set_operand(5, before);
            } else {
                iop.memory_mut()[0x2000] = before;
            }
            for c in [false, true] {
                iop.set_a(0x5555);
                iop.set_c(c);
                exec(&mut iop, &[ins(f, d)]);
                assert_eq!((iop.a(), iop.c()), (after, carry), "{f:o} on {before:#x}");
                let stored = if register {
                    iop.operand(5)
                } else {
                    iop.memory()[0x2000]
                };
                assert_eq!(stored, after);
                // put the operand back for the second pass
                if register {
                    iop.set_operand(5, before);
                } else {
                    iop.memory_mut()[0x2000] = before;
                }
            }
        };
        check(increment, 0, 1, false);
        check(increment, 0xFFFE, 0xFFFF, false);
        check(increment, 0xFFFF, 0, true);
        // all ones plus the operand: the carry sets unless the operand is 0
        check(decrement, 0, 0xFFFF, false);
        check(decrement, 1, 0, true);
        check(decrement, 0x8000, 0x7FFF, true);
    }
}

/// 054 to 057 and 064 to 067: B keeps the low 9 bits; the registers B
/// selects behave like those d selects [HW 6-47 to 6-50].
#[test]
fn b_register_and_registers_selected_by_b() {
    let mut iop = iop(&[]);
    iop.set_a(0xFE03);
    iop.set_c(true);
    exec(&mut iop, &[ins(0o054, 0)]);
    assert_eq!((iop.b(), iop.a(), iop.c()), (0o003, 0xFE03, true));
    // B = A + B: the whole sum stays in A
    exec(&mut iop, &[ins(0o055, 0)]);
    assert_eq!((iop.b(), iop.a(), iop.c()), (0o006, 0xFE06, true));
    // B = B - 1 from 0: A is all ones, B is 511, no carry
    iop.set_b(0);
    exec(&mut iop, &[ins(0o057, 0)]);
    assert_eq!((iop.b(), iop.a(), iop.c()), (511, 0xFFFF, false));
    exec(&mut iop, &[ins(0o057, 0)]);
    assert_eq!((iop.b(), iop.a(), iop.c()), (510, 510, true));
    // B = B + 1 never carries
    exec(&mut iop, &[ins(0o056, 0)]);
    assert_eq!((iop.b(), iop.a(), iop.c()), (511, 511, false));

    iop.set_b(300);
    iop.set_operand(300, 0xFFFF);
    iop.set_a(2);
    exec(&mut iop, &[ins(0o062, 0)]); // A = A + (B)
    assert_eq!((iop.a(), iop.c()), (1, true));
    exec(&mut iop, &[ins(0o063, 0)]); // A = A - (B): 1 - 0xFFFF borrows
    assert_eq!((iop.a(), iop.c()), (2, true));
    exec(&mut iop, &[ins(0o061, 0)]); // A = A & (B)
    assert_eq!((iop.a(), iop.c()), (2, false));
    exec(&mut iop, &[ins(0o064, 0)]); // (B) = A
    assert_eq!(iop.operand(300), 2);
    exec(&mut iop, &[ins(0o065, 0)]); // (B) = A + (B)
    assert_eq!((iop.operand(300), iop.a()), (4, 4));
    exec(&mut iop, &[ins(0o066, 0)]); // (B) = (B) + 1
    assert_eq!((iop.operand(300), iop.a(), iop.c()), (5, 5, false));
    exec(&mut iop, &[ins(0o067, 0)]); // (B) = (B) - 1
    assert_eq!((iop.operand(300), iop.a(), iop.c()), (4, 4, true));
    exec(&mut iop, &[ins(0o060, 0)]); // A = (B)
    assert_eq!((iop.a(), iop.c()), (4, false));
    // the d field of these is ignored
    exec(&mut iop, &[ins(0o060, 0o777)]);
    assert_eq!(iop.a(), 4);
}

/// The shifts work on carry and accumulator as 17 bits [HW 4-3, 6-7 to
/// 6-10].
#[test]
fn shifts() {
    // end off, right: zeros enter at the carry
    assert_eq!(ca(&[ins(0o004, 1)], true, 0x0001, 0), (false, 0x8000));
    assert_eq!(ca(&[ins(0o004, 0)], true, 0x1234, 0), (true, 0x1234));
    assert_eq!(ca(&[ins(0o004, 4)], true, 0x1234, 0), (false, 0x1123));
    assert_eq!(ca(&[ins(0o004, 17)], true, 0xFFFF, 0), (false, 0));
    assert_eq!(ca(&[ins(0o004, 31)], true, 0xFFFF, 0), (false, 0));
    // end off, left: bits leave through the carry
    assert_eq!(ca(&[ins(0o005, 1)], true, 0x8001, 0), (true, 0x0002));
    assert_eq!(ca(&[ins(0o005, 1)], true, 0x0001, 0), (false, 0x0002));
    assert_eq!(ca(&[ins(0o005, 16)], false, 0x0003, 0), (true, 0));
    assert_eq!(ca(&[ins(0o005, 17)], true, 0xFFFF, 0), (false, 0));
    // circular, right: bit 0 of A returns to the carry
    assert_eq!(ca(&[ins(0o006, 1)], false, 0x0001, 0), (true, 0));
    assert_eq!(ca(&[ins(0o006, 1)], true, 0x0000, 0), (false, 0x8000));
    assert_eq!(ca(&[ins(0o006, 8)], true, 0x12CD, 0), (true, 0x9B12));
    // circular, left: the carry returns to bit 0 of A
    assert_eq!(ca(&[ins(0o007, 1)], true, 0x0000, 0), (false, 0x0001));
    assert_eq!(ca(&[ins(0o007, 1)], false, 0x8000, 0), (true, 0));
    assert_eq!(ca(&[ins(0o007, 17)], true, 0x1234, 0), (true, 0x1234));
    // left by n undoes right by n
    for n in 0..=17 {
        let (c, a) = ca(&[ins(0o006, n)], true, 0x1234, 0);
        assert_eq!(ca(&[ins(0o007, n)], c, a, 0), (true, 0x1234));
    }
    // the count is the low 5 bits of d [HW 6-7]: 32 shifts by 0, 33 by 1
    assert_eq!(ca(&[ins(0o004, 32)], true, 0x1234, 0), (true, 0x1234));
    assert_eq!(ca(&[ins(0o005, 33)], false, 0x4000, 0), (false, 0x8000));
    // a circular shift of 18 to 31 places turns by the count modulo 17
    assert_eq!(ca(&[ins(0o006, 18)], false, 0x0001, 0), (true, 0));
    assert_eq!(ca(&[ins(0o007, 18)], true, 0x0000, 0), (false, 0x0001));
    assert_eq!(
        ca(&[ins(0o007, 31)], true, 0x1234, 0),
        ca(&[ins(0o007, 14)], true, 0x1234, 0)
    );
    // 044 to 047 take the count from B and ignore d
    assert_eq!(ca(&[ins(0o044, 0o777)], true, 0x1234, 4), (false, 0x1123));
    assert_eq!(ca(&[ins(0o045, 0)], true, 0x8001, 1), (true, 0x0002));
    assert_eq!(ca(&[ins(0o046, 0)], true, 0x12CD, 8), (true, 0x9B12));
    assert_eq!(ca(&[ins(0o047, 0)], false, 0x8000, 1), (true, 0));
    assert_eq!(ca(&[ins(0o044, 0)], true, 0x1234, 0o440), (true, 0x1234));
}

#[test]
fn long_shifts_are_counted() {
    let mut iop = iop(&[]);
    for d in [0, 16, 17] {
        exec(&mut iop, &[ins(0o006, d)]);
    }
    assert_eq!(iop.counters().long_shifts, 0);
    exec(&mut iop, &[ins(0o006, 18)]);
    iop.set_b(40);
    exec(&mut iop, &[ins(0o045, 0)]);
    assert_eq!(iop.counters().long_shifts, 2);
}

// ---- branches

/// The eight unconditional branches [HW 6-59 to 6-66].
#[test]
fn unconditional_branches() {
    // (f, d, k, target, return address or none)
    let cases = [
        (0o070, 511, None, START + 511, None),
        (0o071, 511, None, START - 511, None),
        (0o072, 3, None, START + 3, Some(START + 1)),
        (0o073, 3, None, START - 3, Some(START + 1)),
        (0o074, 9, None, 0x4000, None),
        (0o075, 9, Some(0xC123), 0x0123, None),
        (0o076, 9, None, 0x4000, Some(START + 1)),
        (0o077, 9, Some(0x0010), 0x4010, Some(START + 2)),
    ];
    for (f, d, k, target, return_address) in cases {
        let mut iop = iop(&[ins(f, d), k.unwrap_or(0)]);
        iop.set_operand(9, 0x4000);
        iop.set_a(0x1357);
        iop.set_c(true);
        run(&mut iop, 1);
        assert_eq!(iop.p(), target, "{f:o}");
        assert_eq!((iop.a(), iop.c()), (0x1357, true));
        match return_address {
            Some(address) => {
                assert_eq!(iop.e(), 1);
                assert_eq!(iop.exit_stack()[1], address);
            }
            None => assert_eq!(iop.e(), 0),
        }
    }
}

/// Branch addresses are modulo 2**16.
#[test]
fn branch_addresses_wrap() {
    let mut iop = Iop::new();
    iop.memory_mut()[0xFFFF] = ins(0o070, 2);
    iop.memory_mut()[1] = ins(0o071, 3);
    iop.start_at(0xFFFF);
    run(&mut iop, 1);
    assert_eq!(iop.p(), 1);
    run(&mut iop, 1);
    assert_eq!(iop.p(), 0xFFFE);
}

/// 100 to 137: mode in f bits 4 to 2, condition in bits 1 to 0; a branch
/// not taken goes on after the instruction, past k if it has one
/// [HW 6-67].
#[test]
fn conditional_branches() {
    for mode in 0..8 {
        for condition in 0..4 {
            for (c, a) in [(false, 0), (false, 5), (true, 0), (true, 5)] {
                let f = 0o100 + mode * 4 + condition;
                let mut iop = iop(&[ins(f, 7), 0x0100]);
                iop.set_operand(7, 0x4000);
                iop.set_a(a);
                iop.set_c(c);
                run(&mut iop, 1);
                let taken = match condition {
                    0 => !c,
                    1 => c,
                    2 => a == 0,
                    _ => a != 0,
                };
                let two = mode == 5 || mode == 7;
                let next = START + if two { 2 } else { 1 };
                let target = match mode {
                    0 | 2 => START + 7,
                    1 | 3 => START - 7,
                    4 | 6 => 0x4000,
                    _ => 0x4100,
                };
                let returns = matches!(mode, 2 | 3 | 6 | 7);
                assert_eq!(iop.p(), if taken { target } else { next }, "{f:o}");
                assert_eq!(iop.e(), (taken && returns) as u8, "{f:o}");
                if taken && returns {
                    assert_eq!(iop.exit_stack()[1], next, "{f:o}");
                }
                assert_eq!((iop.a(), iop.c()), (a, c));
            }
        }
    }
}

// ---- the program exit stack

/// EXIT takes P from the location E points at and then decrements E
/// [HW 6-4].
#[test]
fn exit_returns_to_the_caller() {
    let mut iop = iop(&[ins(0o072, 2), ins(0, 0), ins(0o001, 0)]);
    run(&mut iop, 1);
    assert_eq!((iop.p(), iop.e()), (START + 2, 1));
    run(&mut iop, 1);
    assert_eq!((iop.p(), iop.e()), (START + 1, 0));
    assert!(!iop.done(channel::PXS));
}

/// A return jump that advances E to 14 sets the Exit Stack Boundary flag
/// [HW 6-65], for every kind of return jump (spec Q3), and no other value
/// of E does.
#[test]
fn boundary_flag_on_a_return_jump_to_14() {
    for f in [0o072, 0o073, 0o076, 0o077, 0o112, 0o116, 0o132, 0o136] {
        for e in 0..16 {
            let mut iop = iop(&[ins(f, 5), 0]);
            set_e(&mut iop, e);
            iop.set_a(0);
            run(&mut iop, 1);
            assert_eq!(iop.e() as u16, (e + 1) % 16);
            assert_eq!(iop.done(channel::PXS), e == 13, "{f:o} from E = {e}");
        }
    }
    // a conditional return jump that is not taken pushes nothing
    let mut iop = iop(&[ins(0o112, 5)]);
    set_e(&mut iop, 13);
    iop.set_a(1);
    run(&mut iop, 1);
    assert_eq!((iop.e(), iop.done(channel::PXS)), (13, false));
}

/// With channel 2 enabled, the return jump to 14 completes and the
/// interrupt then stores the address of the subroutine in location 15
/// [HW 3-9].
#[test]
fn boundary_interrupt_after_the_return_jump() {
    let mut iop = iop(&[ins(0o147, 2), ins(0o003, 0), ins(0, 0), ins(0o072, 9)]);
    set_exit_stack(&mut iop, 0, 0x22F7);
    set_e(&mut iop, 13);
    run(&mut iop, 4);
    assert_eq!((iop.p(), iop.e()), (START + 12, 14));
    assert_eq!(iop.exit_stack()[14], START + 4);
    assert_eq!(iop.interrupt_request(&mut NoChannels), Some(2));
    run(&mut iop, 1);
    assert_eq!((iop.p(), iop.e()), (0x22F7, 15));
    assert_eq!(iop.exit_stack()[15], START + 12);
    assert!(!iop.interrupt_enable());
    // PXS : 0 clears the flag, PXS : 6 the enable
    exec(&mut iop, &[ins(0o150, 0)]);
    assert_eq!(iop.a(), 2);
    exec(&mut iop, &[ins(0o140, 2)]);
    assert!(!iop.done(channel::PXS) && iop.enabled(channel::PXS));
    exec(&mut iop, &[ins(0o146, 2)]);
    assert!(!iop.enabled(channel::PXS));
}

/// An interrupt that takes E from 13 to 14 does not set the flag
/// [HW 3-10], and cray-sim's does.
#[test]
fn no_boundary_flag_on_an_interrupt_to_14() {
    let mut iop = iop(&[ins(0o003, 0), ins(0, 0), ins(0, 0)]);
    set_exit_stack(&mut iop, 0, 0x22F7);
    set_e(&mut iop, 13);
    let mut devices = Devices::requesting(6);
    for _ in 0..3 {
        iop.step(&mut devices);
    }
    assert_eq!((iop.p(), iop.e()), (0x22F7, 14));
    assert_eq!(iop.exit_stack()[14], START + 2);
    assert!(!iop.done(channel::PXS));
    assert_eq!(iop.counters().boundary_flags, 0);
}

/// From 15, E wraps to 0 and the handler address is overwritten [HW 3-9].
#[test]
fn exit_stack_pointer_wraps_from_15() {
    let mut iop = iop(&[ins(0o072, 5)]);
    set_exit_stack(&mut iop, 0, 0x22F7);
    set_e(&mut iop, 15);
    run(&mut iop, 1);
    assert_eq!((iop.p(), iop.e()), (START + 5, 0));
    assert_eq!(iop.exit_stack()[0], START + 1);
    assert!(!iop.done(channel::PXS));
}

/// EXIT with E = 0: the decrement is blocked, the flag sets and the
/// program goes on at the address in location 0 [HW 6-4, 3-9]; in cray-sim
/// E becomes 15.  Nothing is pushed and System Interrupt Enable is left
/// alone (spec Q1, Q2).
#[test]
fn exit_with_e_0() {
    let mut iop = iop(&[ins(0o001, 0)]);
    set_exit_stack(&mut iop, 0, 0x22F7);
    let stack = *iop.exit_stack();
    run(&mut iop, 1);
    assert_eq!((iop.p(), iop.e()), (0x22F7, 0));
    assert!(iop.done(channel::PXS));
    assert!(!iop.interrupt_enable());
    assert_eq!(iop.exit_stack(), &stack);
    assert_eq!(iop.counters().boundary_flags, 1);
}

/// The channel 2 functions [HW 5-10].
#[test]
fn exit_stack_channel() {
    let mut iop = iop(&[]);
    for location in 0..16 {
        set_exit_stack(&mut iop, location, 0x1000 + location);
    }
    for location in 0..16 {
        set_e(&mut iop, location);
        iop.set_c(true);
        exec(&mut iop, &[ins(0o151, 2)]);
        assert_eq!((iop.a(), iop.c()), (0x1000 + location, false));
        exec(&mut iop, &[ins(0o150, 2)]);
        assert_eq!(iop.a(), location);
    }
    // through B: IOB : 15
    iop.set_b(2);
    iop.set_a(0xABCD);
    iop.set_c(true);
    exec(&mut iop, &[ins(0o175, 0o777)]);
    assert_eq!(iop.exit_stack()[15], 0xABCD);
    assert_eq!((iop.a(), iop.c()), (0xABCD, true));
    // a function the channel does not have reads zero
    exec(&mut iop, &[ins(0o152, 2)]);
    assert_eq!((iop.a(), iop.c()), (0, false));
}

// ---- interrupts

/// The interrupt sequence [HW 5-19]: System Interrupt Enable clears, E
/// advances, P is stored, the handler address comes from location 0.
/// Nothing else changes.
#[test]
fn interrupt_sequence() {
    let mut iop = iop(&[ins(0o003, 0), ins(0o010, 9), ins(0o010, 8)]);
    set_exit_stack(&mut iop, 0, 0x22F7);
    set_e(&mut iop, 4);
    iop.set_b(0o321);
    let before = iop.counters().instructions;
    let mut devices = Devices::requesting(0o17);
    iop.step(&mut devices);
    iop.step(&mut devices);
    assert!(iop.interrupt_enable());
    iop.set_c(true);
    let clock_periods = iop.step(&mut devices);
    assert_eq!(clock_periods, 9);
    assert_eq!(
        (iop.p(), iop.e(), iop.interrupt_enable()),
        (0x22F7, 5, false)
    );
    assert_eq!(iop.exit_stack()[5], START + 2);
    assert_eq!((iop.a(), iop.c(), iop.b()), (9, true, 0o321));
    assert_eq!(iop.counters().interrupts, 1);
    assert_eq!(iop.counters().instructions, before + 2);
    // with the flag clear the request waits
    iop.step(&mut devices);
    assert_eq!(iop.counters().interrupts, 1);
}

/// `IOR : 10` reads the lowest numbered requesting channel, whatever
/// System Interrupt Enable is, and follows the flags [HW 5-9].
#[test]
fn interrupt_request_priority() {
    let mut iop = iop(&[]);
    let mut devices = Devices::new();
    let read = |iop: &mut Iop, devices: &mut Devices| {
        iop.memory_mut()[SCRATCH as usize] = ins(0o150, 0);
        iop.set_p(SCRATCH);
        iop.set_c(true);
        iop.step(devices);
        assert!(!iop.c());
        iop.a()
    };
    assert_eq!(read(&mut iop, &mut devices), 0);
    // Done without Interrupt Enable, and Interrupt Enable without Done,
    // do not request
    devices.done[0o20] = true;
    devices.enable[0o21] = true;
    assert_eq!(read(&mut iop, &mut devices), 0);
    devices.enable[0o20] = true;
    devices.done[0o47] = true;
    devices.enable[0o47] = true;
    assert_eq!(read(&mut iop, &mut devices), 0o20);
    // the real-time clock (channel 4) comes before every device
    iop.advance(RTC_PERIOD);
    assert_eq!(read(&mut iop, &mut devices), 0o20);
    exec(&mut iop, &[ins(0o147, 4)]);
    assert_eq!(read(&mut iop, &mut devices), 4);
    // the exit stack boundary (channel 2) before the clock
    exec(&mut iop, &[ins(0o147, 2)]);
    set_e(&mut iop, 13);
    iop.memory_mut()[SCRATCH as usize] = ins(0o072, 1);
    iop.set_p(SCRATCH);
    iop.step(&mut devices);
    assert_eq!(read(&mut iop, &mut devices), 2);
    // the program fetch request (channel 1) before that
    exec(&mut iop, &[ins(0o147, 1)]);
    iop.memory_mut()[SCRATCH as usize] = ins(0o074, 0);
    iop.set_p(SCRATCH);
    iop.step(&mut devices);
    assert_eq!(read(&mut iop, &mut devices), 1);
    // clearing Done or Interrupt Enable passes the request on
    exec(&mut iop, &[ins(0o140, 1)]);
    assert_eq!(read(&mut iop, &mut devices), 2);
    exec(&mut iop, &[ins(0o146, 2)]);
    assert_eq!(read(&mut iop, &mut devices), 4);
    exec(&mut iop, &[ins(0o140, 4)]);
    assert_eq!(read(&mut iop, &mut devices), 0o20);
    devices.done[0o20] = false;
    assert_eq!(read(&mut iop, &mut devices), 0o47);
}

/// After 003 the flag sets at the completion of the next instruction that
/// is not 001, 003, 040 to 043, 070 to 137 or 140 to 177 (spec part 7.3,
/// [HW 5-19, 5-20]).
#[test]
fn delayed_interrupt_enable() {
    // each of these holds the enable off
    let holding = [
        ins(0o003, 0),
        ins(0o040, 0),
        ins(0o041, 0),
        ins(0o042, 0),
        ins(0o043, 0),
        ins(0o100, 0),
        ins(0o101, 1),
        ins(0o140, 3),
        ins(0o150, 2),
        ins(0o160, 0),
        ins(0o177, 0),
    ];
    for parcel in holding {
        let mut iop = iop(&[ins(0o003, 0), parcel, ins(0, 0)]);
        iop.set_c(false);
        run(&mut iop, 1);
        assert!(!iop.interrupt_enable() && iop.interrupt_enable_delayed());
        run(&mut iop, 1);
        assert!(!iop.interrupt_enable(), "{parcel:06o}");
        assert!(iop.interrupt_enable_delayed());
        iop.set_p(START + 2);
        run(&mut iop, 1);
        assert!(iop.interrupt_enable() && !iop.interrupt_enable_delayed());
    }
    // a branch that is taken, a return jump and an EXIT hold it off too
    {
        let mut iop = iop(&[
            ins(0o003, 0),
            ins(0o072, 2),
            ins(0, 0),
            ins(0o070, 1),
            ins(0o001, 0),
        ]);
        run(&mut iop, 4);
        assert_eq!(iop.p(), START + 2);
        assert!(!iop.interrupt_enable());
        run(&mut iop, 1);
        assert!(iop.interrupt_enable());
    }
    // everything else lets it through
    for parcel in [
        ins(0, 0),
        ins(0o004, 1),
        ins(0o010, 1),
        ins(0o024, 1),
        ins(0o034, 1),
        ins(0o054, 0),
    ] {
        let mut iop = iop(&[ins(0o003, 0), parcel]);
        run(&mut iop, 2);
        assert!(iop.interrupt_enable(), "{parcel:06o}");
    }
}

/// 002 after 003 wins [HW 6-6], and 002 clears a flag that is set.
#[test]
fn interrupt_disable() {
    let mut iop = iop(&[ins(0o003, 0), ins(0o002, 0), ins(0, 0), ins(0, 0)]);
    let mut devices = Devices::requesting(6);
    for _ in 0..4 {
        iop.step(&mut devices);
    }
    assert!(!iop.interrupt_enable() && !iop.interrupt_enable_delayed());
    assert_eq!((iop.p(), iop.counters().interrupts), (START + 4, 0));

    let mut iop = super::tests::iop(&[ins(0o003, 0), ins(0, 0), ins(0o002, 0), ins(0, 0)]);
    run(&mut iop, 2);
    assert!(iop.interrupt_enable());
    run(&mut iop, 1);
    assert!(!iop.interrupt_enable());
}

/// A redundant 003 followed by an interrupt before the next instruction
/// that is not a branch: the handler is entered with interrupts disabled
/// and they come back on at its first such instruction, which is why a
/// handler begins with 002 [HW 5-19].
#[test]
fn redundant_enable_survives_into_the_handler() {
    const HANDLER: u16 = 0x22F7;
    // interrupts on, then 003 again and a branch; the device requests
    // while the branch executes
    let program = [
        ins(0o003, 0),
        ins(0, 0),
        ins(0o003, 0),
        ins(0o070, 1),
        ins(0, 0),
    ];
    for first in [ins(0o002, 0), ins(0, 0)] {
        let mut iop = iop(&program);
        set_exit_stack(&mut iop, 0, HANDLER);
        iop.memory_mut()[HANDLER as usize] = first;
        let mut devices = Devices::new();
        for _ in 0..3 {
            iop.step(&mut devices);
        }
        devices.done[6] = true;
        devices.enable[6] = true;
        iop.step(&mut devices);
        assert_eq!((iop.p(), iop.e()), (HANDLER, 1));
        assert!(!iop.interrupt_enable() && iop.interrupt_enable_delayed());
        devices.done[6] = false;
        iop.step(&mut devices);
        assert_eq!(iop.interrupt_enable(), first == 0);
        assert!(!iop.interrupt_enable_delayed());
    }
}

// ---- the program fetch request

/// 074 to 077 and the taken forms of 120 to 137 set the flag when operand
/// register d is zero, and the register of channel 1 takes d
/// [HW 3-11, 5-9].
#[test]
fn program_fetch_request_flag() {
    for f in [0o074, 0o075, 0o076, 0o077, 0o122, 0o126, 0o132, 0o136] {
        for value in [0, 0x4000] {
            let mut iop = iop(&[ins(f, 0o345), 0x0100]);
            iop.set_operand(0o345, value);
            iop.set_a(0);
            run(&mut iop, 1);
            assert_eq!(iop.done(channel::PFR), value == 0, "{f:o}");
            assert_eq!(iop.pfr_register(), if value == 0 { 0o345 } else { 0 });
            // the jump is made all the same
            let k = if f & 1 != 0 || f == 0o126 || f == 0o136 {
                0x0100
            } else {
                0
            };
            assert_eq!(iop.p(), value + k, "{f:o}");
        }
    }
    // not when the branch is not taken (spec Q7), nor by relative branches
    for f in [0o123, 0o127, 0o133, 0o137, 0o070, 0o072] {
        let mut iop = iop(&[ins(f, 0o345), 0x0100]);
        iop.set_a(0);
        run(&mut iop, 1);
        assert!(!iop.done(channel::PFR), "{f:o}");
    }
}

/// The channel 1 functions [HW 5-9, 5-10]: `PFR : 10` reads the register
/// and clears the flag (cray-sim leaves it set); `PFR : 0` clears it.
#[test]
fn program_fetch_request_channel() {
    let mut iop = iop(&[
        ins(0o074, 0o345),
        ins(0o041, 1),
        ins(0o040, 1),
        ins(0o150, 1),
    ]);
    iop.set_p(START);
    run(&mut iop, 1);
    iop.set_p(START + 1);
    run(&mut iop, 1);
    assert!(!iop.c());
    run(&mut iop, 1);
    assert!(iop.c());
    run(&mut iop, 1);
    assert_eq!((iop.a(), iop.c()), (0o345, false));
    assert!(!iop.done(channel::PFR));
    assert_eq!(iop.pfr_register(), 0o345);
    assert_eq!(iop.counters().pfr_channel, 3);

    iop.memory_mut()[SCRATCH as usize] = ins(0o074, 0o111);
    iop.set_p(SCRATCH);
    run(&mut iop, 1);
    assert!(iop.done(channel::PFR));
    exec(&mut iop, &[ins(0o140, 1)]);
    assert!(!iop.done(channel::PFR));
    assert_eq!(iop.pfr_register(), 0o111);
}

/// With channel 1 enabled the branch completes first: a return jump stores
/// its return address, then the interrupt stores the new P (spec Q8).
#[test]
fn program_fetch_request_interrupt() {
    let mut iop = iop(&[
        ins(0o147, 1),
        ins(0o003, 0),
        ins(0, 0),
        ins(0o077, 0o345),
        0x0200,
    ]);
    set_exit_stack(&mut iop, 0, 0x22F7);
    run(&mut iop, 4);
    assert_eq!((iop.p(), iop.e()), (0x0200, 1));
    assert_eq!(iop.interrupt_request(&mut NoChannels), Some(1));
    run(&mut iop, 1);
    assert_eq!((iop.p(), iop.e()), (0x22F7, 2));
    assert_eq!(iop.exit_stack()[1..3], [START + 5, 0x0200]);
    exec(&mut iop, &[ins(0o146, 1)]);
    assert_eq!(iop.interrupt_request(&mut NoChannels), None);
    assert!(iop.done(channel::PFR));
}

// ---- channels

/// Channel 0 is always Done and never Busy, which programs use to set and
/// clear the carry [HW 5-9]; channels 1 to 4 have no Busy flag.
#[test]
fn flags_of_the_channels_inside_the_processor() {
    let mut iop = iop(&[]);
    iop.set_b(0);
    exec(&mut iop, &[ins(0o040, 0)]);
    assert!(iop.c());
    exec(&mut iop, &[ins(0o041, 0)]);
    assert!(!iop.c());
    exec(&mut iop, &[ins(0o042, 0o777)]);
    assert!(iop.c());
    exec(&mut iop, &[ins(0o043, 0o777)]);
    assert!(!iop.c());
    for n in 1..5 {
        for f in [0o040, 0o041] {
            iop.set_c(true);
            exec(&mut iop, &[ins(f, n)]);
            assert!(!iop.c());
        }
    }
    // A is not touched by a flag test
    iop.set_a(0x4321);
    exec(&mut iop, &[ins(0o040, 0)]);
    assert_eq!(iop.a(), 0x4321);
}

/// 040 to 043 on channels 5 to 47 ask the devices; channel numbers above
/// 47 octal have nothing behind them (spec Q9).
#[test]
fn flags_of_the_devices() {
    let mut iop = iop(&[]);
    let mut devices = Devices::new();
    devices.done[5] = true;
    devices.busy[0o47] = true;
    let mut test = |f: u16, d: u16, b: u16| {
        iop.memory_mut()[SCRATCH as usize] = ins(f, d);
        iop.set_p(SCRATCH);
        iop.set_b(b);
        iop.set_c(f & 1 == 0 && !(d == 5 || b == 5));
        iop.step(&mut devices);
        iop.c()
    };
    assert!(test(0o040, 5, 0));
    assert!(!test(0o041, 5, 0));
    assert!(!test(0o040, 0o47, 0));
    assert!(test(0o041, 0o47, 0));
    assert!(test(0o042, 0, 5));
    assert!(!test(0o043, 0, 5));
    assert!(!test(0o042, 5, 0o47));
    assert!(test(0o043, 5, 0o47));
    for n in [0o50, 0o51, 0o105, 0o447, 0o777] {
        assert!(!test(0o040, n, 0));
        assert!(!test(0o041, n, 0));
        assert!(!test(0o042, 0, n));
        assert!(!test(0o043, 0, n));
    }
}

/// 140 to 177: the function is f bits 3 to 0, the channel d or B, and the
/// accumulator goes along.  Functions 10 to 13 load A and clear the carry
/// (spec Q10); the others leave both [HW 6-69].
#[test]
fn functions_of_the_devices() {
    let mut iop = iop(&[]);
    let mut devices = Devices::new();
    devices.input = 0xCAFE;
    for by_b in [false, true] {
        for function in 0..0o20u16 {
            let n = 0o20 + function;
            let f = if by_b { 0o160 } else { 0o140 } + function;
            iop.memory_mut()[SCRATCH as usize] = ins(f, if by_b { 0o777 } else { n });
            iop.set_p(SCRATCH);
            iop.set_b(if by_b { n } else { 0o777 });
            iop.set_a(0x1200 + function);
            iop.set_c(true);
            iop.step(&mut devices);
            assert_eq!(
                devices.functions.pop(),
                Some((n as u8, function as u8, 0x1200 + function))
            );
            if (0o10..=0o13).contains(&function) {
                assert_eq!((iop.a(), iop.c()), (0xCAFE, false), "{f:o}");
            } else {
                assert_eq!((iop.a(), iop.c()), (0x1200 + function, true), "{f:o}");
            }
            assert_eq!(iop.p(), SCRATCH + 1);
        }
    }
    // nothing reaches the devices for channels 0 to 4 or above 47 octal,
    // and a read there gives zero
    for n in [0, 1, 2, 3, 4, 0o50, 0o400, 0o777] {
        for function in [0, 6, 7, 0o12, 0o14, 0o17] {
            iop.memory_mut()[SCRATCH as usize] = ins(0o160 + function, 0);
            iop.set_p(SCRATCH);
            iop.set_b(n);
            iop.set_a(0x00FF);
            iop.set_c(true);
            iop.step(&mut devices);
            if function == 0o12 {
                assert_eq!((iop.a(), iop.c()), (0, false));
            } else if n != 2 {
                // (PXS : 14 has just set E from A)
                assert_eq!((iop.a(), iop.c()), (0x00FF, true));
            }
        }
    }
    assert!(devices.functions.is_empty());
}

/// A device can move data in Local Memory during a function.
#[test]
fn devices_reach_local_memory() {
    struct Mover;
    impl Channels for Mover {
        fn function(&mut self, mem: &mut [u16], _: u8, _: u8, a: u16) -> Option<u16> {
            assert_eq!(mem.len(), crate::MEMORY_PARCELS);
            mem[a as usize] = 0xD00D;
            None
        }
        fn busy(&mut self, _: u8) -> bool {
            false
        }
        fn done(&mut self, _: u8) -> bool {
            false
        }
        fn interrupt(&mut self) -> Option<u8> {
            None
        }
    }
    let mut iop = iop(&[ins(0o014, 0), 0x3456, ins(0o144, 5), ins(0o150, 5)]);
    iop.step(&mut Mover);
    iop.step(&mut Mover);
    assert_eq!(iop.memory()[0x3456], 0xD00D);
    // a function that reads and gets no answer loads zero
    iop.step(&mut Mover);
    assert_eq!(iop.a(), 0);
}

/// Channel 3 never reports an error; its Interrupt Enable can be set
/// [HW 5-12].
#[test]
fn local_memory_error_channel() {
    let mut iop = iop(&[]);
    exec(&mut iop, &[ins(0o147, 3)]);
    assert!(iop.enabled(channel::LME) && !iop.done(channel::LME));
    assert_eq!(iop.interrupt_request(&mut NoChannels), None);
    iop.set_a(0xFFFF);
    exec(&mut iop, &[ins(0o150, 3)]);
    assert_eq!(iop.a(), 0);
    exec(&mut iop, &[ins(0o140, 3)]);
    exec(&mut iop, &[ins(0o146, 3)]);
    assert!(!iop.enabled(channel::LME));
}

// ---- the real-time clock

/// Done sets once every 80,000 clock periods, whether they pass in
/// `advance` or in `step` [HW 5-12].
#[test]
fn real_time_clock_period() {
    let mut iop = Iop::new();
    iop.advance(RTC_PERIOD - 1);
    assert!(!iop.done(channel::RTC));
    assert_eq!(iop.rtc(), 0o234177);
    iop.advance(1);
    assert!(iop.done(channel::RTC));
    assert_eq!(iop.rtc(), 0);

    // by instructions: a loop of two (1 + 5 clock periods a turn)
    let mut iop = super::tests::iop(&[ins(0, 0), ins(0o071, 1), ins(0o140, 4)]);
    let mut time = 0u64;
    let mut dones = Vec::new();
    while dones.len() < 5 {
        time += iop.step(&mut NoChannels) as u64;
        assert_eq!(iop.rtc() as u64, time % RTC_PERIOD as u64);
        if iop.done(channel::RTC) {
            dones.push(time / RTC_PERIOD as u64);
            // RTC : 0 clears Done and nothing else
            let p = iop.p();
            iop.set_p(START + 2);
            time += iop.step(&mut NoChannels) as u64;
            iop.set_p(p);
            assert!(!iop.done(channel::RTC));
        }
    }
    assert_eq!(dones, [1, 2, 3, 4, 5]);

    // a long idle time sets Done once and keeps the phase
    let mut iop = Iop::new();
    iop.advance(3 * RTC_PERIOD + 17);
    assert!(iop.done(channel::RTC));
    assert_eq!(iop.rtc(), 17);
    iop.advance(u32::MAX);
    assert_eq!(iop.rtc() as u64, (17 + u32::MAX as u64) % RTC_PERIOD as u64);
}

/// `RTC : 10` reads the upper 16 of the 17 bits: 0 to 116077 octal
/// [HW 5-12].  The counter cannot be set.
#[test]
fn real_time_clock_read() {
    let mut iop = iop(&[]);
    iop.advance(12_345);
    iop.set_c(true);
    exec(&mut iop, &[ins(0o150, 4)]);
    assert_eq!((iop.a(), iop.c()), (12_345 / 2, false));
    let mut iop = super::tests::iop(&[]);
    iop.advance(RTC_PERIOD - 1);
    exec(&mut iop, &[ins(0o150, 4)]);
    assert_eq!(iop.a(), 0o116077);
    // the functions that write do not reach the counter
    let before = iop.rtc();
    iop.set_a(0x1111);
    let clock_periods = exec(&mut iop, &[ins(0o154, 4)]);
    assert_eq!(iop.rtc(), before + clock_periods);
}

/// With channel 4 enabled the clock interrupts once a millisecond, and the
/// handler's `RTC : 0` takes the request away.
#[test]
fn real_time_clock_interrupt() {
    const HANDLER: u16 = 0x22F7;
    // RTC : 7, I = 1, then an idle loop
    let mut iop = iop(&[ins(0o147, 4), ins(0o003, 0), ins(0, 0), ins(0o071, 1)]);
    set_exit_stack(&mut iop, 0, HANDLER);
    // the handler: I = 0, PASS, RTC : 0, I = 1, EXIT
    let handler = [
        ins(0o002, 0),
        ins(0, 0),
        ins(0o140, 4),
        ins(0o003, 0),
        ins(0o001, 0),
    ];
    iop.memory_mut()[HANDLER as usize..][..5].copy_from_slice(&handler);
    let mut time = 0u64;
    let mut entries = Vec::new();
    while time < 10 * RTC_PERIOD as u64 + 1000 {
        let interrupts = iop.counters().interrupts;
        time += iop.step(&mut NoChannels) as u64;
        if iop.counters().interrupts != interrupts {
            assert_eq!(iop.p(), HANDLER);
            entries.push(time / RTC_PERIOD as u64);
        }
    }
    assert_eq!(entries, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(iop.e(), 0);
}

// ---- Master Clear and dead start

/// Master Clear (spec part 2.3) and the dead start interrupt [HW 5-11].
#[test]
fn master_clear_and_dead_start() {
    // run something that leaves state behind
    let mut iop = iop(&[
        ins(0o147, 1),
        ins(0o147, 2),
        ins(0o147, 3),
        ins(0o147, 4),
        ins(0o076, 0),
        ins(0o003, 0),
    ]);
    iop.memory_mut()[0] = ins(0o072, 0o100);
    iop.memory_mut()[0o100] = ins(0o001, 0);
    for d in 0..512 {
        iop.set_operand(d, d as u16);
    }
    set_e(&mut iop, 12);
    run(&mut iop, 7);
    assert_eq!((iop.p(), iop.e()), (1, 13));
    iop.set_p(START + 5);
    run(&mut iop, 1);
    iop.advance(RTC_PERIOD + 5);
    iop.set_a(0x1234);
    iop.set_b(0o765);
    iop.set_c(true);
    assert!(iop.done(1) && iop.done(4) && iop.interrupt_enable_delayed());
    let rtc = iop.rtc();

    iop.master_clear();
    assert!(iop.held());
    assert_eq!((iop.p(), iop.e()), (0, 0));
    assert_eq!(iop.exit_stack(), &[0; 16]);
    assert!(iop.interrupt_enable() && !iop.interrupt_enable_delayed());
    for n in 1..5 {
        assert!(!iop.done(n) && !iop.enabled(n), "channel {n}");
    }
    // A, C, B, the operand registers, memory and the clock's count stay
    assert_eq!((iop.a(), iop.c(), iop.b()), (0x1234, true, 0o765));
    assert!((1..512).all(|d| iop.operand(d) == d as u16));
    assert_eq!(iop.memory()[0], ins(0o072, 0o100));
    assert_eq!(iop.rtc(), rtc);

    // held: steps execute nothing, one clock period each
    let instructions = iop.counters().instructions;
    for _ in 0..10 {
        assert_eq!(iop.step(&mut NoChannels), 1);
    }
    assert_eq!((iop.p(), iop.held()), (0, true));
    assert_eq!(iop.counters().instructions, instructions);
    assert_eq!(iop.rtc(), rtc + 10);

    // the dead start: the system loads Local Memory and channel 5
    // interrupts
    iop.memory_mut()[0] = ins(0o070, 3);
    let mut devices = Devices::requesting(5);
    iop.step(&mut devices);
    assert!(!iop.held() && !iop.interrupt_enable());
    assert_eq!((iop.p(), iop.e()), (0, 1));
    assert_eq!(iop.exit_stack(), &[0; 16]);
    assert_eq!(iop.counters().instructions, instructions);
    iop.step(&mut devices);
    assert_eq!(iop.p(), 3);
    assert_eq!(iop.counters().instructions, instructions + 1);
    // IOR : 10 names the channel until its flags are cleared
    iop.memory_mut()[3] = ins(0o150, 0);
    iop.step(&mut devices);
    assert_eq!(iop.a(), 5);
}

/// A new processor is held, and all of its state is zero.
#[test]
fn power_up() {
    let iop = Iop::new();
    assert!(iop.held() && iop.interrupt_enable());
    assert_eq!(
        (iop.p(), iop.a(), iop.c(), iop.b(), iop.e()),
        (0, 0, false, 0, 0)
    );
    assert!(iop.operands().iter().all(|&r| r == 0));
    assert!(iop.memory().iter().all(|&m| m == 0));
    assert_eq!(iop.memory().len(), 65_536);
    assert_eq!(iop.counters(), Default::default());
}

// ---- memory, instruction layout and time

/// k is the parcel after the instruction, and memory addresses are modulo
/// 2**16.
#[test]
fn two_parcel_instructions_at_the_end_of_memory() {
    let mut iop = Iop::new();
    iop.memory_mut()[0xFFFF] = ins(0o014, 0);
    iop.memory_mut()[0] = 0x8765;
    iop.memory_mut()[1] = ins(0o010, 7);
    iop.start_at(0xFFFF);
    run(&mut iop, 1);
    assert_eq!((iop.a(), iop.p()), (0x8765, 1));
    run(&mut iop, 1);
    assert_eq!((iop.a(), iop.p()), (7, 2));
}

/// Instructions that do not use d ignore it.
#[test]
fn d_is_ignored_where_it_is_not_used() {
    let mut iop = iop(&[
        ins(0, 0o777),
        ins(0o002, 0o777),
        ins(0o003, 0o777),
        ins(0o050, 0o777),
    ]);
    iop.set_b(0o123);
    run(&mut iop, 4);
    assert_eq!((iop.p(), iop.a()), (START + 4, 0o123));
    let mut iop = super::tests::iop(&[ins(0o072, 2), 0, ins(0o001, 0o777)]);
    run(&mut iop, 2);
    assert_eq!(iop.p(), START + 1);
}

/// A store into the parcel that is executed next takes effect: the
/// instruction stack is not modelled (spec Q13).
#[test]
fn a_store_ahead_of_p_is_executed() {
    let mut iop = iop(&[ins(0o034, 1), ins(0o010, 1)]);
    iop.set_operand(1, START + 1);
    iop.set_a(ins(0o010, 99));
    run(&mut iop, 2);
    assert_eq!(iop.a(), 99);
}

/// The clock periods `step` returns (spec parts 4.3 and 8.1).
#[test]
fn clock_periods() {
    let cases = [
        (vec![ins(0, 0)], 1),
        (vec![ins(0o002, 0)], 1),
        (vec![ins(0o003, 0)], 2),
        (vec![ins(0o006, 17)], 3),
        (vec![ins(0o010, 1)], 1),
        (vec![ins(0o013, 1)], 3),
        (vec![ins(0o014, 0), 0], 2),
        (vec![ins(0o017, 0), 0], 4),
        (vec![ins(0o020, 1)], 2),
        (vec![ins(0o023, 1)], 4),
        (vec![ins(0o024, 1)], 2),
        (vec![ins(0o027, 1)], 5),
        (vec![ins(0o030, 1)], 7),
        (vec![ins(0o033, 1)], 9),
        (vec![ins(0o034, 1)], 6),
        (vec![ins(0o037, 1)], 13),
        (vec![ins(0o040, 0)], 5),
        (vec![ins(0o047, 0)], 3),
        (vec![ins(0o050, 0)], 1),
        (vec![ins(0o054, 0)], 3),
        (vec![ins(0o057, 0)], 4),
        (vec![ins(0o060, 0)], 2),
        (vec![ins(0o067, 0)], 5),
        // a channel function that sends, and one that reads
        (vec![ins(0o154, 2)], 1),
        (vec![ins(0o150, 2)], 5),
        (vec![ins(0o173, 0)], 5),
    ];
    for (parcels, clock_periods) in cases {
        let mut iop = iop(&[]);
        iop.set_operand(1, 0x2000);
        assert_eq!(
            exec(&mut iop, &parcels),
            clock_periods,
            "{:06o}",
            parcels[0]
        );
    }
    // an EXIT
    let mut exit = iop(&[ins(0o001, 0)]);
    assert_eq!(exit.step(&mut NoChannels), 7);
    // branches: 5 inside the instruction stack (9 parcels forward, 11
    // back), 9 when it is refilled; not taken, 1, or 2 with a k to skip
    let branches = [
        (ins(0o070, 9), 5),
        (ins(0o070, 10), 9),
        (ins(0o071, 11), 5),
        (ins(0o071, 12), 9),
        (ins(0o072, 9), 5),
        (ins(0o073, 12), 9),
        (ins(0o074, 1), 9),
        (ins(0o077, 1), 9),
        (ins(0o102, 3), 5),
        (ins(0o103, 3), 1),
        (ins(0o126, 1), 9),
        (ins(0o127, 1), 2),
    ];
    for (parcel, clock_periods) in branches {
        let mut iop = iop(&[parcel, 0]);
        iop.set_a(0);
        assert_eq!(iop.step(&mut NoChannels), clock_periods, "{parcel:06o}");
    }
}

// ---- the disassembler

#[test]
fn disassembly() {
    let cases = [
        // parcels from the kernel image, with their addresses
        (0x7003, 0, "P = P + 3"),               // 0000
        (0x7C34, 0, "R = OR[52]"),              // 0001
        (0x0007, 0, "PASS (7)"),                // 0002
        (0x1000, 0, "A = 0"),                   // 0003
        (0x2800, 0, "OR[0] = A"),               // 0004
        (0x7A00, 0x41E4, "P = OR[0] + 0x41E4"), // 0005
        (0x3801, 0, "(OR[1]) = A"),             // 41E7
        (0x5800, 0, "B = A"),                   // 41E9
        (0x8405, 0, "P = P + 5, A = 0"),        // 41EB
        (0x6800, 0, "(B) = A"),                 // 41ED
        (0x5C00, 0, "B = B + 1"),               // 41EE
        (0xD802, 0, "PXS : 14"),                // 41F1
        (0x0C11, 0, "A = A >> 17"),             // 41F2
        (0xD002, 0, "PXS : 10"),                // 41F5
        (0x1800, 0x22F7, "A = 0x22F7"),         // 4203
        (0x1C00, 0x42B3, "A = A + 0x42B3"),     // 4222
        (0x4005, 0, "C = 1, MOS = DN"),         // 423E
        (0x8202, 0, "P = P + 2, C # 0"),        // 423F
        (0x4205, 0, "C = 1, MOS = BZ"),         // 4241
        (0x1628, 0, "A = A - 40"),              // 4249
        (0xEC00, 0, "IOB : 6"),                 // 424B
        (0x7463, 0, "R = P + 99"),              // 4254
        (0x7E00, 0x00A8, "R = OR[0] + 0x00A8"), // 4259
        (0x4800, 0, "A = A > B"),               // 4261
        (0x2602, 0, "A = A - OR[2]"),           // 4265
        (0x0400, 0, "I = 0"),                   // 22F7
        (0x0810, 0, "A = A > 16"),              // 22FA
        (0xB434, 0, "R = OR[52], A = 0"),       // 2303
        (0xD000, 0, "IOR : 10"),                // 230C
        (0xA630, 0, "P = OR[48], A # 0"),       // 2326
        (0x0A10, 0, "A = A < 16"),              // 2335
        (0x0600, 0, "I = 1"),                   // 2337
        (0x0200, 0, "EXIT"),                    // 2338
        (0x4600, 0, "C = 1, IOB = BZ"),         // 22D7
        (0x4400, 0, "C = 1, IOB = DN"),         // 22DC
        (0xF800, 0, "IOB : 14"),                // 22DA
        (0x2C76, 0, "OR[118] = OR[118] + 1"),   // 22D6
        (0x2FF6, 0, "OR[502] = OR[502] - 1"),   // 3F91
        (0x8E03, 0, "P = P - 3, A # 0"),        // 3F92
        (0x7707, 0, "R = P - 263"),             // 4A0B
        (0xCA0F, 0, "IO17 : 5"),                // 4879
        (0xD20F, 0, "IO17 : 11"),               // 488A
        (0x420F, 0, "C = 1, IO17 = BZ"),        // 48B5
        (0x1E00, 0x0800, "A = A - 0x0800"),     // 487E
        // the rest of the table
        (ins(0o007, 3), 0, "A = A << 3"),
        (ins(0o011, 255), 0, "A = A & 255"),
        (ins(0o012, 1), 0, "A = A + 1"),
        (ins(0o015, 0), 0xFF00, "A = A & 0xFF00"),
        (ins(0o021, 9), 0, "A = A & OR[9]"),
        (ins(0o025, 9), 0, "OR[9] = A + OR[9]"),
        (ins(0o030, 9), 0, "A = (OR[9])"),
        (ins(0o035, 9), 0, "(OR[9]) = A + (OR[9])"),
        (ins(0o036, 9), 0, "(OR[9]) = (OR[9]) + 1"),
        (ins(0o045, 0), 0, "A = A < B"),
        (ins(0o046, 0), 0, "A = A >> B"),
        (ins(0o047, 0), 0, "A = A << B"),
        (ins(0o050, 0), 0, "A = B"),
        (ins(0o053, 0), 0, "A = A - B"),
        (ins(0o055, 0), 0, "B = A + B"),
        (ins(0o057, 0), 0, "B = B - 1"),
        (ins(0o060, 0), 0, "A = (B)"),
        (ins(0o065, 0), 0, "(B) = A + (B)"),
        (ins(0o067, 0), 0, "(B) = (B) - 1"),
        (ins(0o071, 5), 0, "P = P - 5"),
        (ins(0o074, 5), 0, "P = OR[5]"),
        (ins(0o104, 5), 0, "P = P - 5, C = 0"),
        (ins(0o111, 5), 0, "R = P + 5, C # 0"),
        (ins(0o137, 5), 0x1234, "R = OR[5] + 0x1234, A # 0"),
        (ins(0o040, 4), 0, "C = 1, RTC = DN"),
        (ins(0o147, 3), 0, "LME : 7"),
        (ins(0o150, 1), 0, "PFR : 10"),
        (ins(0o157, 0o47), 0, "IO47 : 17"),
        (ins(0o177, 0), 0, "IOB : 17"),
    ];
    for (parcel, k, text) in cases {
        assert_eq!(disassemble(parcel, k), text, "{parcel:06o}");
    }
}

#[test]
fn instruction_lengths() {
    for f in 0..0o200u16 {
        let two = matches!(f, 0o014..=0o017 | 0o075 | 0o077 | 0o124..=0o127 | 0o134..=0o137);
        assert_eq!(parcels(ins(f, 0o777)), if two { 2 } else { 1 }, "{f:o}");
        // the processor agrees: P after an instruction that does not branch
        if !(0o070..0o100).contains(&f) && f != 0o001 {
            let mut iop = iop(&[ins(f, 3), 0]);
            // conditions false: C = 0 fails with C set, A = 0 with A not 0
            let (a, c) = match f & 3 {
                0 => (0, true),
                1 => (0, false),
                2 => (1, false),
                _ => (0, false),
            };
            if (0o100..0o140).contains(&f) {
                iop.set_a(a);
                iop.set_c(c);
            }
            run(&mut iop, 1);
            assert_eq!(iop.p(), START + parcels(ins(f, 3)), "{f:o}");
        }
    }
}

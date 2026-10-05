//! The I/O Processor: registers, Local Memory, the instruction set, the
//! program exit stack, interrupts, Master Clear and the channels that are
//! part of the processor (0 IOR, 1 PFR, 2 PXS, 3 LME, 4 RTC).
//!
//! Page numbers `[HW n-m]` are printed pages of HR-0030 rev B, the I/O
//! Subsystem Model B Hardware Reference Manual.  "Spec" is
//! `research/notes/iop-cpu-spec.md` in the core's repository; its part 11
//! lists the questions the manual leaves open, cited here as "spec Q1" and
//! so on.

/// Parcels of Local Memory: a 16-bit address selects one 16-bit parcel
/// [HW 2-1].
pub const MEMORY_PARCELS: usize = 1 << 16;
/// Operand registers, selected by the 9-bit d field or by B [HW 4-1].
pub const OPERAND_REGISTERS: usize = 512;
/// Locations of the program exit stack [HW 3-7].
pub const EXIT_STACK: usize = 16;
/// Clock periods from one setting of the real-time clock's Done flag to the
/// next: the counter runs from 0 to 234177 octal, 1 ms at 12.5 ns [HW 5-12].
pub const RTC_PERIOD: u32 = 80_000;
/// One more than the highest channel number: the Model B has 40 channels,
/// 0 to 47 octal [HW 5-1].
pub const CHANNELS: u16 = 0o50;

/// The channels inside the processor [HW 5-9 to 5-13].
pub mod channel {
    /// Interrupt request: reads the number of the requesting channel.
    pub const IOR: u16 = 0;
    /// Program fetch request.
    pub const PFR: u16 = 1;
    /// Program exit stack.
    pub const PXS: u16 = 2;
    /// Local Memory error.
    pub const LME: u16 = 3;
    /// Real-time clock.
    pub const RTC: u16 = 4;
    /// The first channel outside the processor (Buffer Memory).
    pub const FIRST_EXTERNAL: u16 = 5;
}

const D_MASK: u16 = 0o777;
const E_MASK: u8 = 0o17;

/// The channels outside the processor: 5 to 47 octal.
///
/// Each has a Busy flag, a Done flag and an Interrupt Enable flag, and
/// requests an interrupt while Done and Interrupt Enable are both set
/// [HW 5-2, 5-4, 7-2].  All three flags belong to the device side: functions
/// 6 and 7 clear and set Interrupt Enable on every channel but the
/// Peripheral Expander's [HW 7-1], so the processor passes them on like any
/// other function.
pub trait Channels {
    /// A function strobe with the accumulator [HW 5-3].  `mem` is the
    /// processor's Local Memory, for channels that move data by themselves.
    /// Returns the value for the accumulator when the function is one that
    /// reads (10 to 13 octal), else `None`.  `None` from a function that
    /// reads loads zero.
    fn function(&mut self, mem: &mut [u16], channel: u8, function: u8, a: u16) -> Option<u16>;
    /// The Busy flag of a channel, for 041 and 043.
    fn busy(&mut self, channel: u8) -> bool;
    /// The Done flag of a channel, for 040 and 042.
    fn done(&mut self, channel: u8) -> bool;
    /// The lowest numbered channel (5 to 47 octal) with Done and its
    /// Interrupt Enable both set.
    fn interrupt(&mut self) -> Option<u8>;
}

/// No devices: every channel from 5 up is empty.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoChannels;

impl Channels for NoChannels {
    fn function(&mut self, _: &mut [u16], _: u8, _: u8, _: u16) -> Option<u16> {
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

/// Counts of events since the processor was made, for traces and for the
/// things the specification wants counted during a boot (spec Q4, Q16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Instructions executed.
    pub instructions: u64,
    /// Interrupts taken, dead start interrupts included.
    pub interrupts: u64,
    /// Functions and flag tests addressed to channel 1 (PFR).
    pub pfr_channel: u64,
    /// Shifts with a count (d or B) above 17, where the manual and cray-sim
    /// part ways.
    pub long_shifts: u64,
    /// Times the Exit Stack Boundary flag was set.
    pub boundary_flags: u64,
}

/// Clock periods of the instructions 000 to 067: the clock period in which
/// HR-0030 delivers the result, counted from the issue in clock period 0
/// (the "CP" column of the spec, part 4.3), with two changes.  A PASS takes
/// the one clock period of its issue.  An EXIT has P in clock period 2
/// [HW 6-4] and, like every branch that leaves the instruction stack, needs
/// 5 more to bring the first instruction of the new sequence to issue (074
/// has P in clock period 4 and issues in 9 [HW 6-63]).
///
/// The real machine overlaps instructions and its memory can make a
/// reference wait, so these are estimates (spec part 8.1).
const CLOCK_PERIODS: [u8; 0o70] = [
    1, 7, 1, 2, 3, 3, 3, 3, // 000 to 007
    1, 1, 3, 3, 2, 2, 4, 4, // 010 to 017
    2, 2, 4, 4, 2, 5, 5, 5, // 020 to 027
    7, 7, 9, 9, 6, 13, 13, 13, // 030 to 037
    5, 5, 5, 5, 3, 3, 3, 3, // 040 to 047
    1, 1, 3, 3, 3, 4, 4, 4, // 050 to 057
    2, 2, 4, 4, 2, 5, 5, 5, // 060 to 067
];
/// A branch to an instruction that is in the instruction stack: "can issue
/// at CP 5" [HW 6-59].
const CP_BRANCH_IN_STACK: u32 = 5;
/// A branch that refills the instruction stack from memory: P in clock
/// period 4, issue in n + 4 with n at least 5 [HW 6-63].
const CP_BRANCH: u32 = 9;
/// A forward relative branch of at most 11 octal parcels, or a backward one
/// of at most 13 octal, stays in the instruction stack [HW 3-5].
const IN_STACK_FORWARD: u16 = 0o11;
const IN_STACK_BACKWARD: u16 = 0o13;
/// The interrupt sequence.  The manual gives no time; it is taken to cost
/// what a return jump out of the instruction stack costs.
const CP_INTERRUPT: u32 = CP_BRANCH;
/// A channel function that sends the accumulator, and one that reads into
/// it: "output from accumulator takes 1 CP", "input to accumulator takes 4-6
/// CP" (T0201D page 4.4).
const CP_FUNCTION_OUT: u32 = 1;
const CP_FUNCTION_IN: u32 = 5;

/// One I/O Processor.  See the crate documentation.
pub struct Iop {
    /// The accumulator.
    a: u16,
    /// The carry bit: bit 2**16 of the accumulator in add, subtract and
    /// shift [HW 4-4].
    c: bool,
    /// The B register, 9 bits.
    b: u16,
    p: u16,
    /// The exit stack pointer, 4 bits: it points at the newest entry.
    e: u8,
    xs: [u16; EXIT_STACK],
    or: [u16; OPERAND_REGISTERS],
    mem: Vec<u16>,
    /// The System Interrupt Enable flag.
    i: bool,
    /// 003 has been executed and the flag is not set yet.
    i_delayed: bool,
    /// Master Clear has been applied and no interrupt has been taken since.
    held: bool,
    /// Channel 1: the Program Fetch Request flag (its Done flag), the
    /// register with the d of the instruction that set it, Interrupt Enable.
    pfr: bool,
    pfr_register: u16,
    pfr_enable: bool,
    /// Channel 2: the Exit Stack Boundary flag (its Done flag) and
    /// Interrupt Enable.
    boundary: bool,
    pxs_enable: bool,
    /// Channel 3: Interrupt Enable.  The parity error flag never sets.
    lme_enable: bool,
    /// Channel 4: the counter (0 to `RTC_PERIOD` - 1), Done, Interrupt
    /// Enable.
    rtc: u32,
    rtc_done: bool,
    rtc_enable: bool,
    counters: Counters,
}

impl Default for Iop {
    fn default() -> Self {
        Iop::new()
    }
}

impl Iop {
    /// A processor at power-up: every register and all of Local Memory
    /// zero, and Master Clear applied.
    pub fn new() -> Iop {
        let mut iop = Iop {
            a: 0,
            c: false,
            b: 0,
            p: 0,
            e: 0,
            xs: [0; EXIT_STACK],
            or: [0; OPERAND_REGISTERS],
            mem: vec![0; MEMORY_PARCELS],
            i: false,
            i_delayed: false,
            held: false,
            pfr: false,
            pfr_register: 0,
            pfr_enable: false,
            boundary: false,
            pxs_enable: false,
            lme_enable: false,
            rtc: 0,
            rtc_done: false,
            rtc_enable: false,
            counters: Counters::default(),
        };
        iop.master_clear();
        iop
    }

    /// Master Clear.  The processor then executes nothing until it takes an
    /// interrupt; `step` only lets time pass.
    ///
    /// The manual says only that exit stack location 0 is cleared and that
    /// the dead start interrupt starts the program at 0 with E advanced by
    /// one [HW 5-11].  The rest is the choice of the spec, part 2.3 (Q11,
    /// Q12): P and E are 0, the whole exit stack is cleared, System
    /// Interrupt Enable is set so that the dead start interrupt is
    /// accepted, and the flags and Interrupt Enables of channels 1 to 4 are
    /// cleared.  A, the carry, B, the operand registers and Local Memory
    /// keep what they held: the kernel reads operand register 511 before it
    /// clears the registers.  The real-time clock cannot be set [HW 5-12]
    /// and goes on counting.
    pub fn master_clear(&mut self) {
        self.p = 0;
        self.e = 0;
        self.xs = [0; EXIT_STACK];
        self.i = true;
        self.i_delayed = false;
        self.held = true;
        self.pfr = false;
        self.pfr_register = 0;
        self.pfr_enable = false;
        self.boundary = false;
        self.pxs_enable = false;
        self.lme_enable = false;
        self.rtc_done = false;
        self.rtc_enable = false;
    }

    /// Leave the Master Clear state without a dead start and go on at `p`
    /// with System Interrupt Enable clear.  For tests; the real machine
    /// starts by the dead start interrupt (see `step`).
    pub fn start_at(&mut self, p: u16) {
        self.p = p;
        self.i = false;
        self.i_delayed = false;
        self.held = false;
    }

    // ---- state

    /// P: the address of the next instruction.
    pub fn p(&self) -> u16 {
        self.p
    }
    /// The accumulator.
    pub fn a(&self) -> u16 {
        self.a
    }
    /// The carry bit.
    pub fn c(&self) -> bool {
        self.c
    }
    /// The B register (9 bits).
    pub fn b(&self) -> u16 {
        self.b
    }
    /// E, the exit stack pointer (4 bits).
    pub fn e(&self) -> u8 {
        self.e
    }
    /// The program exit stack.
    pub fn exit_stack(&self) -> &[u16; EXIT_STACK] {
        &self.xs
    }
    /// Operand register `d`.
    pub fn operand(&self, d: usize) -> u16 {
        self.or[d]
    }
    /// The operand registers.
    pub fn operands(&self) -> &[u16; OPERAND_REGISTERS] {
        &self.or
    }
    /// The System Interrupt Enable flag.
    pub fn interrupt_enable(&self) -> bool {
        self.i
    }
    /// 003 has been executed and System Interrupt Enable is not set yet.
    pub fn interrupt_enable_delayed(&self) -> bool {
        self.i_delayed
    }
    /// Master Clear has been applied and no interrupt has been taken since.
    pub fn held(&self) -> bool {
        self.held
    }
    /// Local Memory.
    pub fn memory(&self) -> &[u16] {
        &self.mem
    }
    /// Local Memory, for the dead start load and for devices that move data
    /// outside a function strobe.
    pub fn memory_mut(&mut self) -> &mut [u16] {
        &mut self.mem
    }
    /// The real-time clock's counter: 0 to `RTC_PERIOD` - 1.
    pub fn rtc(&self) -> u32 {
        self.rtc
    }
    /// The register of channel 1: the d of the instruction that last set
    /// the Program Fetch Request flag.
    pub fn pfr_register(&self) -> u16 {
        self.pfr_register
    }
    /// The Done flag of a channel inside the processor (0 to 4): always set
    /// on channel 0, the Program Fetch Request flag on 1, the Exit Stack
    /// Boundary flag on 2, never set on 3, the real-time clock's on 4.
    pub fn done(&self, channel: u16) -> bool {
        match channel {
            channel::IOR => true,
            channel::PFR => self.pfr,
            channel::PXS => self.boundary,
            channel::RTC => self.rtc_done,
            _ => false,
        }
    }
    /// The Interrupt Enable flag of a channel inside the processor (1 to
    /// 4).  Channel 0 has none.
    pub fn enabled(&self, channel: u16) -> bool {
        match channel {
            channel::PFR => self.pfr_enable,
            channel::PXS => self.pxs_enable,
            channel::LME => self.lme_enable,
            channel::RTC => self.rtc_enable,
            _ => false,
        }
    }
    /// Event counts since the processor was made.
    pub fn counters(&self) -> Counters {
        self.counters
    }

    /// Set P.
    pub fn set_p(&mut self, p: u16) {
        self.p = p;
    }
    /// Set the accumulator.
    pub fn set_a(&mut self, a: u16) {
        self.a = a;
    }
    /// Set the carry bit.
    pub fn set_c(&mut self, c: bool) {
        self.c = c;
    }
    /// Set the B register from the low 9 bits of `b`.
    pub fn set_b(&mut self, b: u16) {
        self.b = b & D_MASK;
    }
    /// Set operand register `d`.
    pub fn set_operand(&mut self, d: usize, value: u16) {
        self.or[d] = value;
    }

    // ---- time and interrupts

    /// Let `clock_periods` pass without executing anything.  The real-time
    /// clock counts them: on reaching 234177 octal it sets its Done flag
    /// and starts again at 0 [HW 5-12].
    pub fn advance(&mut self, clock_periods: u32) {
        let count = self.rtc as u64 + clock_periods as u64;
        if count >= RTC_PERIOD as u64 {
            self.rtc_done = true;
        }
        self.rtc = (count % RTC_PERIOD as u64) as u32;
    }

    /// The number `IOR : 10` reads: the highest priority (lowest numbered)
    /// channel whose Done and Interrupt Enable flags are both set
    /// [HW 5-1, 5-9].  Channel 0 has no Interrupt Enable and never
    /// requests.
    pub fn interrupt_request(&self, channels: &mut dyn Channels) -> Option<u8> {
        if self.pfr && self.pfr_enable {
            Some(channel::PFR as u8)
        } else if self.boundary && self.pxs_enable {
            Some(channel::PXS as u8)
        } else if self.rtc_done && self.rtc_enable {
            Some(channel::RTC as u8)
        } else {
            channels.interrupt()
        }
    }

    /// Execute one instruction, or take one interrupt, and return the
    /// number of clock periods (12.5 ns) it took on the real machine.  The
    /// real-time clock has counted them when `step` returns.
    ///
    /// An interrupt is taken instead of the next instruction when a channel
    /// requests one and System Interrupt Enable is set [HW 5-19].  After
    /// Master Clear that is the only thing that happens: the dead start is
    /// the interrupt of channel 5 at the end of the load of Local Memory
    /// [HW 5-15], which starts the program at the address in exit stack
    /// location 0, and Master Clear made that 0.  Until then a step is one
    /// idle clock period.
    pub fn step(&mut self, channels: &mut dyn Channels) -> u32 {
        let clock_periods = self.execute(channels);
        self.advance(clock_periods);
        clock_periods
    }

    fn execute(&mut self, channels: &mut dyn Channels) -> u32 {
        if self.i && self.interrupt_request(channels).is_some() {
            self.interrupt();
            return CP_INTERRUPT;
        }
        if self.held {
            return 1;
        }
        let parcel = self.mem[self.p as usize];
        let f = parcel >> 9;
        let delayed = self.i_delayed;
        let clock_periods = self.instruction(channels, f, parcel & D_MASK);
        self.counters.instructions += 1;
        // 003 takes effect late: "the System Interrupt Enable flag is
        // delayed until the next nonbranch or non-I/O instruction is
        // issued.  Instructions 40 through 43 and 70 through 137 do not
        // enable interrupts" [HW 5-20], and "a jump or exit instruction
        // must be completed before interrupts are actually set" [HW 5-19].
        // Spec part 7.3 and Q6: the flag sets at the completion of the
        // first instruction after the 003 that is not 001, 003, 040 to 043,
        // 070 to 137 or 140 to 177; 002 cancels it.
        let holds_off = matches!(f, 0o001 | 0o003 | 0o040..=0o043 | 0o070..=0o177);
        if delayed && self.i_delayed && !holds_off {
            self.i = true;
            self.i_delayed = false;
        }
        clock_periods
    }

    /// The interrupt sequence [HW 5-19]: clear System Interrupt Enable,
    /// advance E, store the address of the next instruction of the
    /// interrupted program there, and go on at the address in exit stack
    /// location 0, which is read without E.  An interrupt that takes E to
    /// 14 does not set the Exit Stack Boundary flag [HW 3-10].  A delayed
    /// 003 stays pending and enables interrupts inside the handler, which
    /// is why a handler begins with 002 [HW 5-19].
    fn interrupt(&mut self) {
        self.i = false;
        self.e = (self.e + 1) & E_MASK;
        self.xs[self.e as usize] = self.p;
        self.p = self.xs[0];
        self.held = false;
        self.counters.interrupts += 1;
    }

    // ---- arithmetic

    /// The 17-bit value of carry and accumulator.
    fn ca(&self) -> u32 {
        (self.c as u32) << 16 | self.a as u32
    }
    fn set_ca(&mut self, ca: u32) {
        self.a = ca as u16;
        self.c = ca & 1 << 16 != 0;
    }
    /// A load clears the carry.
    fn load(&mut self, x: u16) {
        self.a = x;
        self.c = false;
    }
    /// A logical product clears the carry.
    fn and(&mut self, x: u16) {
        self.a &= x;
        self.c = false;
    }
    /// Add: the carry is complemented, not set, if a carry is propagated
    /// from the accumulator [HW 6-13].
    fn add(&mut self, x: u16) {
        self.set_ca(self.ca() + x as u32);
    }
    /// Subtract: add the complement, then add 1; the carry is complemented
    /// if either addition carries [HW 6-14], that is whenever the
    /// accumulator is not below the operand [HW 4-2].
    fn sub(&mut self, x: u16) {
        self.set_ca(self.ca() + !x as u32 + 1);
    }

    /// 004 to 007 and 044 to 047: shift carry and accumulator as one 17-bit
    /// register.  The low 5 bits of d or B are the count [HW 6-7]; cray-sim
    /// uses the whole value for end-off shifts (spec Q5).  An end-off shift
    /// of more than 16 places clears both.  A circular shift of 18 to 31
    /// places turns the 17 bits by the count modulo 17 (spec Q4; the manual
    /// does not say).
    fn shift(&mut self, f: u16, count: u16) {
        if count > 17 {
            self.counters.long_shifts += 1;
        }
        let n = (count & 0o37) as u32;
        let ca = self.ca();
        let r = n % 17;
        let ca = match f & 3 {
            0 if n > 16 => 0,
            0 => ca >> n,
            1 if n > 16 => 0,
            1 => ca << n,
            2 => ca >> r | ca << (17 - r),
            _ => ca << r | ca >> (17 - r),
        };
        self.set_ca(ca & 0x1_FFFF);
    }

    /// The eight operations that the groups 020, 030, 050 and 060 perform
    /// with their operand, selected by the low three bits of f: load,
    /// logical product, add, subtract, store, add and store, increment,
    /// decrement.  Returns the value that goes back to the operand, if any.
    ///
    /// Increment and decrement clear the carry, enter 1 or all ones in the
    /// accumulator and add the operand, so the result is also left in the
    /// accumulator, and after a decrement the carry is set whenever the
    /// operand was not zero [HW 6-25, 6-26].
    fn operate(&mut self, f: u16, x: u16) -> Option<u16> {
        match f & 7 {
            0 => self.load(x),
            1 => self.and(x),
            2 => self.add(x),
            3 => self.sub(x),
            4 => {}
            5 => self.add(x),
            6 => {
                self.load(1);
                self.add(x);
            }
            _ => {
                self.load(0xFFFF);
                self.add(x);
            }
        }
        (f & 4 != 0).then_some(self.a)
    }

    // ---- the program exit stack

    /// A return jump advances E and stores the return address at the new
    /// location.  The Exit Stack Boundary flag sets if the advanced E is 14
    /// [HW 6-65]; the manual says so for 076 and 077 only, and it is done
    /// for every return jump (spec Q3).  From 15, E wraps to 0 and the
    /// address of the interrupt handler is lost [HW 3-9].
    fn push_return(&mut self, address: u16) {
        self.e = (self.e + 1) & E_MASK;
        self.xs[self.e as usize] = address;
        if self.e == 14 {
            self.set_boundary();
        }
    }

    fn set_boundary(&mut self) {
        self.boundary = true;
        self.counters.boundary_flags += 1;
    }

    /// 001: P comes from the exit stack location E points at, then E is
    /// decremented.  If E was 0 the decrement is blocked and the Exit Stack
    /// Boundary flag sets [HW 6-4]; cray-sim lets E wrap to 15.  The
    /// program then goes on at the address in location 0, the interrupt
    /// handler, with E still 0 [HW 3-9, 3-10].  Nothing is pushed and
    /// System Interrupt Enable is left alone (spec Q1, Q2), so if the flag
    /// may interrupt, the ordinary interrupt sequence follows.
    fn exit(&mut self) {
        self.p = self.xs[self.e as usize];
        if self.e == 0 {
            self.set_boundary();
        } else {
            self.e -= 1;
        }
    }

    // ---- instructions

    /// Execute the instruction with operation code `f` and field `d` at P;
    /// returns its clock periods.
    fn instruction(&mut self, channels: &mut dyn Channels, f: u16, d: u16) -> u32 {
        let p = self.p;
        // the second parcel of a 2-parcel instruction
        let k = self.mem[p.wrapping_add(1) as usize];
        let mut next = p.wrapping_add(1);
        match f {
            // PASS; d is ignored (the kernel keeps halt codes there)
            0o000 => {}
            // EXIT
            0o001 => {
                self.exit();
                return CLOCK_PERIODS[f as usize] as u32;
            }
            // I = 0; it also cancels a 003 that has not taken effect
            // [HW 6-6]
            0o002 => {
                self.i = false;
                self.i_delayed = false;
            }
            // I = 1, delayed: see `execute`
            0o003 => self.i_delayed = true,
            0o004..=0o007 => self.shift(f, d),
            // A with d
            0o010..=0o013 => {
                self.operate(f & 3, d);
            }
            // A with k
            0o014..=0o017 => {
                self.operate(f & 3, k);
                next = p.wrapping_add(2);
            }
            // A with operand register d
            0o020..=0o027 => {
                let r = d as usize;
                if let Some(result) = self.operate(f, self.or[r]) {
                    self.or[r] = result;
                }
            }
            // A with the parcel of memory that operand register d addresses
            0o030..=0o037 => {
                let m = self.or[d as usize] as usize;
                if let Some(result) = self.operate(f, self.mem[m]) {
                    self.mem[m] = result;
                }
            }
            // C = Done or Busy of channel d (040, 041) or channel B (042,
            // 043)
            0o040..=0o043 => {
                let n = if f < 0o042 { d } else { self.b };
                self.c = self.flag(channels, n, f & 1 != 0);
            }
            0o044..=0o047 => self.shift(f, self.b),
            // A with B; B takes the low 9 bits of a result
            0o050..=0o057 => {
                if let Some(result) = self.operate(f, self.b) {
                    self.b = result & D_MASK;
                }
            }
            // A with operand register B
            0o060..=0o067 => {
                let r = self.b as usize;
                if let Some(result) = self.operate(f, self.or[r]) {
                    self.or[r] = result;
                }
            }
            0o070..=0o137 => return self.branch(f, d, k),
            // 140 to 157: function f bits 3 to 0 on channel d; 160 to 177:
            // on channel B
            _ => {
                let n = if f < 0o160 { d } else { self.b };
                self.p = next;
                return self.function(channels, n, f & 0o17);
            }
        }
        self.p = next;
        CLOCK_PERIODS[f as usize] as u32
    }

    /// 070 to 137.  The branch mode is f bits 2 to 0 of 070 to 077 and
    /// f bits 4 to 2 of 100 to 137: P + d, P - d, return jump to P + d,
    /// return jump to P - d, operand register d, operand register d + k,
    /// and the return jumps to the last two.  The condition of 100 to 137
    /// is f bits 1 to 0: C = 0, C # 0, A = 0, A # 0 [HW 6-67].  P in these
    /// sums is the address of the branch itself, d is a 9-bit positive
    /// number and all sums are modulo 2**16 [HW 6-59].  A and the carry do
    /// not change.
    fn branch(&mut self, f: u16, d: u16, k: u16) -> u32 {
        let p = self.p;
        let mode = if f < 0o100 { f & 7 } else { f >> 2 & 7 };
        let taken = f < 0o100
            || match f & 3 {
                0 => !self.c,
                1 => self.c,
                2 => self.a == 0,
                _ => self.a != 0,
            };
        let two_parcels = mode & 5 == 5;
        let next = p.wrapping_add(if two_parcels { 2 } else { 1 });
        if !taken {
            // "the next instruction in the current program sequence can
            // issue in the next CP" [HW 6-67]; k has to pass as well
            self.p = next;
            return if two_parcels { 2 } else { 1 };
        }
        let (target, in_stack) = match mode {
            0 | 2 => (p.wrapping_add(d), d <= IN_STACK_FORWARD),
            1 | 3 => (p.wrapping_sub(d), d <= IN_STACK_BACKWARD),
            _ => {
                // The Program Fetch Request flag sets when operand register
                // d holds zero, and the register of channel 1 takes d
                // [HW 3-11, 5-9].  Not when a conditional branch is not
                // taken (spec Q7).  The branch still completes; the
                // interrupt, if channel 1 may interrupt, comes after it
                // (spec Q8).
                let base = self.or[d as usize];
                if base == 0 {
                    self.pfr = true;
                    self.pfr_register = d;
                }
                (base.wrapping_add(if two_parcels { k } else { 0 }), false)
            }
        };
        if mode & 2 != 0 {
            self.push_return(next);
        }
        self.p = target;
        if in_stack {
            CP_BRANCH_IN_STACK
        } else {
            CP_BRANCH
        }
    }

    // ---- channels

    /// The Busy (`busy`) or Done flag of channel `n` for 040 to 043.  A
    /// number above 47 octal has no channel and reads as an empty one: both
    /// flags clear (spec Q9).
    fn flag(&mut self, channels: &mut dyn Channels, n: u16, busy: bool) -> bool {
        if n == channel::PFR {
            self.counters.pfr_channel += 1;
        }
        match n {
            // channels 0 to 4 have no Busy flag; channel 0 is always Done
            // [HW 5-9]
            0..=4 => !busy && self.done(n),
            5..CHANNELS if busy => channels.busy(n as u8),
            5..CHANNELS => channels.done(n as u8),
            _ => false,
        }
    }

    /// 140 to 177: send a function and the accumulator to channel `n`.
    /// Functions 10 to 13 replace the accumulator with what the channel
    /// returns [HW 6-69] and clear the carry (spec Q10); the others leave
    /// both alone.  The processor never waits for a channel.  A function
    /// that a channel does not have does nothing and reads zero, and so
    /// does every function of a channel number above 47 octal (spec Q9).
    fn function(&mut self, channels: &mut dyn Channels, n: u16, function: u16) -> u32 {
        let value = match n {
            channel::IOR => self.ior(channels, function),
            channel::PFR => self.pfr_function(function),
            channel::PXS => self.pxs(function),
            channel::LME => self.lme(function),
            channel::RTC => self.rtc_function(function),
            5..CHANNELS => channels
                .function(&mut self.mem, n as u8, function as u8, self.a)
                .unwrap_or(0),
            _ => 0,
        };
        if (0o10..=0o13).contains(&function) {
            self.load(value);
            CP_FUNCTION_IN
        } else {
            CP_FUNCTION_OUT
        }
    }

    /// Channel 0, interrupt request.  `IOR : 10` reads the number of the
    /// highest priority channel that requests an interrupt, zero if there
    /// is none, whether or not System Interrupt Enable is set [HW 5-9].
    fn ior(&mut self, channels: &mut dyn Channels, function: u16) -> u16 {
        match function {
            0o10 => self.interrupt_request(channels).map_or(0, u16::from),
            _ => 0,
        }
    }

    /// Channel 1, program fetch request [HW 5-9, 5-10].
    fn pfr_function(&mut self, function: u16) -> u16 {
        self.counters.pfr_channel += 1;
        match function {
            0 => self.pfr = false,
            6 => self.pfr_enable = false,
            7 => self.pfr_enable = true,
            // reading the register clears the flag as well [HW 5-10]; in
            // cray-sim it does not
            0o10 => {
                self.pfr = false;
                return self.pfr_register;
            }
            _ => {}
        }
        0
    }

    /// Channel 2, program exit stack [HW 5-10].  The functions act at once;
    /// the real machine needs 5 clock periods before the stack is used
    /// again, which programs spend in three circular shifts of 17 places
    /// [HW 5-11].
    fn pxs(&mut self, function: u16) -> u16 {
        match function {
            0 => self.boundary = false,
            6 => self.pxs_enable = false,
            7 => self.pxs_enable = true,
            0o10 => return self.e as u16,
            0o11 => return self.xs[self.e as usize],
            0o14 => self.e = self.a as u8 & E_MASK,
            0o15 => self.xs[self.e as usize] = self.a,
            _ => {}
        }
        0
    }

    /// Channel 3, Local Memory error [HW 5-12].  The memory of the model
    /// has no errors: the flag never sets and `LME : 10` reads zero.
    fn lme(&mut self, function: u16) -> u16 {
        match function {
            6 => self.lme_enable = false,
            7 => self.lme_enable = true,
            _ => {}
        }
        0
    }

    /// Channel 4, real-time clock [HW 5-12, 5-13].  `RTC : 10` reads the
    /// upper 16 of the counter's 17 bits: 0 to 116077 octal, in units of
    /// two clock periods.
    fn rtc_function(&mut self, function: u16) -> u16 {
        match function {
            0 => self.rtc_done = false,
            6 => self.rtc_enable = false,
            7 => self.rtc_enable = true,
            0o10 => return (self.rtc >> 1) as u16,
            _ => {}
        }
        0
    }
}

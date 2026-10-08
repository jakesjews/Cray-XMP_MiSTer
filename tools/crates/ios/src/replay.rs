//! A record of what an I/O Processor does, instruction by instruction, for
//! checking another implementation of the processor against this one: the
//! hardware description in `rtl/ios/iop_cpu.v`, run by `sim/harness/iop_main.cpp`.
//!
//! The other implementation is given the same Local Memory and, at every
//! step, what this one's channels answered: the channel that asks for an
//! interrupt, the value a function read, the flag a test found.  It must
//! then take the same interrupts, send the same functions and end every
//! step with the same registers, in the same number of clock periods.  What
//! a device stores in Local Memory by itself is in the record too.
//!
//! The real-time clock, channel 4, is one of the channels outside the
//! processor here: the hardware keeps it with the other channels.
//!
//! # Format
//!
//! Records of 20 bytes, numbers little-endian:
//!
//! ```text
//! byte 0      kind: 0 instruction, 1 interrupt, 2 a parcel of Local Memory to set,
//!             3 Master Clear, 4 a parcel of Local Memory to check, 5 an operand
//!             register to check, 6 an exit stack location to check, 7 the end,
//!             8 an operand register to set, 9 the accumulator (number), B (value
//!             bits 8 to 0) and the carry (value bit 15) to set, 10 a number of
//!             clocks in which no channel asks and no step may end
//! byte 1      the channel (4 to 47 octal) asking for an interrupt before the step, or 0
//! bytes 2-3   P before the step; for kinds 2, 4, 5 and 6 the address or number
//! bytes 4-5   the parcel at P; for kinds 2, 4, 5 and 6 the value
//! bytes 6-7   the channel number of a function or a flag test (9 bits)
//! bytes 8-9   the accumulator before the step
//! bytes 10-11 the value read by a function 10 to 13, or the flag found by 040 to 043
//! bytes 12-17 P, the accumulator and B after the step
//! byte 18     bits 3 to 0: E after the step; bits 7 to 4: the clock periods of
//!             the step (1 to 13)
//! byte 19     after the step: bit 0 carry, 1 System Interrupt Enable, 2 its delayed
//!             setting pending, 3 Program Fetch Request flag, 4 Exit Stack Boundary
//!             flag, 5 held by Master Clear
//! ```

use crate::iop::{channel, Channels, Iop, EXIT_STACK, MEMORY_PARCELS, OPERAND_REGISTERS};
use std::io::Write;

pub const RECORD_BYTES: usize = 20;

const INSTRUCTION: u8 = 0;
const INTERRUPT: u8 = 1;
const SET_MEMORY: u8 = 2;
const MASTER_CLEAR: u8 = 3;
const CHECK_MEMORY: u8 = 4;
const CHECK_OPERAND: u8 = 5;
const CHECK_EXIT_STACK: u8 = 6;
const END: u8 = 7;
const SET_OPERAND: u8 = 8;
const SET_REGISTERS: u8 = 9;
const IDLE: u8 = 10;

/// Writes the record of one processor.
pub struct Recorder {
    out: Box<dyn Write>,
    /// Local Memory as the reader of the record has it.
    shadow: Vec<u16>,
    steps: u64,
}

impl Recorder {
    /// A record that starts with the reader's processor in Master Clear and
    /// its Local Memory zero.
    pub fn new(out: Box<dyn Write>) -> Recorder {
        Recorder {
            out,
            shadow: vec![0; MEMORY_PARCELS],
            steps: 0,
        }
    }

    /// Steps recorded.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    fn short(&mut self, kind: u8, number: u16, value: u16) {
        let mut record = [0u8; RECORD_BYTES];
        record[0] = kind;
        record[2..4].copy_from_slice(&number.to_le_bytes());
        record[4..6].copy_from_slice(&value.to_le_bytes());
        let _ = self.out.write_all(&record);
    }

    /// The processor was master cleared, and nothing asks it for an
    /// interrupt for a while: it must do nothing.
    pub fn master_clear(&mut self) {
        self.short(MASTER_CLEAR, 0, 0);
        self.short(IDLE, 40, 0);
    }

    /// Tell the reader of every parcel a device has changed.
    pub fn sync(&mut self, iop: &Iop) {
        for (address, &parcel) in iop.memory().iter().enumerate() {
            if self.shadow[address] != parcel {
                self.shadow[address] = parcel;
                self.short(SET_MEMORY, address as u16, parcel);
            }
        }
    }

    /// One step of `iop`, recorded.  Returns its clock periods.
    pub fn step(&mut self, iop: &mut Iop, channels: &mut dyn Channels) -> u32 {
        let (p, a) = (iop.p(), iop.a());
        let parcel = iop.memory()[p as usize];
        let (f, d) = (parcel >> 9, parcel & 0o777);
        let by_b = matches!(f, 0o042 | 0o043 | 0o160..=0o177);
        let on_channel = matches!(f, 0o040..=0o043 | 0o140..=0o177);
        let number = match (on_channel, by_b) {
            (false, _) => 0,
            (true, false) => d,
            (true, true) => iop.b(),
        };
        // a store of the instruction itself, which the reader does too
        let stores = matches!(f, 0o034..=0o037).then(|| iop.operand(d as usize));
        let clock = iop.done(channel::RTC) && iop.enabled(channel::RTC);
        let request = if clock {
            channel::RTC as u8
        } else {
            channels.interrupt().unwrap_or(0)
        };
        let before = iop.counters();
        let clock_periods = iop.step(channels);
        let interrupt = iop.counters().interrupts != before.interrupts;
        let value = match f {
            _ if interrupt => 0,
            0o040..=0o043 => iop.c() as u16,
            0o150..=0o153 | 0o170..=0o173 => iop.a(),
            _ => 0,
        };
        let mut record = [0u8; RECORD_BYTES];
        record[0] = if interrupt { INTERRUPT } else { INSTRUCTION };
        record[1] = request;
        for (n, field) in [p, parcel, number, a, value, iop.p(), iop.a(), iop.b()]
            .into_iter()
            .enumerate()
        {
            record[2 + 2 * n..4 + 2 * n].copy_from_slice(&field.to_le_bytes());
        }
        debug_assert!((1..16).contains(&clock_periods));
        record[18] = iop.e() | (clock_periods as u8) << 4;
        record[19] = iop.c() as u8
            | (iop.interrupt_enable() as u8) << 1
            | (iop.interrupt_enable_delayed() as u8) << 2
            | (iop.done(channel::PFR) as u8) << 3
            | (iop.done(channel::PXS) as u8) << 4
            | (iop.held() as u8) << 5;
        let _ = self.out.write_all(&record);
        self.steps += 1;
        if !interrupt {
            if let Some(address) = stores {
                self.shadow[address as usize] = iop.memory()[address as usize];
            }
            // a function of a channel outside the processor may have moved data
            if f >= 0o140 && (channel::RTC + 1..crate::iop::CHANNELS).contains(&number) {
                self.sync(iop);
            }
        }
        clock_periods
    }

    /// End the record with everything the reader should now hold.
    pub fn finish(&mut self, iop: &Iop) {
        self.sync(iop);
        for address in 0..MEMORY_PARCELS {
            self.short(CHECK_MEMORY, address as u16, iop.memory()[address]);
        }
        for d in 0..OPERAND_REGISTERS {
            self.short(CHECK_OPERAND, d as u16, iop.operand(d));
        }
        for e in 0..EXIT_STACK {
            self.short(CHECK_EXIT_STACK, e as u16, iop.exit_stack()[e]);
        }
        self.short(END, 0, 0);
        let _ = self.out.flush();
    }
}

/// A source of numbers that is the same on every machine.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 16
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Channels that answer anything: a function reads a random value, a flag
/// is random, and now and then a random channel asks for an interrupt.
struct RandomChannels {
    random: Random,
    request: Option<u8>,
}

impl Channels for RandomChannels {
    fn function(&mut self, mem: &mut [u16], _: u8, function: u8, _: u16) -> Option<u16> {
        // some functions move data by themselves
        if function == 4 && self.random.below(4) == 0 {
            let at = self.random.below(MEMORY_PARCELS as u64 - 8) as usize;
            for parcel in mem[at..at + 8].iter_mut() {
                *parcel = self.random.next() as u16;
            }
        }
        (0o10..=0o13)
            .contains(&function)
            .then(|| self.random.next() as u16)
    }
    fn busy(&mut self, _: u8) -> bool {
        self.random.below(2) == 1
    }
    fn done(&mut self, _: u8) -> bool {
        self.random.below(2) == 1
    }
    fn interrupt(&mut self) -> Option<u8> {
        self.request
    }
}

/// How the random programs of `random` are made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mix {
    /// Every parcel of memory is random: every operation code as often as
    /// any other, with branches all over memory.
    Noise,
    /// Mostly operations on the registers and memory, some branches, a few
    /// functions; interrupts are let in more often.
    Work,
}

/// A random program on random channels, `steps` steps of it, recorded.
/// The same seed gives the same record.
pub fn random(seed: u64, steps: u64, mix: Mix, out: Box<dyn Write>) {
    let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut iop = Iop::new();
    for parcel in iop.memory_mut().iter_mut() {
        let noise = random.next() as u16;
        *parcel = match mix {
            Mix::Noise => noise,
            Mix::Work => {
                let f = match random.below(32) {
                    // I = 1 and a few instructions later an interrupt can come
                    0 => 0o003,
                    1 => 0o001 + random.below(2) as u16,
                    2..=4 => 0o070 + random.below(0o50) as u16,
                    // functions, mostly those the channels of the processor
                    // have, some of them by B
                    5..=7 => {
                        let usual = [0, 7, 7, 0o10, 0o10, 0o11, 0o14, 0o15];
                        let function = match random.below(8) {
                            0 => random.below(16) as u16,
                            _ => usual[random.below(8) as usize],
                        };
                        0o140 + function + 0o20 * (random.below(8) == 0) as u16
                    }
                    8 => 0o040 + random.below(4) as u16,
                    _ => {
                        [0o004, 0o010, 0o020, 0o030, 0o044, 0o050, 0o060][random.below(7) as usize]
                            + random.below(8) as u16
                    }
                };
                // small numbers: registers that are used again, and for a
                // function or a flag test mostly the first six channels
                let d = match (f, random.below(4)) {
                    (_, 0) => noise & 0o777,
                    (0o040..=0o043 | 0o140..=0o177, _) => noise % 6,
                    _ => noise & 0o37,
                };
                f << 9 | d
            }
        };
    }
    for d in 0..OPERAND_REGISTERS {
        // some of the registers a program of the second kind uses most are
        // zero: a branch through one sets the Program Fetch Request flag
        let zero = mix == Mix::Work && d < 0o40 && random.below(4) == 0;
        iop.set_operand(d, if zero { 0 } else { random.next() as u16 });
    }
    iop.set_a(random.next() as u16);
    iop.set_b(random.next() as u16);
    iop.set_c(random.below(2) == 1);
    let mut recorder = Recorder::new(out);
    // the reader's registers are zero at power-up: these are set as its
    // first instructions would set them, by a record of their own kind
    recorder.master_clear();
    recorder.sync(&iop);
    for d in 0..OPERAND_REGISTERS {
        recorder.short(SET_OPERAND, d as u16, iop.operand(d));
    }
    recorder.short(SET_REGISTERS, iop.a(), iop.b() | (iop.c() as u16) << 15);
    // the dead start interrupt, then whatever comes
    let mut channels = RandomChannels {
        random: Random(seed ^ 0x5DEE_CE66_D1CE_4E5B | 1),
        request: Some(5),
    };
    for step in 0..steps {
        // half way, a Master Clear and a second dead start: the registers
        // keep what they hold
        if step == steps / 2 && step != 0 {
            iop.master_clear();
            recorder.master_clear();
            channels.request = Some(5);
        }
        recorder.step(&mut iop, &mut channels);
        let often = if mix == Mix::Work { 3 } else { 8 };
        if channels.random.below(often) == 0 {
            channels.request = match channels.random.below(3) {
                0 => None,
                _ => Some(4 + channels.random.below(0o44) as u8),
            };
        }
    }
    recorder.finish(&iop);
}

/// A short program for what random programs seldom reach: both channels
/// of the processor that can ask for an interrupt asking at once.  The
/// Program Fetch Request, channel 1, is reported first, then the Exit Stack
/// Boundary, channel 2, then nothing.
pub fn directed(out: Box<dyn Write>) {
    let ins = |f: u16, d: u16| f << 9 | d;
    let program = [
        ins(0o147, 2), // PXS : 7
        ins(0o147, 1), // PFR : 7
        ins(0o010, 0), // A = 0
        ins(0o024, 5), // OR[5] = A
        ins(0o010, 13),
        ins(0o154, 2), // PXS : 14, E = 13
        ins(0o072, 2), // R = P + 2: E reaches 14, the boundary flag sets
        0,
        ins(0o075, 5), // P = OR[5] + k through a register that is zero
        11,
        0,
        ins(0o150, 0), // IOR : 10 reads 1
        ins(0o140, 1), // PFR : 0
        ins(0o150, 0), // IOR : 10 reads 2
        ins(0o140, 2), // PXS : 0
        ins(0o150, 0), // IOR : 10 reads 0
    ];
    let mut iop = Iop::new();
    iop.memory_mut()[..program.len()].copy_from_slice(&program);
    let mut recorder = Recorder::new(out);
    recorder.master_clear();
    recorder.sync(&iop);
    let mut channels = RandomChannels {
        random: Random(1),
        request: Some(5),
    };
    let mut read = Vec::new();
    for _ in 0..14 {
        let reads = iop.memory()[iop.p() as usize] == ins(0o150, 0) && !iop.held();
        recorder.step(&mut iop, &mut channels);
        channels.request = None;
        if reads {
            read.push(iop.a());
        }
    }
    assert_eq!(read, [1, 2, 0], "the program did not do what it is for");
    recorder.finish(&iop);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Sink(Rc<RefCell<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_directed_program_runs() {
        let sink = Sink::default();
        directed(Box::new(sink.clone()));
        assert_eq!(sink.0.borrow().len() % RECORD_BYTES, 0);
    }

    #[test]
    fn a_record_is_repeatable_and_whole() {
        let (one, two, other) = (Sink::default(), Sink::default(), Sink::default());
        random(7, 500, Mix::Work, Box::new(one.clone()));
        random(7, 500, Mix::Work, Box::new(two.clone()));
        random(8, 500, Mix::Work, Box::new(other.clone()));
        let record = one.0.borrow();
        assert_eq!(*record, *two.0.borrow());
        assert_ne!(*record, *other.0.borrow());
        assert_eq!(record.len() % RECORD_BYTES, 0);
        let kinds: Vec<u8> = record.chunks(RECORD_BYTES).map(|r| r[0]).collect();
        let count = |kind: u8| kinds.iter().filter(|&&k| k == kind).count();
        assert_eq!(count(INSTRUCTION) + count(INTERRUPT), 500);
        // the first step is the dead start interrupt from channel 5: E is 1
        // after it, and it took 9 clock periods
        let first = record
            .chunks(RECORD_BYTES)
            .find(|r| r[0] <= INTERRUPT)
            .unwrap();
        assert_eq!((first[0], first[1], first[18]), (INTERRUPT, 5, 1 | 9 << 4));
        assert_eq!((count(MASTER_CLEAR), count(IDLE)), (2, 2));
        assert!(
            count(INTERRUPT) > 1,
            "no interrupt was taken after the dead start"
        );
        assert_eq!(count(CHECK_MEMORY), MEMORY_PARCELS);
        assert_eq!(count(CHECK_OPERAND), OPERAND_REGISTERS);
        assert_eq!(count(CHECK_EXIT_STACK), EXIT_STACK);
        assert_eq!(kinds.last(), Some(&END));
    }
}

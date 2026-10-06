//! The Peripheral Expander, channel 17 octal of the MIOP, and its devices
//! [HW 7-7 to 7-14].
//!
//! The expander is a bus for controllers of the Data General kind.  Each
//! device has three registers A, B and C, a Busy and a Done flag and an
//! interrupt request, and takes the control pulses Start, Clear and Pulse.
//! A device is selected by a 6-bit address and moves its data to and from
//! Local Memory by itself.
//!
//! The channel's functions are from the manual.  What the devices do with
//! their registers is not in any Cray manual in `research/`: it follows the
//! cray-sim project's simulator, with which the IOS software is known to
//! work, as `research/notes/ios-devices-spec.md` part 10 records it.  Two
//! status bits also follow that simulator where the manual reads otherwise:
//! bit 9 of the status is the selected device's interrupt mask, and bit 13
//! is the selected device's interrupt request.
//!
//! Devices: tape 22, disk 60, printer 17 and, if there is one, the clock on
//! 50 (what it answers) and 51 (what is sent to it).

use crate::image::{Image, Tape, TapeState};
use crate::system::{Flags, Timing};

/// Device addresses, octal.
pub const PRINTER: u8 = 0o17;
pub const TAPE: u8 = 0o22;
pub const CLOCK_IN: u8 = 0o50;
pub const CLOCK_OUT: u8 = 0o51;
pub const DISK: u8 = 0o60;

/// In ascending order: the lowest address has the highest priority.
const DEVICES: [u8; 5] = [PRINTER, TAPE, CLOCK_IN, CLOCK_OUT, DISK];

/// Control pulses of function 17 [HW 7-14].
const START: u16 = 1;
const CLEAR: u16 = 2;
const PULSE: u16 = 4;

/// The sector of the expander disk: 256 parcels.
pub const DISK_SECTOR_BYTES: usize = 512;
const DISK_HEADS: u64 = 5;
const DISK_SECTORS: u64 = 35;

/// Busy, Done and the interrupt request of one device, and when the
/// operation it is doing ends.
#[derive(Clone, Copy, Default)]
struct Unit {
    busy: bool,
    done: bool,
    interrupt: bool,
    until: Option<u64>,
}

impl Unit {
    /// An operation that ends `delay` clock periods from now with Done and
    /// an interrupt request.
    fn start(&mut self, now: u64, delay: u32) {
        self.busy = true;
        self.done = false;
        self.until = Some(now + delay as u64);
    }
    fn settle(&mut self, now: u64) {
        if self.until.is_some_and(|t| now >= t) {
            self.until = None;
            self.busy = false;
            self.done = true;
            self.interrupt = true;
        }
    }
}

/// The tape drive.  A is the command and, read back, the status; B the
/// Local Memory address; C the count, negative.
struct TapeDrive {
    unit: Unit,
    tape: Tape,
    a: u16,
    b: u16,
    c: u16,
    status: u16,
}

/// Tape status bits.
const TAPE_READY: u16 = 0x0001;
const TAPE_BEGINNING: u16 = 0x0080;
const TAPE_FILE_MARK: u16 = 0x0100;
const TAPE_END: u16 = 0x0200;
const TAPE_ERROR: u16 = 0x8000;

impl TapeDrive {
    fn dia(&self) -> u16 {
        self.status
            | match self.tape.state() {
                TapeState::Beginning => TAPE_BEGINNING,
                TapeState::FileMark => TAPE_FILE_MARK,
                TapeState::End => TAPE_END,
                TapeState::Record => 0,
            }
    }

    /// The count in C: it is entered as its negative.
    fn count(&self) -> usize {
        self.c.wrapping_neg() as usize
    }

    /// Status after a motion: an error if it ran into a file mark or the
    /// end of the tape.
    fn moved(&self) -> u16 {
        match self.tape.state() {
            TapeState::FileMark | TapeState::End => TAPE_READY | TAPE_ERROR,
            _ => TAPE_READY,
        }
    }

    fn start(&mut self, mem: &mut [u16], now: u64, timing: &Timing) {
        // the command is bits 5 to 3 of A
        self.status = match self.a >> 3 & 7 {
            0 => {
                for parcel in self.tape.read(self.count()) {
                    mem[self.b as usize] = parcel;
                    self.b = self.b.wrapping_add(1);
                    self.c = self.c.wrapping_add(1);
                }
                self.moved()
            }
            1 => {
                self.tape.rewind();
                TAPE_READY
            }
            command @ (3 | 4) => {
                let mut status = TAPE_READY;
                while self.c != 0 {
                    let record = if command == 3 {
                        self.tape.space_forward()
                    } else {
                        self.tape.space_backward()
                    };
                    if !record {
                        status |= TAPE_ERROR;
                        break;
                    }
                    self.c = self.c.wrapping_add(1);
                }
                status
            }
            5 => {
                let mut record = Vec::with_capacity(self.count());
                while self.c != 0 {
                    record.push(mem[self.b as usize]);
                    self.b = self.b.wrapping_add(1);
                    self.c = self.c.wrapping_add(1);
                }
                self.tape.write(&record);
                TAPE_READY
            }
            6 => {
                self.tape.write_mark();
                TAPE_READY
            }
            _ => TAPE_READY,
        };
        self.unit.start(now, timing.expander_tape);
    }
}

/// The 80 Mbyte disk.  B is entered into the register that C then names;
/// with a command in C, B is the Local Memory address.
struct Disk {
    unit: Unit,
    image: Image,
    a: u16,
    b: u16,
    c: u16,
    cylinder: u16,
    head: u16,
    sector: u16,
    count: u16,
}

impl Disk {
    fn doc(&mut self, value: u16) {
        self.c = value;
        match value {
            5 => self.cylinder = self.b,
            1 => self.head = self.b,
            2 => self.sector = self.b,
            3 => self.count = self.b,
            _ => {}
        }
    }

    fn start(&mut self, mem: &mut [u16], now: u64, timing: &Timing) {
        let first = (self.cylinder as u64 * DISK_HEADS + self.head as u64) * DISK_SECTORS
            + self.sector as u64;
        match self.c {
            // read
            0 => {
                for n in 0..self.count as u64 {
                    for parcel in self.image.read(first + n) {
                        mem[self.b as usize] = parcel;
                        self.b = self.b.wrapping_add(1);
                    }
                }
            }
            // write, and format, which writes zeros
            0o10 | 0o20 => {
                let parcels = DISK_SECTOR_BYTES / 2;
                for n in 0..self.count as u64 {
                    let mut sector = vec![0; parcels];
                    for parcel in sector.iter_mut() {
                        if self.c == 0o10 {
                            *parcel = mem[self.b as usize];
                        }
                        self.b = self.b.wrapping_add(1);
                    }
                    self.image.write(first + n, &sector);
                }
            }
            // return to cylinder 0
            0o120 => {
                self.cylinder = 0;
                self.head = 0;
                self.sector = 0;
            }
            // anything else does nothing and does not answer
            _ => return,
        }
        self.a &= !1;
        self.unit.start(now, timing.expander_disk);
    }
}

/// The printer, which is a plotter too.  A command in A is run by Pulse; B
/// is the count, negative, and writing the Local Memory address to C prints,
/// two characters to a parcel.  In graphics mode a parcel is sixteen dots of
/// a row of 1,056; here a character stands for eight of them, `X` if any is
/// set, so that a row is as wide as a line of text.
#[derive(Default)]
struct Printer {
    unit: Unit,
    a: u16,
    b: u16,
    status: u16,
    graphics: bool,
    text: Vec<u8>,
}

const PRINTER_DONE: u16 = 0x4000;

impl Printer {
    fn doc(&mut self, mem: &[u16], mut address: u16) {
        while self.b != 0 {
            let bytes = mem[address as usize].to_be_bytes();
            if self.graphics {
                self.text
                    .extend(bytes.map(|dots| if dots != 0 { b'X' } else { b' ' }));
            } else {
                self.text.extend(bytes);
            }
            address = address.wrapping_add(1);
            self.b = self.b.wrapping_add(1);
        }
        self.unit.busy = false;
        self.unit.done = true;
        self.unit.interrupt = true;
        self.status = PRINTER_DONE;
    }

    fn pulse(&mut self) {
        self.unit.busy = false;
        self.unit.done = true;
        match self.a {
            // new page, new line
            0 | 3 => {
                self.text.push(if self.a == 0 { 0x0C } else { b'\n' });
                self.unit.interrupt = true;
                self.status = PRINTER_DONE;
            }
            6 => {
                self.unit.interrupt = false;
                self.status = 0;
            }
            // graphics mode and text mode
            1 | 4 => {
                self.graphics = self.a == 1;
                self.status = 0;
            }
            _ => self.status = 0,
        }
    }
}

/// The clock: a Hayes Chronograph, which takes commands in characters and
/// answers in characters.  A character goes out by A of device 51 and
/// Start; each one is answered by an interrupt of device 51.  An answer
/// comes in one character at a time through A of device 50, each announced
/// by an interrupt of device 50.
pub struct Clock {
    /// `YYMMDD` and `HHMMSS`.
    date: String,
    time: String,
    a: u16,
    request: Vec<u8>,
    answer: std::collections::VecDeque<u8>,
    /// Interrupt requests of device 50 and of device 51.
    interrupt: [bool; 2],
    /// When the next character of the answer is announced.
    announce: Option<u64>,
}

impl Clock {
    pub fn new(date: &str, time: &str) -> Clock {
        Clock {
            date: date.to_string(),
            time: time.to_string(),
            a: 0,
            request: Vec::new(),
            answer: Default::default(),
            interrupt: [false; 2],
            announce: None,
        }
    }

    fn settle(&mut self, now: u64) {
        if self.announce.is_some_and(|t| now >= t) {
            self.announce = None;
            self.interrupt[0] = true;
        }
    }

    fn doa(&mut self, device: u8, value: u16) {
        self.a = value;
        self.interrupt[(device == CLOCK_OUT) as usize] = false;
    }

    fn dia(&mut self, device: u8) -> u16 {
        if device != CLOCK_IN {
            return 0;
        }
        match self.answer.pop_front() {
            Some(c) => {
                self.interrupt[0] = true;
                c as u16
            }
            None => 0,
        }
    }

    fn control(&mut self, device: u8, control: u16, now: u64, timing: &Timing) {
        let n = (device == CLOCK_OUT) as usize;
        if control & CLEAR != 0 {
            self.interrupt[n] = false;
            if n == 0 && !self.answer.is_empty() {
                self.announce = Some(now + timing.expander_clock as u64);
            }
        }
        if control & START != 0 {
            let c = self.a as u8;
            if c == b'\r' {
                let answer = match self.request.as_slice() {
                    b"ATRD" => format!("{}\r", self.date),
                    b"ATRT" => format!("{}\r", self.time),
                    _ => "0\r".to_string(),
                };
                self.answer = answer.bytes().collect();
                self.announce = Some(now + timing.expander_clock as u64);
                self.request.clear();
            } else {
                self.request.push(c);
            }
            self.interrupt[n] = true;
        }
    }
}

/// The channel and the devices on it.  The channel's own Busy and Done
/// flags are with those of the MIOP's other channels and are passed in.
pub struct Expander {
    /// The device address register, 6 bits.
    address: u8,
    /// What the last function 1, 2 or 3 put on the data bus.
    bus: u16,
    /// A set bit keeps the device that owns it from interrupting.
    mask: u16,
    /// Function 7, bit 0: interrupt at the end of a delayed function.
    channel_interrupts: bool,
    /// Function 7, bit 1: interrupts from the devices.
    device_interrupts: bool,
    tape: TapeDrive,
    disk: Disk,
    printer: Printer,
    clock: Option<Clock>,
}

impl Expander {
    pub fn new(tape: Tape, disk: Image, clock: Option<Clock>) -> Expander {
        Expander {
            address: 0,
            bus: 0,
            mask: 0,
            channel_interrupts: false,
            device_interrupts: false,
            tape: TapeDrive {
                unit: Unit::default(),
                tape,
                a: 0,
                b: 0,
                c: 0,
                status: 0,
            },
            disk: Disk {
                unit: Unit::default(),
                image: disk,
                a: 0,
                b: 0,
                c: 0,
                cylinder: 0,
                head: 0,
                sector: 0,
                count: 0,
            },
            printer: Printer::default(),
            clock,
        }
    }

    /// Master Clear of the MIOP.
    pub fn master_clear(&mut self) {
        self.address = 0;
        self.mask = 0;
        self.channel_interrupts = false;
        self.device_interrupts = false;
        self.tape.unit = Unit::default();
        self.disk.unit = Unit::default();
        self.printer.unit = Unit::default();
        self.printer.graphics = false;
        if let Some(clock) = &mut self.clock {
            clock.interrupt = [false; 2];
            clock.announce = None;
        }
    }

    /// What the printer has printed.
    pub fn printed(&self) -> &[u8] {
        &self.printer.text
    }

    /// Sectors of the disk written since the start.
    pub fn disk_written(&self) -> usize {
        self.disk.image.written_blocks()
    }

    fn exists(&self, device: u8) -> bool {
        match device {
            PRINTER | TAPE | DISK => true,
            CLOCK_IN | CLOCK_OUT => self.clock.is_some(),
            _ => false,
        }
    }

    /// The mask bit of a device [HW 7-10].
    fn mask_bit(device: u8) -> u16 {
        match device {
            DISK => 1 << 6,
            TAPE => 1 << 5,
            PRINTER => 1 << 3,
            _ => 1 << 1,
        }
    }

    fn unit(&self, device: u8) -> Unit {
        match device {
            PRINTER => self.printer.unit,
            TAPE => self.tape.unit,
            DISK => self.disk.unit,
            CLOCK_IN | CLOCK_OUT => Unit {
                interrupt: self
                    .clock
                    .as_ref()
                    .is_some_and(|c| c.interrupt[(device == CLOCK_OUT) as usize]),
                ..Unit::default()
            },
            _ => Unit::default(),
        }
    }

    /// The selected device's part of a status word: bit 7 it can move data
    /// by itself, bit 9 it may interrupt, bit 13 it asks to, bit 14 its
    /// Busy and bit 15 its Done flag.
    fn device_status(&self) -> u16 {
        if !self.exists(self.address) {
            return 0;
        }
        let unit = self.unit(self.address);
        1 << 7
            | ((self.mask & Self::mask_bit(self.address) == 0) as u16) << 9
            | (unit.interrupt as u16) << 13
            | (unit.busy as u16) << 14
            | (unit.done as u16) << 15
    }

    /// The channel's part: bit 8 its interrupt enable, bits 10 and 11 Busy,
    /// bit 12 Done.
    fn channel_status(&self, flags: &Flags) -> u16 {
        (self.channel_interrupts as u16) << 8
            | (flags.busy as u16) << 10
            | (flags.busy as u16) << 11
            | (flags.done as u16) << 12
    }

    /// The channel asks for an interrupt: at the end of a delayed function
    /// if function 7 said so, and for a device that asks and is not masked
    /// [HW 7-10].
    pub fn request(&self, flags: &Flags) -> bool {
        (self.channel_interrupts && flags.done)
            || (self.device_interrupts
                && DEVICES.iter().any(|&d| {
                    self.exists(d) && self.unit(d).interrupt && self.mask & Self::mask_bit(d) == 0
                }))
    }

    /// Let the devices finish what is due.
    pub fn settle(&mut self, now: u64) {
        self.tape.unit.settle(now);
        self.disk.unit.settle(now);
        if let Some(clock) = &mut self.clock {
            clock.settle(now);
        }
    }

    /// When the next device finishes.
    pub fn next_event(&self) -> Option<u64> {
        [
            self.tape.unit.until,
            self.disk.unit.until,
            self.clock.as_ref().and_then(|c| c.announce),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// A function of channel 17.  `flags` are the channel's Busy and Done,
    /// `mem` the MIOP's Local Memory.
    pub fn function(
        &mut self,
        flags: &mut Flags,
        mem: &mut [u16],
        now: u64,
        timing: &Timing,
        function: u8,
        a: u16,
    ) -> Option<u16> {
        let device = self.address;
        match function {
            // idle the channel
            0 => {
                flags.idle();
                self.channel_interrupts = false;
                return None;
            }
            // fetch register A, B or C of the device onto the data bus
            1 => {
                self.bus = match device {
                    PRINTER => self.printer.status,
                    TAPE => self.tape.dia(),
                    DISK => self.disk.a,
                    CLOCK_IN | CLOCK_OUT => self.clock.as_mut().map_or(0, |c| c.dia(device)),
                    _ => 0,
                }
            }
            2 => {
                self.bus = match device {
                    TAPE => self.tape.b,
                    DISK => self.disk.b,
                    _ => 0,
                }
            }
            3 => {
                self.bus = match device {
                    TAPE => self.tape.c,
                    DISK => self.disk.c,
                    _ => 0,
                }
            }
            // fetch Busy, Done and the interrupting device: they are read
            // as they are when function 11 asks
            4 => {}
            5 => {
                self.address = a as u8 & 0o77;
                return None;
            }
            6 => self.mask = a,
            7 => {
                self.channel_interrupts = a & 1 != 0;
                self.device_interrupts = a & 2 != 0;
                return None;
            }
            0o10 => return Some(self.bus),
            // status 1, with the address of the highest priority device
            // that asks for an interrupt; status 2, with the address
            // register
            0o11 => {
                let asking = DEVICES
                    .iter()
                    .find(|&&d| self.exists(d) && self.unit(d).interrupt);
                return Some(
                    self.device_status()
                        | self.channel_status(flags)
                        | asking.map_or(0, |&d| d as u16),
                );
            }
            0o13 => return Some(self.device_status() | self.channel_status(flags) | device as u16),
            0o14 => match device {
                PRINTER => self.printer.a = a,
                TAPE => self.tape.a = a,
                DISK => self.disk.a = a,
                CLOCK_IN | CLOCK_OUT => {
                    if let Some(clock) = &mut self.clock {
                        clock.doa(device, a)
                    }
                }
                _ => {}
            },
            0o15 => match device {
                PRINTER => self.printer.b = a,
                TAPE => self.tape.b = a,
                DISK => self.disk.b = a,
                _ => {}
            },
            0o16 => match device {
                PRINTER => self.printer.doc(mem, a),
                TAPE => self.tape.c = a,
                DISK => self.disk.doc(a),
                _ => {}
            },
            0o17 => match device {
                PRINTER => {
                    if a & CLEAR != 0 {
                        self.printer.unit.interrupt = false;
                    }
                    if a & PULSE != 0 {
                        self.printer.pulse();
                    }
                }
                TAPE => {
                    if a & CLEAR != 0 {
                        self.tape.unit.interrupt = false;
                    }
                    if a & START != 0 {
                        self.tape.start(mem, now, timing);
                    }
                }
                DISK => {
                    if a & CLEAR != 0 {
                        self.disk.unit.interrupt = false;
                    }
                    if a & START != 0 {
                        self.disk.start(mem, now, timing);
                    }
                }
                CLOCK_IN | CLOCK_OUT => {
                    if let Some(clock) = &mut self.clock {
                        clock.control(device, a, now, timing)
                    }
                }
                _ => {}
            },
            _ => return None,
        }
        // a delayed function: Busy now, Done at least a microsecond later
        // [HW 7-14]
        flags.start(now, timing.expander_function);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(records: &[&[u8]]) -> Tape {
        let mut bytes = Vec::new();
        for r in records {
            bytes.extend((r.len() as u32).to_le_bytes());
            bytes.extend(*r);
            bytes.extend((r.len() as u32).to_le_bytes());
        }
        Tape::from_tap(&bytes).unwrap()
    }

    struct Bench {
        expander: Expander,
        flags: Flags,
        mem: Vec<u16>,
        now: u64,
        timing: Timing,
    }

    impl Bench {
        fn new(clock: bool) -> Bench {
            let mut disk = Image::empty(DISK_SECTOR_BYTES);
            disk.write(35 * 5 * 2 + 35 + 8, &[0x1111; 256]);
            disk.write(35 * 5 * 2 + 35 + 9, &[0x2222; 256]);
            Bench {
                expander: Expander::new(
                    tap(&[&[0xAB, 0xCD, 0x12, 0x34], &[0x56, 0x78]]),
                    disk,
                    clock.then(|| Clock::new("890606", "123538")),
                ),
                flags: Flags::default(),
                mem: vec![0; 1 << 16],
                now: 0,
                timing: Timing::default(),
            }
        }
        /// A function, after which the channel's delay is waited out.
        fn f(&mut self, function: u8, a: u16) -> u16 {
            let value = self.expander.function(
                &mut self.flags,
                &mut self.mem,
                self.now,
                &self.timing,
                function,
                a,
            );
            if self.flags.busy {
                self.now += self.timing.expander_function as u64;
                self.flags.settle(self.now);
                assert!(self.flags.done && !self.flags.busy);
            }
            value.unwrap_or(0)
        }
        fn wait(&mut self, clock_periods: u32) {
            self.now += clock_periods as u64;
            self.expander.settle(self.now);
        }
        fn read(&mut self, register: u8) -> u16 {
            self.f(register, 0);
            self.f(0o10, 0)
        }
    }

    /// The kernel's own tape reader (kernel 4878 to 4897).
    #[test]
    fn tape_read_as_the_kernel_does_it() {
        let mut b = Bench::new(false);
        b.f(6, 0xFFE7);
        b.f(0, 0);
        b.f(5, TAPE as u16);
        b.f(0o15, 0x4D5C);
        b.f(0o16, 0u16.wrapping_sub(2048));
        b.f(0o14, 0);
        b.f(0o17, START);
        b.f(4, 0);
        // not done yet: the device is busy, the channel is done
        assert_eq!(b.f(0o11, 0), 0x5080);
        b.wait(b.timing.expander_tape);
        // Done, asking, masked; the tape is the device that asks
        assert_eq!(b.f(0o11, 0), 0xB080 | TAPE as u16);
        assert_eq!(b.read(1), TAPE_READY);
        assert_eq!(b.read(2), 0x4D5E);
        assert_eq!(b.read(3), 0u16.wrapping_sub(2046));
        assert_eq!(&b.mem[0x4D5C..0x4D5F], [0xABCD, 0x1234, 0]);
        assert!(!b.expander.request(&b.flags));
        // rewind: A = 10 octal
        b.f(0o14, 0o10);
        b.f(0o17, START | CLEAR);
        b.wait(b.timing.expander_tape);
        assert_eq!(b.read(1), TAPE_READY | TAPE_BEGINNING);
    }

    #[test]
    fn tape_stops_at_the_end() {
        let mut b = Bench::new(false);
        b.f(5, TAPE as u16);
        for expect in [TAPE_READY, TAPE_READY, TAPE_READY | TAPE_ERROR | TAPE_END] {
            b.f(0o15, 0x100);
            b.f(0o16, 0xFFF0);
            b.f(0o14, 0);
            b.f(0o17, START);
            b.wait(b.timing.expander_tape);
            assert_eq!(b.read(1), expect);
        }
    }

    #[test]
    fn device_interrupts_and_the_mask() {
        let mut b = Bench::new(false);
        b.f(6, 0);
        b.f(7, 2);
        b.f(5, DISK as u16);
        // cylinder 2, head 1, sector 8, two sectors, read to 0xD800
        for (value, register) in [(2, 5), (1, 1), (8, 2), (2, 3)] {
            b.f(0o15, value);
            b.f(0o16, register);
        }
        b.f(0o14, 0x4000);
        b.f(0o15, 0xD800);
        b.f(0o16, 0);
        b.f(0o17, START);
        assert!(!b.expander.request(&b.flags));
        b.wait(b.timing.expander_disk);
        assert!(b.expander.request(&b.flags));
        assert_eq!(b.mem[0xD800], 0x1111);
        assert_eq!(b.mem[0xD9FF], 0x2222);
        // the handler asks who it was
        b.f(5, 0);
        b.f(4, 0);
        assert_eq!(b.f(0o11, 0) & 0o77, DISK as u16);
        b.f(5, DISK as u16);
        assert_eq!(b.read(1), 0x4000);
        assert_eq!(b.read(2), 0xDA00);
        // masked, it does not interrupt; unmasked and cleared, neither
        b.f(6, 1 << 6);
        assert!(!b.expander.request(&b.flags));
        b.f(6, 0);
        assert!(b.expander.request(&b.flags));
        b.f(0o17, CLEAR);
        assert!(!b.expander.request(&b.flags));
        b.f(4, 0);
        assert_eq!(b.f(0o11, 0) & 0o77, 0);
        // without mode bit 1 a device does not interrupt either
        b.f(0o17, START);
        b.f(7, 0);
        b.wait(b.timing.expander_disk);
        assert!(!b.expander.request(&b.flags));
        // mode bit 0: the end of a delayed function interrupts
        b.f(7, 1);
        b.f(4, 0);
        assert!(b.expander.request(&b.flags));
        b.f(0, 0);
        assert!(!b.expander.request(&b.flags));
    }

    #[test]
    fn disk_write() {
        let mut b = Bench::new(false);
        b.f(5, DISK as u16);
        for (value, register) in [(0, 5), (0, 1), (0, 2), (1, 3)] {
            b.f(0o15, value);
            b.f(0o16, register);
        }
        b.mem[0x2000..0x2100].fill(0x7777);
        b.f(0o15, 0x2000);
        b.f(0o16, 0o10);
        b.f(0o17, START);
        b.wait(b.timing.expander_disk);
        assert_eq!(b.expander.disk_written(), 3);
        b.f(0o15, 0x3000);
        b.f(0o16, 0);
        b.f(0o17, START | CLEAR);
        assert_eq!(b.mem[0x3000..0x3100], [0x7777; 256]);
    }

    /// The clock dialogue of the overlay XCLOCK.
    #[test]
    fn clock_dialogue() {
        let mut b = Bench::new(true);
        b.f(6, 0);
        b.f(7, 2);
        let mut answer = Vec::new();
        b.f(5, CLOCK_OUT as u16);
        for c in *b"ATRD\r" {
            b.f(0o14, c as u16);
            b.f(0o17, START);
            assert!(b.expander.request(&b.flags));
            b.f(4, 0);
            assert_eq!(b.f(0o11, 0) & 0o77, CLOCK_OUT as u16);
            b.f(0o17, CLEAR);
        }
        for _ in 0..7 {
            assert!(!b.expander.request(&b.flags));
            b.wait(b.timing.expander_clock);
            assert!(b.expander.request(&b.flags));
            b.f(5, 0);
            b.f(4, 0);
            assert_eq!(b.f(0o11, 0) & 0o77, CLOCK_IN as u16);
            b.f(5, CLOCK_IN as u16);
            answer.push(b.read(1) as u8);
            b.f(0o17, CLEAR);
        }
        assert_eq!(answer, b"890606\r");
        b.wait(b.timing.expander_clock);
        assert!(!b.expander.request(&b.flags));
    }

    #[test]
    fn a_missing_device_reads_zero() {
        let mut b = Bench::new(false);
        b.f(5, CLOCK_IN as u16);
        b.f(0o14, 5);
        b.f(0o17, START);
        assert_eq!(b.read(1), 0);
        b.f(4, 0);
        assert_eq!(b.f(0o13, 0), 1 << 12 | CLOCK_IN as u16);
    }
}

//! Channel interfaces other than the Peripheral Expander: Buffer Memory,
//! the 100 Mbyte channel to central memory, the DD-29 disk drives, the
//! consoles and the block multiplexer channel.
//!
//! Each interface here keeps its registers; its Busy, Done and Interrupt
//! Enable flags are with the other channels of its processor and are passed
//! in.  A transfer moves its data when the function is issued, and the
//! channel then stays Busy for the time the real one takes.
//!
//! Page numbers: `[HW n-m]` is HR-0030 rev B, `[DSK n-m]` is HR-0077, the
//! disk systems manual.  `research/notes/ios-devices-spec.md` ("the
//! devices spec") condenses both.

use crate::image::Image;
use crate::system::{Flags, Timing};
use std::collections::VecDeque;

/// Words of a block length register that holds zero [HW 5-14].
const FULL_BLOCK: u32 = 1 << 14;

/// The words of a transfer and the Local Memory parcels they are.
fn block(length: u16) -> u32 {
    match length & 0x3FFF {
        0 => FULL_BLOCK,
        n => n as u32,
    }
}

/// Four parcels of Local Memory as a word: the parcel at the lowest address
/// is bits 63 to 48 [HW 8-1].
fn word_from(mem: &[u16], address: u16) -> u64 {
    (0..4).fold(0, |w, n| {
        w << 16 | mem[address.wrapping_add(n) as usize] as u64
    })
}

fn word_to(mem: &mut [u16], address: u16, word: u64) {
    for n in 0..4 {
        mem[address.wrapping_add(n) as usize] = (word >> (48 - 16 * n)) as u16;
    }
}

/// Channel 5, MOS: block copies between Local Memory and Buffer Memory
/// [HW 5-13 to 5-16].
#[derive(Default)]
pub struct Mos {
    /// Local Memory address; the low two bits are forced to zero.
    local: u16,
    /// Buffer Memory address, 24 bits, entered in two parts.
    buffer: u32,
}

impl Mos {
    #[allow(clippy::too_many_arguments)]
    pub fn function(
        &mut self,
        flags: &mut Flags,
        mem: &mut [u16],
        bm: &mut [u64],
        now: u64,
        timing: &Timing,
        function: u8,
        a: u16,
    ) {
        match function {
            0 => flags.idle(),
            1 => self.local = a & !3,
            2 => self.buffer = self.buffer & 0x1FF | (a as u32 & 0x7FFF) << 9,
            3 => self.buffer = self.buffer & !0x1FF | a as u32 & 0x1FF,
            4 | 5 => self.copy(flags, mem, bm, now, timing, function == 4, block(a)),
            _ => {}
        }
    }

    /// Copy `words` words, into Local Memory if `read`.  The three
    /// registers are zero when the copy ends [HW 5-13].
    #[allow(clippy::too_many_arguments)]
    fn copy(
        &mut self,
        flags: &mut Flags,
        mem: &mut [u16],
        bm: &mut [u64],
        now: u64,
        timing: &Timing,
        read: bool,
        words: u32,
    ) {
        for n in 0..words {
            let local = self.local.wrapping_add(4 * n as u16);
            let buffer = (self.buffer + n) as usize % bm.len();
            if read {
                word_to(mem, local, bm[buffer]);
            } else {
                bm[buffer] = word_from(mem, local);
            }
        }
        self.local = 0;
        self.buffer = 0;
        flags.start(now, words * timing.mos_word);
    }

    /// The dead start: the registers are cleared, the whole of Local Memory
    /// (4,096 parcels of it if `short`) is read from Buffer Memory address
    /// 0, and the channel may interrupt [HW 5-15, 5-17].
    pub fn dead_start(
        &mut self,
        flags: &mut Flags,
        mem: &mut [u16],
        bm: &mut [u64],
        now: u64,
        timing: &Timing,
        short: bool,
    ) {
        *self = Mos::default();
        flags.enable = true;
        self.copy(
            flags,
            mem,
            bm,
            now,
            timing,
            true,
            if short { 1 << 10 } else { FULL_BLOCK },
        );
    }
}

/// Central memory as the 100 Mbyte channel reaches it: absolute addresses,
/// no base or limit.
pub trait CentralMemory {
    fn read(&self, address: u32) -> u64;
    fn write(&mut self, address: u32, word: u64);
}

/// Channels 14 and 15 of the BIOP, HIA and HOA: block copies between Local
/// Memory and central memory [HW 7-21 to 7-36].  The mainframe has no part
/// in them.
#[derive(Default)]
pub struct HighSpeed {
    local: u16,
    /// Central memory address, 22 bits used.
    central: u32,
}

impl HighSpeed {
    /// `input` is channel 14, which reads central memory with function 4;
    /// channel 15 writes it with function 5.
    #[allow(clippy::too_many_arguments)]
    pub fn function(
        &mut self,
        flags: &mut Flags,
        mem: &mut [u16],
        central: &mut dyn CentralMemory,
        now: u64,
        timing: &Timing,
        input: bool,
        function: u8,
        a: u16,
    ) {
        match function {
            0 => flags.idle(),
            1 => self.local = a & !3,
            2 => self.central = self.central & 0x1FF | (a as u32 & 0x7FFF) << 9,
            3 => self.central = self.central & !0x1FF | a as u32 & 0x1FF,
            4 | 5 if input == (function == 4) => {
                let words = block(a);
                for n in 0..words {
                    let local = self.local.wrapping_add(4 * n as u16);
                    let address = (self.central + n) & 0x3F_FFFF;
                    if input {
                        word_to(mem, local, central.read(address));
                    } else {
                        central.write(address, word_from(mem, local));
                    }
                }
                flags.start(now, words * timing.high_speed_word);
            }
            _ => {}
        }
    }
}

/// A sector of a DD-29: 512 words, 2,048 parcels [DSK 1-4].
pub const DD29_SECTOR_BYTES: usize = 4096;
const DD29_SECTOR_PARCELS: u16 = 2048;
const DD29_HEADS: u64 = 10;
const DD29_SECTORS: u64 = 18;
/// Parcels the two buffers of the controller hold [DSK 2-12].
const DD29_BUFFER: u16 = 512;

/// A DD-29 disk drive on its channel of a DCU-4 controller [DSK 2-1 to
/// 2-20].  There are no faults: Busy never stays set with Done, and the
/// error and interlock registers read zero.
pub struct Dd29 {
    image: Image,
    /// The Local Memory Address register; the low two bits are forced to
    /// zero.
    local: u16,
    /// The Status Response register.
    status: u16,
    cylinder: u16,
    head: u16,
    reserved: bool,
    /// From Master Clear to the first function 1 a write fills the
    /// controller's buffers and a read empties them, with no disk involved
    /// [DSK 2-17].
    echo: bool,
    buffer: Vec<u16>,
}

impl Dd29 {
    pub fn new(image: Image) -> Dd29 {
        Dd29 {
            image,
            local: 0,
            status: 0,
            cylinder: 0,
            head: 0,
            reserved: false,
            echo: true,
            buffer: vec![0; DD29_BUFFER as usize],
        }
    }

    pub fn master_clear(&mut self) {
        self.local = 0;
        self.status = 0;
        self.reserved = false;
        self.echo = true;
    }

    /// Sectors written since the start.
    pub fn written(&self) -> usize {
        self.image.written_blocks()
    }

    /// The drive's image as it now is, as a new file.
    pub fn save(&self, to: &std::path::Path) -> std::io::Result<()> {
        self.image.save(to)
    }

    fn sector(&self, sector: u16) -> u64 {
        (self.cylinder as u64 * DD29_HEADS + self.head as u64) * DD29_SECTORS + sector as u64
    }

    pub fn function(
        &mut self,
        flags: &mut Flags,
        mem: &mut [u16],
        now: u64,
        timing: &Timing,
        function: u8,
        a: u16,
    ) -> Option<u16> {
        match function {
            0 => flags.idle(),
            // select a mode or ask for status [DSK 2-5 to 2-11]
            1 => {
                self.echo = false;
                match a >> 9 & 7 {
                    0 => self.reserved = false,
                    1 => {
                        self.reserved = true;
                        self.head = 0;
                    }
                    3 => self.cylinder = 0,
                    // the sector under the heads, the error flags
                    5 | 6 => self.status = 0,
                    7 => {
                        self.status = match a & 0o777 {
                            0 => self.cylinder,
                            // head group; bit 5 reserved to this processor,
                            // bit 6 a 600 Mbyte unit
                            1 if self.reserved => 0o140 | self.head,
                            // cylinders still to cross, complemented: on
                            // cylinder
                            2 => 0o1777,
                            _ => 0,
                        }
                    }
                    _ => {}
                }
                flags.start(now, timing.disk_select);
            }
            // read or write one sector at the Local Memory address
            2 | 3 => {
                let write = function == 3;
                if self.echo {
                    for n in 0..DD29_BUFFER {
                        let at = self.local.wrapping_add(n) as usize;
                        if write {
                            self.buffer[n as usize] = mem[at];
                        } else {
                            mem[at] = self.buffer[n as usize];
                        }
                    }
                    self.local = self.local.wrapping_add(DD29_BUFFER);
                    flags.start(now, timing.disk_select);
                } else {
                    let index = self.sector(a & 0o37);
                    if write {
                        // bits 7 to 5 ask for a special mode; all of them
                        // leave the sector zero
                        let data: Vec<u16> = (0..DD29_SECTOR_PARCELS)
                            .map(|n| {
                                if a & 0o340 == 0 {
                                    mem[self.local.wrapping_add(n) as usize]
                                } else {
                                    0
                                }
                            })
                            .collect();
                        self.image.write(index, &data);
                    } else {
                        for (n, parcel) in self.image.read(index).into_iter().enumerate() {
                            mem[self.local.wrapping_add(n as u16) as usize] = parcel;
                        }
                    }
                    self.local = self.local.wrapping_add(DD29_SECTOR_PARCELS);
                    flags.start(now, timing.disk_sector);
                }
            }
            // reserve the unit and select a head group; Busy and Done do
            // not change
            4 => {
                self.reserved = true;
                self.head = a & 0o17;
            }
            // seek; the first sector identifier read on arrival is the
            // cylinder shifted left 5 with zeros below [DSK 2-22]
            5 => {
                self.cylinder = a & 0o1777;
                self.status = self.cylinder << 5;
                flags.start(now, timing.disk_seek);
            }
            0o10 => return Some(self.local),
            0o11 => return Some(self.status),
            0o14 => self.local = a & !3,
            0o15 => self.status = a,
            _ => {}
        }
        None
    }
}

/// A console: a keyboard channel and a display channel, 7 bits each way
/// [HW 7-5, 7-6].
#[derive(Default)]
pub struct Console {
    typed: VecDeque<u8>,
    /// The character last read.
    last: u8,
    /// The earliest time of the next key.
    next_key: u64,
    /// Everything sent to the display.
    pub output: Vec<u8>,
}

impl Console {
    /// Keys to be pressed, one after the other.
    pub fn type_text(&mut self, flags: &mut Flags, now: u64, text: &[u8]) {
        self.typed.extend(text);
        self.expect_key(flags, now);
    }

    /// True if all that was typed has been read.
    pub fn idle(&self) -> bool {
        self.typed.is_empty()
    }

    /// The next key arrives: Done sets when it has.
    fn expect_key(&mut self, flags: &mut Flags, now: u64) {
        if !flags.done && flags.until.is_none() && !self.typed.is_empty() {
            flags.until = Some(now.max(self.next_key));
        }
    }

    pub fn keyboard(
        &mut self,
        flags: &mut Flags,
        now: u64,
        timing: &Timing,
        function: u8,
    ) -> Option<u16> {
        match function {
            // a key that was not read is lost
            0 | 0o10 => {
                if flags.done {
                    flags.done = false;
                    if let Some(key) = self.typed.pop_front() {
                        self.last = key & 0x7F;
                    }
                    self.next_key = now + timing.key as u64;
                }
                self.expect_key(flags, now);
                (function == 0o10).then_some(self.last as u16)
            }
            _ => None,
        }
    }

    /// The character goes out at once; the channel is Busy for the time it
    /// takes on the line.  It always completes, with or without a terminal.
    pub fn display(&mut self, flags: &mut Flags, now: u64, timing: &Timing, function: u8, a: u16) {
        match function {
            0 => flags.idle(),
            0o14 => {
                self.output.push(a as u8 & 0x7F);
                flags.start(now, timing.console_character);
            }
            _ => {}
        }
    }
}

/// A block multiplexer channel of the XIOP with no control unit on it
/// [HW 7-48 on].  Its registers read back what was entered, which is what
/// the XIOP's start-up test looks at; the way two entries of an address or
/// a count pair up is the cray-sim simulator's.
#[derive(Default)]
pub struct BlockMultiplexer {
    local: [u16; 2],
    count: u16,
    /// The count last entered, until it is read.
    entered: Option<u16>,
    /// An address has been entered since the last count: the next count is
    /// for the transfer, the one after for the transfer chained to it.
    load_count: bool,
    /// The device address and mode last entered, until it is read.
    mode: Option<u16>,
}

impl BlockMultiplexer {
    pub fn function(
        &mut self,
        flags: &mut Flags,
        now: u64,
        timing: &Timing,
        function: u8,
        a: u16,
    ) -> Option<u16> {
        match function {
            0 => flags.idle(),
            // reset, a channel command: there is nothing to answer, and it
            // is done
            1 | 2 | 4 => flags.finish(),
            // read the address of a requesting control unit: there is none
            3 => {
                flags.finish();
                flags.busy = true;
            }
            // the delay counter, for diagnostics
            5 => flags.start(now, timing.multiplexer_delay),
            0o10 => return Some(self.local[(a & 1) as usize] | a & 1),
            0o11 => return Some(self.entered.take().unwrap_or(self.count)),
            0o12 => return Some(self.mode.take().unwrap_or(0)),
            // input tags: bit 13, the byte count is zero
            0o13 => return Some(if self.count == 0 { 1 << 13 } else { 0 }),
            0o14 => {
                self.local[(a & 1) as usize] = a & !1;
                self.load_count = true;
            }
            0o15 => {
                self.entered = Some(a);
                if self.load_count {
                    self.count = a;
                }
                self.load_count = false;
            }
            0o16 => self.mode = Some(a),
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(flags: &mut Flags, now: &mut u64) {
        *now = flags.until.expect("the channel is at work");
        flags.settle(*now);
        assert!(flags.done && !flags.busy);
    }

    #[test]
    fn buffer_memory_copies() {
        let timing = Timing::default();
        let (mut mos, mut flags, mut now) = (Mos::default(), Flags::default(), 0);
        let mut mem = vec![0u16; 1 << 16];
        let mut bm = vec![0u64; 1 << 15];
        for (n, parcel) in mem[0x1000..0x1010].iter_mut().enumerate() {
            *parcel = 0xA000 + n as u16;
        }
        // four words from Local Memory 0x1000 (the low bits of 0x1003 are
        // dropped) to Buffer Memory 0x2A07 = 0x15 * 512 + 7
        for (function, a) in [(0, 0), (3, 7), (2, 0x15), (1, 0x1003), (5, 4)] {
            mos.function(&mut flags, &mut mem, &mut bm, now, &timing, function, a);
        }
        assert!(flags.busy && !flags.done);
        assert_eq!(bm[0x2A07], 0xA000_A001_A002_A003);
        assert_eq!(bm[0x2A0A], 0xA00C_A00D_A00E_A00F);
        settle(&mut flags, &mut now);
        assert_eq!(now, 4 * timing.mos_word as u64);
        // and back to 0x8000; the registers were cleared by the first copy
        for (function, a) in [(0, 0), (3, 7), (2, 0x15), (1, 0x8000), (4, 2)] {
            mos.function(&mut flags, &mut mem, &mut bm, now, &timing, function, a);
        }
        assert_eq!(
            mem[0x8000..0x8009],
            [0xA000, 0xA001, 0xA002, 0xA003, 0xA004, 0xA005, 0xA006, 0xA007, 0]
        );
        // a length of zero is 16,384 words: all of Local Memory, and the
        // Local Memory address wraps
        bm[0] = 0x1111_2222_3333_4444;
        bm[0x3FFF] = 0x5555_6666_7777_8888;
        mos.function(&mut flags, &mut mem, &mut bm, now, &timing, 1, 8);
        mos.function(&mut flags, &mut mem, &mut bm, now, &timing, 4, 0);
        assert_eq!(mem[8..12], [0x1111, 0x2222, 0x3333, 0x4444]);
        assert_eq!(mem[4..8], [0x5555, 0x6666, 0x7777, 0x8888]);
    }

    #[test]
    fn dead_start_load() {
        let timing = Timing::default();
        let (mut mos, mut flags) = (Mos::default(), Flags::default());
        let mut mem = vec![0xFFFFu16; 1 << 16];
        let mut bm = vec![0u64; 1 << 15];
        bm[0] = 0x7003_0000_0000_0001;
        bm[0x3FFF] = 2;
        mos.function(&mut flags, &mut mem, &mut bm, 0, &timing, 1, 0x100);
        mos.dead_start(&mut flags, &mut mem, &mut bm, 0, &timing, false);
        assert!(flags.busy && flags.enable);
        assert_eq!(mem[0], 0x7003);
        assert_eq!(mem[0xFFFF], 2);
        flags.settle(FULL_BLOCK as u64 * timing.mos_word as u64);
        assert!(flags.done);
        // the short one leaves the rest of memory alone
        mem.fill(0xFFFF);
        mos.dead_start(&mut flags, &mut mem, &mut bm, 0, &timing, true);
        assert_eq!(mem[0], 0x7003);
        assert_eq!(mem[4095], 0);
        assert_eq!(mem[4096], 0xFFFF);
    }

    struct Central(Vec<u64>);
    impl CentralMemory for Central {
        fn read(&self, address: u32) -> u64 {
            self.0[address as usize]
        }
        fn write(&mut self, address: u32, word: u64) {
            self.0[address as usize] = word;
        }
    }

    #[test]
    fn high_speed_channel() {
        let timing = Timing::default();
        let (mut out, mut input, mut flags) =
            (HighSpeed::default(), HighSpeed::default(), Flags::default());
        let mut mem = vec![0u16; 1 << 16];
        let mut central = Central(vec![0; 1 << 12]);
        mem[0x800..0x808].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        // two words to central memory 0x280 = 1 * 512 + 0x80
        for (function, a) in [(0, 0), (3, 0x80), (2, 1), (1, 0x800), (5, 2)] {
            out.function(
                &mut flags,
                &mut mem,
                &mut central,
                0,
                &timing,
                false,
                function,
                a,
            );
        }
        assert_eq!(
            central.0[0x280..0x282],
            [0x0001_0002_0003_0004, 0x0005_0006_0007_0008]
        );
        assert!(flags.busy);
        // function 4 is not the output channel's
        flags.idle();
        out.function(&mut flags, &mut mem, &mut central, 0, &timing, false, 4, 2);
        assert!(!flags.busy);
        for (function, a) in [(3, 0x81), (2, 1), (1, 0x900), (4, 1)] {
            input.function(
                &mut flags,
                &mut mem,
                &mut central,
                0,
                &timing,
                true,
                function,
                a,
            );
        }
        assert_eq!(mem[0x900..0x905], [5, 6, 7, 8, 0]);
    }

    /// What the BIOP does with every drive at start: the buffer echo test
    /// and the status register test (devices spec part 8.3).
    #[test]
    fn disk_start_up_tests() {
        let timing = Timing::default();
        let (mut disk, mut flags, mut now) = (
            Dd29::new(Image::empty(DD29_SECTOR_BYTES)),
            Flags::default(),
            0,
        );
        let mut mem = vec![0u16; 1 << 16];
        for (n, parcel) in mem[0x4000..0x4200].iter_mut().enumerate() {
            *parcel = n as u16 ^ 0x5A5A;
        }
        let f =
            |disk: &mut Dd29, flags: &mut Flags, mem: &mut [u16], now: &mut u64, function, a| {
                let value = disk.function(flags, mem, *now, &timing, function, a);
                if flags.busy {
                    settle(flags, now);
                }
                value
            };
        f(&mut disk, &mut flags, &mut mem, &mut now, 0o14, 0x4000);
        f(&mut disk, &mut flags, &mut mem, &mut now, 3, 0);
        assert_eq!(
            f(&mut disk, &mut flags, &mut mem, &mut now, 0o10, 0),
            Some(0x4200)
        );
        f(&mut disk, &mut flags, &mut mem, &mut now, 2, 0);
        assert_eq!(
            f(&mut disk, &mut flags, &mut mem, &mut now, 0o10, 0),
            Some(0x4400)
        );
        assert_eq!(mem[0x4200..0x4400], mem[0x4000..0x4200]);
        assert_eq!(disk.written(), 0);
        f(&mut disk, &mut flags, &mut mem, &mut now, 0o15, 0xBEEF);
        assert_eq!(
            f(&mut disk, &mut flags, &mut mem, &mut now, 0o11, 0),
            Some(0xBEEF)
        );
        // not reserved: the head register reads zero
        f(&mut disk, &mut flags, &mut mem, &mut now, 1, 0o7001);
        assert_eq!(
            f(&mut disk, &mut flags, &mut mem, &mut now, 0o11, 0),
            Some(0)
        );
        f(&mut disk, &mut flags, &mut mem, &mut now, 1, 0o1000);
        f(&mut disk, &mut flags, &mut mem, &mut now, 4, 7);
        f(&mut disk, &mut flags, &mut mem, &mut now, 1, 0o7001);
        assert_eq!(
            f(&mut disk, &mut flags, &mut mem, &mut now, 0o11, 0),
            Some(0o147)
        );
    }

    #[test]
    fn disk_sectors() {
        let timing = Timing::default();
        let (mut disk, mut flags) = (Dd29::new(Image::empty(DD29_SECTOR_BYTES)), Flags::default());
        let mut mem = vec![0u16; 1 << 16];
        mem[0x1000..0x1800].fill(0x1234);
        disk.function(&mut flags, &mut mem, 0, &timing, 1, 0o1000);
        disk.function(&mut flags, &mut mem, 0, &timing, 4, 9);
        disk.function(&mut flags, &mut mem, 0, &timing, 5, 822);
        assert_eq!(
            disk.function(&mut flags, &mut mem, 0, &timing, 0o11, 0),
            Some(822 << 5)
        );
        disk.function(&mut flags, &mut mem, 0, &timing, 0o14, 0x1000);
        disk.function(&mut flags, &mut mem, 0, &timing, 3, 17);
        assert!(flags.busy);
        assert_eq!(
            disk.function(&mut flags, &mut mem, 0, &timing, 0o10, 0),
            Some(0x1800)
        );
        // the last sector of the drive
        assert_eq!(disk.sector(17), 823 * 10 * 18 - 1);
        assert_eq!(disk.written(), 1);
        disk.function(&mut flags, &mut mem, 0, &timing, 0o14, 0x2000);
        disk.function(&mut flags, &mut mem, 0, &timing, 2, 17);
        assert_eq!(mem[0x2000..0x2800], [0x1234; 2048]);
        disk.function(&mut flags, &mut mem, 0, &timing, 0o14, 0x2000);
        disk.function(&mut flags, &mut mem, 0, &timing, 2, 16);
        assert_eq!(mem[0x2000..0x2800], [0; 2048]);
    }

    #[test]
    fn console_keys_and_characters() {
        let timing = Timing::default();
        let (mut console, mut keys, mut display) =
            (Console::default(), Flags::default(), Flags::default());
        console.type_text(&mut keys, 100, b"OK");
        assert!(!keys.done);
        keys.settle(100);
        assert!(keys.done && !keys.busy);
        assert_eq!(
            console.keyboard(&mut keys, 100, &timing, 0o10),
            Some(b'O' as u16)
        );
        // the driver clears the channel behind the read: the next key
        // still comes
        assert_eq!(console.keyboard(&mut keys, 110, &timing, 0), None);
        assert!(!keys.done);
        keys.settle(100 + timing.key as u64);
        assert!(keys.done);
        assert_eq!(
            console.keyboard(&mut keys, 0, &timing, 0o10),
            Some(b'K' as u16)
        );
        assert!(console.idle() && keys.until.is_none());
        console.display(&mut display, 0, &timing, 0o14, 0x80 | b'*' as u16);
        assert!(display.busy);
        display.settle(timing.console_character as u64);
        assert!(display.done && !display.busy);
        assert_eq!(console.output, b"*");
    }

    /// The XIOP's start-up test, as recorded from the cray-sim simulator.
    #[test]
    fn block_multiplexer_registers() {
        let timing = Timing::default();
        let (mut bmx, mut flags) = (BlockMultiplexer::default(), Flags::default());
        let mut f = |function, a| bmx.function(&mut flags, 0, &timing, function, a);
        f(0o16, 0);
        f(0, 0);
        assert_eq!(f(0o13, 0), Some(0x2000));
        assert_eq!(f(0o12, 0), Some(0));
        for (function, a) in [
            (0o14, 0xA5A4),
            (0o14, 0xA5A5),
            (0o15, 0xA5A5),
            (0o15, 0xA5A6),
        ] {
            f(function, a);
        }
        assert_eq!(f(0o10, 0), Some(0xA5A4));
        assert_eq!(f(0o10, 1), Some(0xA5A5));
        assert_eq!(f(0o11, 0), Some(0xA5A6));
        assert_eq!(f(0o11, 0), Some(0xA5A5));
        assert_eq!(f(0o13, 0), Some(0));
    }
}

//! The I/O Subsystem and the mainframe as one system: up to four I/O
//! Processors with their channels, Buffer Memory, the peripherals, and the
//! CPU model on the other end of the MIOP's channel pair.
//!
//! Time is counted in clock periods of 12.5 ns.  Every processor keeps its
//! own count and they are stepped in turn, a few instructions at a time, so
//! that none is more than `QUANTUM` clock periods ahead of another.
//!
//! Channel assignments are those of the surviving COS 1.17 system
//! (`docs/spec/ios-devices-spec.md` part 1.1, "the devices spec"):
//!
//! ```text
//! every IOP   5 Buffer Memory; 6/7, 10/11, 12/13 the other IOPs in ascending order
//! IOP 0 MIOP  16 error log, 17 Peripheral Expander, 20/21 mainframe channels 11/10,
//!             24/25 front-end concentrator (nothing connected), 40/41 to 46/47 consoles
//! IOP 1 BIOP  14/15 central memory, 20 to 37 disk drives, 42/43 console
//! IOP 3 XIOP  20 to 23 block multiplexer channels (no control units), 42/43 console
//! ```

use crate::devices::{BlockMultiplexer, CentralMemory, Console, Dd29, HighSpeed, Mos};
use crate::expander::{Clock, Expander};
use crate::image::{Image, Tape};
use crate::iop::{Channels, Iop};
use crate::replay::Recorder;
use cray_xmp_model::{Machine, StepResult};
use std::io::Write;

/// Clock periods a processor may run ahead of the others.
const QUANTUM: u64 = 32;
/// Channel numbers, octal.
const CHANNELS: usize = 0o50;
const MOS: usize = 5;
const EXB: usize = 0o17;
const CIA: usize = 0o20;
const COA: usize = 0o21;
/// The mainframe's channels on the other end.
const MAINFRAME_INPUT: u32 = 0o10;
const MAINFRAME_OUTPUT: u32 = 0o11;
/// Bits of the external control register of COA, function 4 [HW 7-20].
const HOLD_DISCONNECT: u16 = 1 << 9;
const IO_MASTER_CLEAR: u16 = 1 << 14;
const CPU_MASTER_CLEAR: u16 = 1 << 15;

/// How long things take, in clock periods of 12.5 ns.  The defaults are
/// the real times where the software can tell and much shorter ones where
/// it only waits (a disk, a terminal line).
#[derive(Clone, Debug)]
pub struct Timing {
    /// One step of the CPU model.
    pub cpu_step: u32,
    /// The time an I/O Processor takes for an instruction, in percent of
    /// the real one's (100 or more).  Its real-time clock keeps real time.
    pub iop_percent: u32,
    /// A word of a Buffer Memory copy: 16,384 words take about 2 ms
    /// [HW 5-17].
    pub mos_word: u32,
    /// A word on the 100 Mbyte channel.
    pub high_speed_word: u32,
    /// A parcel on the 6 Mbyte channel pair to the mainframe.
    pub link_parcel: u32,
    /// A character to a console.
    pub console_character: u32,
    /// From one key to the next.
    pub key: u32,
    /// A delayed function of the Peripheral Expander: at least 1
    /// microsecond [HW 7-14].
    pub expander_function: u32,
    /// A tape motion, a disk operation, a character from the clock.
    pub expander_tape: u32,
    pub expander_disk: u32,
    pub expander_clock: u32,
    /// A DD-29: a mode selection, a seek, a sector.
    pub disk_select: u32,
    pub disk_seek: u32,
    pub disk_sector: u32,
    /// Not zero: a seek and a sector take the drive's own times instead
    /// (15 to 80 ms; the sector when it has next passed under the heads).
    pub disk_real: u32,
    /// The delay counter of a block multiplexer channel.
    pub multiplexer_delay: u32,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            cpu_step: 4,
            iop_percent: 100,
            mos_word: 10,
            high_speed_word: 7,
            link_parcel: 27,
            console_character: 2_000,
            key: 400_000,
            expander_function: 80,
            expander_tape: 8_000,
            expander_disk: 8_000,
            expander_clock: 8_000,
            disk_select: 400,
            disk_seek: 16_000,
            disk_sector: 8_000,
            disk_real: 0,
            multiplexer_delay: 8_000,
        }
    }
}

impl Timing {
    /// Set a time by the name of its field.  False if there is no such
    /// field.
    pub fn set(&mut self, name: &str, value: u32) -> bool {
        let field = match name {
            "cpu_step" => &mut self.cpu_step,
            "iop_percent" => &mut self.iop_percent,
            "mos_word" => &mut self.mos_word,
            "high_speed_word" => &mut self.high_speed_word,
            "link_parcel" => &mut self.link_parcel,
            "console_character" => &mut self.console_character,
            "key" => &mut self.key,
            "expander_function" => &mut self.expander_function,
            "expander_tape" => &mut self.expander_tape,
            "expander_disk" => &mut self.expander_disk,
            "expander_clock" => &mut self.expander_clock,
            "disk_select" => &mut self.disk_select,
            "disk_seek" => &mut self.disk_seek,
            "disk_sector" => &mut self.disk_sector,
            "disk_real" => &mut self.disk_real,
            "multiplexer_delay" => &mut self.multiplexer_delay,
            _ => return false,
        };
        *field = value;
        true
    }

    /// Every channel operation is over as soon as it is asked for, as in
    /// the cray-sim simulator: for comparing with a boot recorded there.
    pub fn instant() -> Timing {
        Timing {
            cpu_step: 4,
            iop_percent: 100,
            mos_word: 0,
            high_speed_word: 0,
            link_parcel: 0,
            console_character: 0,
            key: 400_000,
            expander_function: 0,
            expander_tape: 0,
            expander_disk: 0,
            expander_clock: 0,
            disk_select: 0,
            disk_seek: 0,
            disk_sector: 0,
            disk_real: 0,
            multiplexer_delay: 0,
        }
    }
}

/// The flags of a channel: Busy, Done, Interrupt Enable [HW 5-2], and when
/// the operation under way ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct Flags {
    pub busy: bool,
    pub done: bool,
    pub enable: bool,
    /// At this time Busy clears and Done sets.
    pub until: Option<u64>,
}

impl Flags {
    /// Function 0 of most channels: clear Busy and Done, abandon what was
    /// under way.
    pub fn idle(&mut self) {
        self.busy = false;
        self.done = false;
        self.until = None;
    }
    /// An operation that takes `delay` clock periods.
    pub fn start(&mut self, now: u64, delay: u32) {
        self.busy = true;
        self.done = false;
        self.until = Some(now + delay as u64);
    }
    /// The operation is complete.
    pub fn finish(&mut self) {
        self.busy = false;
        self.done = true;
        self.until = None;
    }
    pub fn settle(&mut self, now: u64) {
        if self.until.is_some_and(|t| now >= t) {
            self.finish();
        }
    }
}

/// What is on a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Empty,
    Mos,
    /// From, and to, another IOP: the first, second or third other one.
    LinkIn(usize),
    LinkOut(usize),
    ErrorLog,
    Expander,
    /// From and to the mainframe.
    MainframeIn,
    MainframeOut,
    /// From and to a front end that is not there.
    ConcentratorIn,
    ConcentratorOut,
    CentralIn,
    CentralOut,
    Disk(usize),
    Multiplexer(usize),
    Keyboard(usize),
    Display(usize),
}

/// One end of the MIOP's channel pair to the mainframe (and of the pair to
/// the concentrator): CIA or COA [HW 7-15 to 7-21].
#[derive(Default)]
struct LowSpeed {
    /// Local Memory address of the next parcel.
    address: u16,
    /// Parcels still to move.
    count: u32,
    /// Output: the parcel at `address` is on the lines with Ready and has
    /// not been answered.
    waiting: bool,
    /// Output: the external control lines of function 4.
    control: u16,
    /// The earliest time of the next parcel.
    next: u64,
}

/// The channels of one IOP.
struct Local {
    flags: [Flags; CHANNELS],
    kind: [Kind; CHANNELS],
    mos: Mos,
    /// The control register of each output channel to another IOP: bit 0
    /// Master Clear, bit 1 dead start, bit 3 short transfer [HW 5-17].
    control: [u16; 3],
    consoles: [Console; 4],
    /// The flags changed since `request` was worked out.
    dirty: bool,
    /// The earliest time at which an operation under way ends.
    next_event: u64,
    /// The lowest numbered channel that asks for an interrupt.
    request: Option<u8>,
}

/// Something one processor does to another, carried out when its
/// instruction is over.
enum Action {
    MasterClear(usize),
    DeadStart(usize, bool),
}

/// The CPU model as the 100 Mbyte channel sees it.
struct Central<'a>(&'a mut Machine);

impl CentralMemory for Central<'_> {
    fn read(&self, address: u32) -> u64 {
        self.0.mem(address).unwrap_or(0)
    }
    fn write(&mut self, address: u32, word: u64) {
        self.0.store(address, Some(word));
    }
}

/// Everything but the I/O Processors themselves: what their channels lead
/// to.
struct Side {
    timing: Timing,
    /// The processor that is being stepped, and its time.
    current: usize,
    now: u64,
    present: [bool; 4],
    local: [Local; 4],
    /// Buffer Memory.
    bm: Vec<u64>,
    /// The data register from one IOP to another.
    link: [[u16; 4]; 4],
    expander: Expander,
    from_mainframe: LowSpeed,
    to_mainframe: LowSpeed,
    concentrator: [LowSpeed; 2],
    central: [HighSpeed; 2],
    disks: Vec<Dd29>,
    multiplexers: [BlockMultiplexer; 4],
    cpu: Machine,
    /// CPU Master Clear is on: the CPU does nothing.
    cpu_held: bool,
    /// Why the CPU model stopped, if it did.
    cpu_stopped: Option<String>,
    /// Times CPU Master Clear was released.
    cpu_starts: u32,
    actions: Vec<Action>,
}

/// The other IOPs as IOP `n` numbers them: in ascending order.
fn others(n: usize) -> [usize; 3] {
    let mut out = [0; 3];
    for (slot, p) in (0..4).filter(|&p| p != n).enumerate() {
        out[slot] = p;
    }
    out
}

/// The channels of IOP `n` that lead to its `slot`th other IOP.
fn link_in(slot: usize) -> usize {
    6 + 2 * slot
}
fn link_out(slot: usize) -> usize {
    7 + 2 * slot
}

impl Side {
    /// Bring the flags of IOP `n` up to its time and work out which channel
    /// asks for an interrupt.
    fn settle(&mut self, n: usize) {
        let l = &mut self.local[n];
        if !l.dirty && self.now < l.next_event {
            return;
        }
        let mut next = u64::MAX;
        for flags in l.flags.iter_mut() {
            flags.settle(self.now);
            if let Some(t) = flags.until {
                next = next.min(t);
            }
        }
        if n == 0 {
            self.expander.settle(self.now);
            if let Some(t) = self.expander.next_event() {
                next = next.min(t);
            }
        }
        l.next_event = next;
        l.dirty = false;
        let expander = &self.expander;
        l.request = (MOS..CHANNELS)
            .find(|&c| match l.kind[c] {
                Kind::Expander => expander.request(&l.flags[c]),
                _ => l.flags[c].done && l.flags[c].enable,
            })
            .map(|c| c as u8);
    }

    /// Master Clear of IOP `n` reaches every one of its channel interfaces
    /// [HW 5-4].  The Interrupt Enable flags are cleared here; the manual
    /// does not give their state and the kernel sets every one it uses.
    fn master_clear(&mut self, n: usize) {
        let l = &mut self.local[n];
        l.flags = [Flags::default(); CHANNELS];
        l.mos = Mos::default();
        l.control = [0; 3];
        l.dirty = true;
        match n {
            0 => {
                self.expander.master_clear();
                self.from_mainframe = LowSpeed::default();
                // the lines to the mainframe keep what they carry
                self.to_mainframe = LowSpeed {
                    control: self.to_mainframe.control,
                    ..LowSpeed::default()
                };
                self.concentrator = Default::default();
            }
            1 => {
                self.central = Default::default();
                for disk in self.disks.iter_mut() {
                    disk.master_clear();
                }
            }
            3 => self.multiplexers = Default::default(),
            _ => {}
        }
    }

    /// The external control lines from the MIOP to the mainframe [HW 7-20;
    /// CSM-0111000 3-21].  CPU Master Clear holds the CPU; its release is
    /// the dead start, an exchange with the package at address 0.  I/O
    /// Master Clear stops every channel of the mainframe.
    fn mainframe_control(&mut self, lines: u16) {
        let before = std::mem::replace(&mut self.to_mainframe.control, lines);
        if (before | lines) & IO_MASTER_CLEAR != 0 {
            self.cpu.io_master_clear();
            self.to_mainframe.waiting = false;
        }
        if lines & CPU_MASTER_CLEAR != 0 {
            self.cpu_held = true;
        } else if before & CPU_MASTER_CLEAR != 0 {
            self.cpu.dead_start();
            self.cpu_held = false;
            self.cpu_stopped = None;
            self.cpu_starts += 1;
        }
    }

    /// Move a parcel each way between the MIOP and the mainframe, if one is
    /// due.  `mem` is the MIOP's Local Memory.
    fn link(&mut self, mem: &mut [u16], now: u64) {
        let flags = &mut self.local[0].flags;
        // COA to the mainframe's input channel.  A Ready that finds that
        // channel stopped is held there, and the parcel is answered when
        // the program starts the channel.
        let out = &mut self.to_mainframe;
        if flags[COA].busy && now >= out.next {
            let answered = if out.waiting {
                self.cpu.channel_resumed(MAINFRAME_INPUT)
            } else {
                out.waiting = true;
                self.cpu
                    .channel_input(MAINFRAME_INPUT, mem[out.address as usize])
            };
            if answered {
                out.waiting = false;
                out.address = out.address.wrapping_add(1);
                out.count = out.count.saturating_sub(1);
                out.next = now + self.timing.link_parcel as u64;
                if out.count == 0 {
                    if out.control & HOLD_DISCONNECT == 0 {
                        self.cpu.channel_disconnect(MAINFRAME_INPUT);
                    }
                    flags[COA].finish();
                    self.local[0].dirty = true;
                }
            }
        }
        // the mainframe's output channel to CIA.  A parcel that comes
        // while CIA is idle waits on the lines.
        let flags = &mut self.local[0].flags;
        let input = &mut self.from_mainframe;
        if flags[CIA].busy && now >= input.next {
            if let Some(parcel) = self.cpu.channel_output(MAINFRAME_OUTPUT) {
                mem[input.address as usize] = parcel;
                input.address = input.address.wrapping_add(1);
                input.count = input.count.saturating_sub(1);
                input.next = now + self.timing.link_parcel as u64;
                let disconnect = self.cpu.channel_resume(MAINFRAME_OUTPUT);
                if input.count == 0 || disconnect {
                    flags[CIA].finish();
                    self.local[0].dirty = true;
                }
            }
        }
    }
}

impl Channels for Side {
    fn function(&mut self, mem: &mut [u16], channel: u8, function: u8, a: u16) -> Option<u16> {
        let (n, now, c) = (self.current, self.now, channel as usize);
        self.settle(n);
        let l = &mut self.local[n];
        l.dirty = true;
        let kind = l.kind[c];
        let flags = &mut l.flags[c];
        // functions 6 and 7 clear and set Interrupt Enable on every
        // interface but the expander's [HW 7-1]
        if kind != Kind::Expander && (function == 6 || function == 7) {
            flags.enable = function == 7;
            return None;
        }
        let timing = &self.timing;
        match kind {
            Kind::Empty => None,
            Kind::Mos => {
                l.mos
                    .function(flags, mem, &mut self.bm, now, timing, function, a);
                None
            }
            // the input channel from another IOP [HW 5-16]: reading the
            // word empties the register and tells the sender, whether or
            // not a word was there
            Kind::LinkIn(slot) => {
                let from = others(n)[slot];
                match function {
                    0 => flags.idle(),
                    0o10 => {
                        flags.done = false;
                        if self.present[from] {
                            let back = others(from).iter().position(|&p| p == n).unwrap();
                            let sender = &mut self.local[from];
                            sender.flags[link_out(back)].finish();
                            sender.dirty = true;
                        }
                        return Some(self.link[from][n]);
                    }
                    _ => {}
                }
                None
            }
            // the output channel to another IOP [HW 5-17, 5-18]
            Kind::LinkOut(slot) => {
                let to = others(n)[slot];
                match function {
                    0 => flags.idle(),
                    // Master Clear and dead start of the other IOP: it
                    // loads and starts when Master Clear drops with the
                    // dead start bit set
                    1 => {
                        let before = std::mem::replace(&mut l.control[slot], a & 0o17);
                        if self.present[to] {
                            if a & 1 != 0 && before & 1 == 0 {
                                self.actions.push(Action::MasterClear(to));
                            }
                            if a & 1 == 0 && before & 3 == 3 {
                                self.actions.push(Action::DeadStart(to, before & 0o10 != 0));
                            }
                        }
                    }
                    0o14 => {
                        flags.busy = true;
                        flags.done = false;
                        self.link[n][to] = a;
                        if self.present[to] {
                            let back = others(to).iter().position(|&p| p == n).unwrap();
                            let receiver = &mut self.local[to];
                            receiver.flags[link_in(back)].done = true;
                            receiver.dirty = true;
                        }
                    }
                    _ => {}
                }
                None
            }
            // nothing is ever logged [HW 7-41 to 7-47]
            Kind::ErrorLog => {
                if function == 0 {
                    flags.idle();
                }
                (0o10..=0o13).contains(&function).then_some(0)
            }
            Kind::Expander => self.expander.function(flags, mem, now, timing, function, a),
            // CIA [HW 7-15 to 7-17]: the parcels move in `link`
            Kind::MainframeIn | Kind::ConcentratorIn => {
                let end = if kind == Kind::MainframeIn {
                    &mut self.from_mainframe
                } else {
                    &mut self.concentrator[0]
                };
                match function {
                    0 => flags.idle(),
                    1 => {
                        end.address = a;
                        if end.count == 0 {
                            end.count = 1 << 16;
                        }
                        flags.busy = true;
                        flags.done = false;
                    }
                    2 => end.count = a as u32,
                    // the address after the last parcel stored; no parity
                    // errors, and no Ready is reported as waiting
                    0o10 => return Some(end.address),
                    0o11 => return Some(0),
                    _ => {}
                }
                None
            }
            // COA [HW 7-18 to 7-21]
            Kind::MainframeOut | Kind::ConcentratorOut => {
                let mainframe = kind == Kind::MainframeOut;
                let end = if mainframe {
                    &mut self.to_mainframe
                } else {
                    &mut self.concentrator[1]
                };
                match function {
                    0 => flags.idle(),
                    1 => {
                        end.address = a;
                        end.waiting = false;
                        if end.count == 0 {
                            end.count = 1 << 16;
                        }
                        if mainframe {
                            flags.busy = true;
                            flags.done = false;
                        } else {
                            // nobody takes the parcels: they go out all the same
                            let parcels = end.count;
                            end.address = a.wrapping_add(parcels as u16);
                            end.count = 0;
                            flags.start(now, parcels * timing.link_parcel);
                        }
                    }
                    2 => end.count = a as u32,
                    4 if mainframe => self.mainframe_control(a),
                    0o10 => return Some(end.address),
                    0o11 => return Some(0),
                    _ => {}
                }
                None
            }
            Kind::CentralIn | Kind::CentralOut => {
                let input = kind == Kind::CentralIn;
                self.central[!input as usize].function(
                    flags,
                    mem,
                    &mut Central(&mut self.cpu),
                    now,
                    timing,
                    input,
                    function,
                    a,
                );
                None
            }
            Kind::Disk(d) => self.disks[d].function(flags, mem, now, timing, function, a),
            Kind::Multiplexer(m) => self.multiplexers[m].function(flags, now, timing, function, a),
            Kind::Keyboard(k) => l.consoles[k].keyboard(flags, now, timing, function),
            Kind::Display(k) => {
                l.consoles[k].display(flags, now, timing, function, a);
                None
            }
        }
    }

    fn busy(&mut self, channel: u8) -> bool {
        self.settle(self.current);
        self.local[self.current].flags[channel as usize].busy
    }

    fn done(&mut self, channel: u8) -> bool {
        self.settle(self.current);
        self.local[self.current].flags[channel as usize].done
    }

    fn interrupt(&mut self) -> Option<u8> {
        self.settle(self.current);
        self.local[self.current].request
    }
}

/// What a system is made of.
pub struct Config {
    /// The IOP kernel: parcels, high byte first.  It is put into Buffer
    /// Memory at address 0, from where the MIOP is dead started.
    pub kernel: Vec<u8>,
    /// The tape on the Peripheral Expander's drive.
    pub tape: Tape,
    /// The disk on the Peripheral Expander.
    pub expander_disk: Image,
    /// DD-29 drives: the BIOP channel (20 to 37 octal) and the image.
    pub disks: Vec<(u8, Image)>,
    /// Words of Buffer Memory.
    pub buffer_memory_words: usize,
    /// The date `YYMMDD` and the time `HHMMSS` of a clock on the
    /// Peripheral Expander, if there is one.
    pub clock: Option<(String, String)>,
    /// Which of the four IOPs exist.
    pub iops: [bool; 4],
    /// Channels to leave with nothing on them: the IOP and the channel
    /// number.  For finding out what the software does without a device.
    pub without: Vec<(usize, u8)>,
    pub timing: Timing,
}

impl Config {
    /// The system of the surviving COS 1.17 software: MIOP, BIOP and XIOP,
    /// four million words of Buffer Memory, no clock, no disks yet.
    pub fn new(kernel: Vec<u8>, tape: Tape, expander_disk: Image) -> Config {
        Config {
            kernel,
            tape,
            expander_disk,
            disks: Vec::new(),
            buffer_memory_words: 1 << 22,
            clock: None,
            iops: [true, true, false, true],
            without: Vec::new(),
            timing: Timing::default(),
        }
    }

    /// Change a parcel of the kernel before it is loaded.
    pub fn poke_kernel(&mut self, parcel: usize, value: u16) {
        self.kernel[2 * parcel..2 * parcel + 2].copy_from_slice(&value.to_be_bytes());
    }

    /// Give the system a clock.  The kernel and the overlay XCLOCK of this
    /// software were built for a machine without one; the five parcels the
    /// cray-sim configuration changes turn its driver on (devices spec part
    /// 13).  False if the tape is not that software's.
    pub fn with_clock(&mut self, date: &str, time: &str) -> bool {
        self.clock = Some((date.to_string(), time.to_string()));
        self.poke_kernel(0x3939, 0o50);
        [0x3D24B, 0x3D6A3, 0x3D7C3, 0x3D399, 0x3D49B]
            .iter()
            .all(|&byte| self.tape.poke(0, byte, 0o50))
    }
}

/// The system.
pub struct System {
    iop: [Iop; 4],
    /// The time of each IOP and of the CPU, in clock periods.
    time: [u64; 4],
    cpu_time: u64,
    clock: u64,
    side: Side,
    log: Option<Box<dyn Write>>,
    /// The IOP whose steps are recorded, and the record.
    replay: Option<(usize, Recorder)>,
}

impl System {
    /// The system at power-on: the mainframe held by its Master Clear, the
    /// MIOP loading the kernel from Buffer Memory, the other IOPs held
    /// until the MIOP starts them.
    pub fn new(config: Config) -> System {
        let mut bm = vec![0u64; config.buffer_memory_words];
        for (n, parcel) in config.kernel.chunks(2).enumerate() {
            let parcel = (parcel[0] as u64) << 8 | *parcel.get(1).unwrap_or(&0) as u64;
            bm[n / 4] |= parcel << (48 - 16 * (n % 4));
        }
        let mut disks = Vec::new();
        let local = [0, 1, 2, 3].map(|n| {
            let mut kind = [Kind::Empty; CHANNELS];
            kind[MOS] = Kind::Mos;
            for slot in 0..3 {
                kind[link_in(slot)] = Kind::LinkIn(slot);
                kind[link_out(slot)] = Kind::LinkOut(slot);
            }
            // the consoles: four on the MIOP, the second of them alone on
            // the others
            for k in 0..4 {
                if n == 0 || k == 1 {
                    kind[0o40 + 2 * k] = Kind::Keyboard(k);
                    kind[0o41 + 2 * k] = Kind::Display(k);
                }
            }
            match n {
                0 => {
                    kind[0o16] = Kind::ErrorLog;
                    kind[EXB] = Kind::Expander;
                    kind[CIA] = Kind::MainframeIn;
                    kind[COA] = Kind::MainframeOut;
                    kind[0o24] = Kind::ConcentratorIn;
                    kind[0o25] = Kind::ConcentratorOut;
                }
                1 => {
                    kind[0o14] = Kind::CentralIn;
                    kind[0o15] = Kind::CentralOut;
                }
                3 => {
                    for m in 0..4 {
                        kind[0o20 + m] = Kind::Multiplexer(m);
                    }
                }
                _ => {}
            }
            Local {
                flags: [Flags::default(); CHANNELS],
                kind,
                mos: Mos::default(),
                control: [0; 3],
                consoles: Default::default(),
                dirty: true,
                next_event: 0,
                request: None,
            }
        });
        let mut local = local;
        for (channel, image) in config.disks {
            assert!(
                (0o20..0o40).contains(&channel),
                "a disk is on a BIOP channel from 20 to 37 octal"
            );
            local[1].kind[channel as usize] = Kind::Disk(disks.len());
            disks.push(Dd29::new(image));
        }
        for (n, channel) in config.without {
            local[n].kind[channel as usize] = Kind::Empty;
        }
        let mut cpu = Machine::new();
        cpu.set_system();
        cpu.set_timing(config.timing.cpu_step);
        let clock = config.clock.map(|(date, time)| Clock::new(&date, &time));
        let mut system = System {
            iop: [Iop::new(), Iop::new(), Iop::new(), Iop::new()],
            time: [0; 4],
            cpu_time: 0,
            clock: 0,
            side: Side {
                timing: config.timing,
                current: 0,
                now: 0,
                present: config.iops,
                local,
                bm,
                link: [[0; 4]; 4],
                expander: Expander::new(config.tape, config.expander_disk, clock),
                from_mainframe: LowSpeed::default(),
                to_mainframe: LowSpeed {
                    control: CPU_MASTER_CLEAR,
                    ..LowSpeed::default()
                },
                concentrator: Default::default(),
                central: Default::default(),
                disks,
                multiplexers: Default::default(),
                cpu,
                cpu_held: true,
                cpu_stopped: None,
                cpu_starts: 0,
                actions: Vec::new(),
            },
            log: None,
            replay: None,
        };
        system.dead_start(0, false);
        system
    }

    /// Write a line for every channel function and every interrupt taken
    /// to `log`, in the form of the boot that was recorded on the cray-sim
    /// simulator (`tests/boot.rs`):
    ///
    /// ```text
    /// F iop P channel function A value busy-done      (hex; channel and function octal)
    /// I iop P channel held
    /// ```
    pub fn set_log(&mut self, log: Option<Box<dyn Write>>) {
        self.log = log;
    }

    /// Record every step of IOP `n` on `out`, in the form of the module
    /// `replay`, from now until `end_replay`.  The processor should not
    /// have run yet.
    pub fn set_replay(&mut self, n: usize, out: Box<dyn Write>) {
        let mut recorder = Recorder::new(out);
        recorder.sync(&self.iop[n]);
        self.replay = Some((n, recorder));
    }

    /// End the record, with the state the processor is in.  Returns the
    /// number of steps recorded.
    pub fn end_replay(&mut self) -> u64 {
        match self.replay.take() {
            Some((n, mut recorder)) => {
                recorder.finish(&self.iop[n]);
                recorder.steps()
            }
            None => 0,
        }
    }

    /// Steps recorded so far.
    pub fn replay_steps(&self) -> u64 {
        self.replay
            .as_ref()
            .map_or(0, |(_, recorder)| recorder.steps())
    }

    /// Master Clear IOP `n` and load it from Buffer Memory.
    fn dead_start(&mut self, n: usize, short: bool) {
        self.iop[n].master_clear();
        self.side.master_clear(n);
        let side = &mut self.side;
        let l = &mut side.local[n];
        l.mos.dead_start(
            &mut l.flags[MOS],
            self.iop[n].memory_mut(),
            &mut side.bm,
            self.time[n],
            &side.timing,
            short,
        );
        l.dirty = true;
        if let Some((recorded, recorder)) = &mut self.replay {
            if *recorded == n {
                recorder.master_clear();
                recorder.sync(&self.iop[n]);
            }
        }
    }

    /// One instruction, or one interrupt, of IOP `n`.
    fn step(&mut self, n: usize) {
        let (iop, side) = (&mut self.iop[n], &mut self.side);
        side.current = n;
        side.now = self.time[n];
        if iop.held() && side.interrupt().is_none() {
            // Master Clear: nothing happens until the dead start interrupt
            iop.advance(QUANTUM as u32);
            self.time[n] += QUANTUM;
            return;
        }
        // a slower processor: the rest of the instruction's time passes
        // with nothing happening
        let slower = (side.timing.iop_percent.max(100) - 100) as u64;
        let mut took = |iop: &mut Iop, clock_periods: u32| {
            let rest = clock_periods as u64 * slower / 100;
            iop.advance(rest as u32);
            self.time[n] += clock_periods as u64 + rest;
        };
        // what the function log needs from before the step
        let (p, a, held) = (iop.p(), iop.a(), iop.held());
        let parcel = iop.memory()[p as usize];
        let (f, d) = (parcel >> 9, parcel & 0o777);
        let channel = if f < 0o160 { d } else { iop.b() };
        let request = match self.log {
            Some(_) => iop.interrupt_request(side),
            None => None,
        };
        let before = iop.counters();
        let clock_periods = match &mut self.replay {
            Some((recorded, recorder)) if *recorded == n => recorder.step(iop, side),
            _ => iop.step(side),
        };
        took(iop, clock_periods);
        if let Some(log) = &mut self.log {
            let after = iop.counters();
            if after.interrupts != before.interrupts {
                let _ = writeln!(
                    log,
                    "I {} {:04x} {:02o} {}",
                    n,
                    p,
                    request.unwrap_or(0),
                    held as u8
                );
            } else if f >= 0o140 && after.instructions != before.instructions {
                let function = f & 0o17;
                let value = if (0o10..=0o13).contains(&function) {
                    iop.a()
                } else {
                    0
                };
                let (busy, done) = match channel {
                    0..=4 => (false, iop.done(channel)),
                    5..=0o47 => (side.busy(channel as u8), side.done(channel as u8)),
                    _ => (false, false),
                };
                let _ = writeln!(
                    log,
                    "F {} {:04x} {:02o} {:02o} {:04x} {:04x} {}{}",
                    n, p, channel, function, a, value, busy as u8, done as u8
                );
            }
        }
        while let Some(action) = self.side.actions.pop() {
            match action {
                Action::MasterClear(to) => {
                    self.iop[to].master_clear();
                    self.side.master_clear(to);
                    if let Some((recorded, recorder)) = &mut self.replay {
                        if *recorded == to {
                            recorder.master_clear();
                        }
                    }
                }
                Action::DeadStart(to, short) => {
                    self.time[to] = self.time[to].max(self.time[n]);
                    self.dead_start(to, short);
                }
            }
        }
    }

    /// Run the system for `clock_periods`.
    pub fn run(&mut self, clock_periods: u64) {
        let end = self.clock + clock_periods;
        while self.clock < end {
            self.clock += QUANTUM;
            for n in 0..4 {
                if self.side.present[n] {
                    while self.time[n] < self.clock {
                        self.step(n);
                    }
                }
            }
            let side = &mut self.side;
            if side.cpu_held && self.cpu_time < self.clock {
                side.cpu.advance(self.clock - self.cpu_time);
                self.cpu_time = self.clock;
            }
            while self.cpu_time < self.clock {
                self.cpu_time += side.timing.cpu_step as u64;
                match side.cpu.step() {
                    StepResult::Running => {}
                    StepResult::Exit(code) => {
                        side.cpu_stopped = Some(format!(
                            "the program wrote {:#o} to the test exit word",
                            code
                        ));
                        side.cpu_held = true;
                    }
                    StepResult::Error(error) => {
                        side.cpu_stopped = Some(error.to_string());
                        side.cpu_held = true;
                    }
                }
                if side.cpu_held {
                    break;
                }
            }
            if side.local[0].flags[CIA].busy || side.local[0].flags[COA].busy {
                side.link(self.iop[0].memory_mut(), self.clock);
                // parcels from the mainframe came into Local Memory
                if let Some((0, recorder)) = &mut self.replay {
                    recorder.sync(&self.iop[0]);
                }
            }
        }
    }

    // ---- looking in

    /// Clock periods since power-on.
    pub fn time(&self) -> u64 {
        self.clock
    }
    /// I/O Processor `n`.
    pub fn iop(&self, n: usize) -> &Iop {
        &self.iop[n]
    }
    /// The mainframe.
    pub fn cpu(&self) -> &Machine {
        &self.side.cpu
    }
    /// True while CPU Master Clear is on, or the CPU model has stopped.
    pub fn cpu_held(&self) -> bool {
        self.side.cpu_held
    }
    /// Why the CPU model stopped: it met something it does not model.
    pub fn cpu_stopped(&self) -> Option<&str> {
        self.side.cpu_stopped.as_deref()
    }
    /// Times the MIOP has released CPU Master Clear.
    pub fn cpu_starts(&self) -> u32 {
        self.side.cpu_starts
    }
    /// Buffer Memory.
    pub fn buffer_memory(&self) -> &[u64] {
        &self.side.bm
    }
    /// The flags of a channel of IOP `n`, as they were when it last looked.
    pub fn channel(&self, n: usize, channel: usize) -> Flags {
        self.side.local[n].flags[channel]
    }
    /// What has been sent to console `k` (0 to 3: channels 41, 43, 45 and
    /// 47) of IOP `n`.
    pub fn console(&self, n: usize, k: usize) -> &[u8] {
        &self.side.local[n].consoles[k].output
    }
    /// Press keys on console `k` of IOP `n`.
    pub fn type_text(&mut self, n: usize, k: usize, text: &[u8]) {
        let l = &mut self.side.local[n];
        l.consoles[k].type_text(&mut l.flags[0o40 + 2 * k], self.time[n], text);
        l.dirty = true;
        l.next_event = 0;
    }
    /// True if every key typed on console `k` of IOP `n` has been read.
    pub fn typed(&self, n: usize, k: usize) -> bool {
        self.side.local[n].consoles[k].idle()
    }
    /// What the printer has printed.
    pub fn printed(&self) -> &[u8] {
        self.side.expander.printed()
    }
    /// Sectors written to the expander disk and to each DD-29.
    pub fn written(&self) -> (usize, Vec<usize>) {
        (
            self.side.expander.disk_written(),
            self.side.disks.iter().map(|d| d.written()).collect(),
        )
    }
    /// The expander disk as it now is, as a new file.
    pub fn save_expander_disk(&self, to: &std::path::Path) -> std::io::Result<()> {
        self.side.expander.save_disk(to)
    }
    /// DD-29 `index`, in the order of `Config::disks`, as it now is, as a
    /// new file.
    pub fn save_disk(&self, index: usize, to: &std::path::Path) -> std::io::Result<()> {
        self.side.disks[index].save(to)
    }
}

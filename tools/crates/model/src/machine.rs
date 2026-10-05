//! Machine state, memory, the I/O page, the exchange sequence and the
//! fetch/issue loop.  Instruction semantics are in `exec.rs` and `vector.rs`.

use crate::event::{Event, Observer};
use cray1_isa::{decode, Decoded};
use std::collections::VecDeque;
use std::fmt;

/// Number of 64-bit memory words: one million words, a 20-bit word address.
pub const MEMORY_WORDS: u32 = 1 << 20;
/// First word of the invented I/O page (the top 16 words of memory).
pub const IO_PAGE: u32 = MEMORY_WORDS - 16;
/// Console status: bit 0 = an input character is waiting, bit 1 = output
/// ready (always 1 in the model).
pub const CON_STAT: u32 = IO_PAGE;
/// Console data: a read takes the next input character (0 if none), a write
/// outputs the low 8 bits.
pub const CON_DATA: u32 = IO_PAGE + 1;
/// A write ends the run; the value is the exit code.  Reads 0.
pub const TEST_EXIT: u32 = IO_PAGE + 2;
/// A clock counter.  The model cannot predict it: a read is undefined.
pub const CYCLES: u32 = IO_PAGE + 3;

/// Mask of the 22-bit P register (a parcel address).
pub const P_MASK: u32 = (1 << 22) - 1;
/// Mask of a 24-bit A or B register.
pub const A_MASK: u32 = (1 << 24) - 1;

/// Bits of the M (mode) register, HRM page 3-35.  The register is the 4-bit
/// field at bits 36 to 39 of word 2 of the exchange package, bit 39 (monitor
/// mode) being the least significant.
pub mod mode {
    /// Bit 39: monitor mode.
    pub const MONITOR: u8 = 0o01;
    /// Bit 38: interrupt on uncorrectable memory error.
    pub const UNCORRECTABLE_MEMORY: u8 = 0o02;
    /// Bit 37: interrupt on floating point error.
    pub const FLOATING_POINT: u8 = 0o04;
    /// Bit 36: interrupt on correctable memory error.
    pub const CORRECTABLE_MEMORY: u8 = 0o10;
}

/// Bits of the F (flag) register, HRM page 3-37 (figure 3-8).  The register
/// is the 9-bit field at bits 31 to 39 of word 3 of the exchange package,
/// bit 39 (normal exit) being the least significant.
pub mod flag {
    /// Bit 39: normal exit (004).
    pub const NORMAL_EXIT: u16 = 0o001;
    /// Bit 38: error exit (000).
    pub const ERROR_EXIT: u16 = 0o002;
    /// Bit 37: I/O interrupt.  Never raised: no channels are attached.
    pub const IO_INTERRUPT: u16 = 0o004;
    /// Bit 36: memory error.  Never raised by the model.
    pub const MEMORY_ERROR: u16 = 0o010;
    /// Bit 35: program range error.
    pub const PROGRAM_RANGE: u16 = 0o020;
    /// Bit 34: operand range error.
    pub const OPERAND_RANGE: u16 = 0o040;
    /// Bit 33: floating point error.
    pub const FLOATING_POINT: u16 = 0o100;
    /// Bit 32: real-time clock interrupt.  Never raised by the model.
    pub const RTC_INTERRUPT: u16 = 0o200;
    /// Bit 31: console interrupt.  Never raised by the model.
    pub const CONSOLE_INTERRUPT: u16 = 0o400;
}

/// Why a run stopped with a test error (exit status 3 of `cray1-run`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Control flow, an address, a shift count, VL, a block length, an I/O
    /// write or an instruction parcel depended on an undefined value.
    UndefinedValue,
    /// The program did something whose outcome the manual does not define
    /// (0023xx to 0027xx, an instruction fetch outside the field in monitor
    /// mode).
    NotDefinedByManual,
}

/// A test error: the model stopped because it cannot predict what the
/// machine does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestError {
    pub kind: ErrorKind,
    /// P of the instruction (or of the fetch) that failed.
    pub p: u32,
    /// The parcels of the instruction, if they were fetched.
    pub parcels: Option<(u16, Option<u16>)>,
    /// What was undefined or not defined.
    pub detail: String,
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let kind = match self.kind {
            ErrorKind::UndefinedValue => "undefined value used",
            ErrorKind::NotDefinedByManual => "not defined by the manual",
        };
        write!(f, "{}: {} at P={:08o}", kind, self.detail, self.p)?;
        match self.parcels {
            Some((p0, Some(p1))) => {
                write!(
                    f,
                    " ({:06o} {:06o}  {})",
                    p0,
                    p1,
                    cray1_isa::disassemble(&decode(p0, Some(p1)))
                )
            }
            Some((p0, None)) => write!(
                f,
                " ({:06o}  {})",
                p0,
                cray1_isa::disassemble(&decode(p0, None))
            ),
            None => Ok(()),
        }
    }
}

impl std::error::Error for TestError {}

/// The result of one `Machine::step`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepResult {
    /// The step completed and the machine can go on.
    Running,
    /// TEST_EXIT was written with this value; the run is over.
    Exit(u64),
    /// A test error; the run is over.
    Error(TestError),
}

/// The result of `Machine::run`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunResult {
    /// TEST_EXIT was written with this value.
    Exit(u64),
    /// The step limit was reached without an exit.
    Limit,
    /// A test error.
    Error(TestError),
}

impl RunResult {
    /// The process exit status `cray1-run` uses: the exit code saturated at
    /// 255, 2 for no exit, 3 for a test error.
    pub fn exit_status(&self) -> u8 {
        match self {
            RunResult::Exit(code) => (*code).min(255) as u8,
            RunResult::Limit => 2,
            RunResult::Error(_) => 3,
        }
    }
}

/// Memory with a "defined" bit and a "written by the run" bit per word.
pub(crate) struct Memory {
    words: Vec<u64>,
    defined: Vec<u64>,
    written: Vec<u64>,
}

impl Memory {
    fn new() -> Memory {
        let n = MEMORY_WORDS as usize;
        Memory {
            words: vec![0; n],
            defined: vec![0; n / 64],
            written: vec![0; n / 64],
        }
    }
    #[inline]
    pub(crate) fn get(&self, addr: u32) -> Option<u64> {
        let a = addr as usize;
        (self.defined[a / 64] >> (a % 64) & 1 != 0).then(|| self.words[a])
    }
    #[inline]
    fn put(&mut self, addr: u32, value: Option<u64>) {
        let a = addr as usize;
        self.words[a] = value.unwrap_or(0);
        match value {
            Some(_) => self.defined[a / 64] |= 1 << (a % 64),
            None => self.defined[a / 64] &= !(1 << (a % 64)),
        }
    }
    #[inline]
    fn mark_written(&mut self, addr: u32) {
        let a = addr as usize;
        self.written[a / 64] |= 1 << (a % 64);
    }
    fn is_written(&self, addr: u32) -> bool {
        let a = addr as usize;
        self.written[a / 64] >> (a % 64) & 1 != 0
    }
}

/// The CRAY-1 at instruction level.  See the crate documentation.
pub struct Machine {
    pub(crate) a: [Option<u32>; 8],
    pub(crate) s: [Option<u64>; 8],
    pub(crate) b: [Option<u32>; 64],
    pub(crate) t: [Option<u64>; 64],
    pub(crate) v: Box<[[Option<u64>; 64]; 8]>,
    pub(crate) vl: Option<u8>,
    pub(crate) vm: Option<u64>,
    pub(crate) p: u32,
    pub(crate) ba: u32,
    pub(crate) la: u32,
    pub(crate) xa: u8,
    pub(crate) m: u8,
    pub(crate) f: u16,
    pub(crate) mem: Memory,
    input: VecDeque<u8>,
    console: Vec<u8>,
    started: bool,
    halted: Option<StepResult>,
    exit: Option<u64>,
    /// An exit instruction or a newly set flag asks for an exchange at the
    /// end of the current instruction.
    pub(crate) want_exchange: bool,
    /// P and parcels of the instruction being executed, for error reports.
    pub(crate) cur_p: u32,
    pub(crate) cur_parcels: Option<(u16, Option<u16>)>,
    steps: u64,
    instructions: u64,
    observer: Option<Box<dyn Observer>>,
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new()
    }
}

impl Machine {
    /// A machine in the reset state with nothing in memory: A, S, B, T, V,
    /// VL, VM, the real-time clock and every memory word are undefined.
    /// The first `step` performs the dead start exchange with the package
    /// at address 0.
    pub fn new() -> Machine {
        Machine {
            a: [None; 8],
            s: [None; 8],
            b: [None; 64],
            t: [None; 64],
            v: Box::new([[None; 64]; 8]),
            vl: None,
            vm: None,
            p: 0,
            ba: 0,
            la: 0,
            xa: 0,
            m: 0,
            f: 0,
            mem: Memory::new(),
            input: VecDeque::new(),
            console: Vec::new(),
            started: false,
            halted: None,
            exit: None,
            want_exchange: false,
            cur_p: 0,
            cur_parcels: None,
            steps: 0,
            instructions: 0,
            observer: None,
        }
    }

    /// A machine with `image` loaded.
    pub fn with_image(image: &[u8]) -> Result<Machine, String> {
        let mut m = Machine::new();
        m.load_image(image)?;
        Ok(m)
    }

    /// Load a raw image: big-endian 64-bit words starting at word 0.  The
    /// words become defined; they do not count as written by the run.
    pub fn load_image(&mut self, image: &[u8]) -> Result<(), String> {
        let (chunks, rest) = image.as_chunks::<8>();
        if !rest.is_empty() {
            return Err(format!(
                "image length {} is not a multiple of 8 bytes",
                image.len()
            ));
        }
        let words: Vec<u64> = chunks.iter().map(|c| u64::from_be_bytes(*c)).collect();
        self.load_words(0, &words)
    }

    /// Load words at absolute word address `addr` (see `load_image`).
    pub fn load_words(&mut self, addr: u32, words: &[u64]) -> Result<(), String> {
        if addr as u64 + words.len() as u64 > IO_PAGE as u64 {
            return Err(format!(
                "image of {} words at {:o} does not fit below the I/O page at word {:o}",
                words.len(),
                addr,
                IO_PAGE
            ));
        }
        for (n, w) in words.iter().enumerate() {
            self.mem.put(addr + n as u32, Some(*w));
        }
        Ok(())
    }

    /// Queue console input characters.
    pub fn push_input(&mut self, bytes: &[u8]) {
        self.input.extend(bytes);
    }

    /// Install (or remove) the observer that receives every `Event`.
    pub fn set_observer(&mut self, observer: Option<Box<dyn Observer>>) {
        self.observer = observer;
    }

    // ---- state accessors

    /// (Ai): 24 bits, `None` if undefined.
    pub fn a(&self, i: usize) -> Option<u32> {
        self.a[i]
    }
    /// (Si), `None` if undefined.
    pub fn s(&self, i: usize) -> Option<u64> {
        self.s[i]
    }
    /// (Bjk): 24 bits.
    pub fn b(&self, jk: usize) -> Option<u32> {
        self.b[jk]
    }
    /// (Tjk).
    pub fn t(&self, jk: usize) -> Option<u64> {
        self.t[jk]
    }
    /// Element `elem` of Vi.
    pub fn v(&self, i: usize, elem: usize) -> Option<u64> {
        self.v[i][elem]
    }
    /// The 7-bit VL register.
    pub fn vl(&self) -> Option<u8> {
        self.vl
    }
    /// The number of operations a vector instruction performs for the
    /// current VL: ((VL - 1) mod 64) + 1 (HRM page 4-10).
    pub fn vector_length(&self) -> Option<usize> {
        self.vl.map(vl_count)
    }
    /// The vector mask.  Bit 63 of the value is the manual's bit 0 and
    /// belongs to element 0.
    pub fn vm(&self) -> Option<u64> {
        self.vm
    }
    /// The 22-bit program parcel address, relative to BA.
    pub fn p(&self) -> u32 {
        self.p
    }
    /// The 18-bit base address register (the base is BA * 16 words).
    pub fn ba(&self) -> u32 {
        self.ba
    }
    /// The 18-bit limit address register (the limit is LA * 16 words).
    pub fn la(&self) -> u32 {
        self.la
    }
    /// The 8-bit exchange address register (the package is at XA * 16).
    pub fn xa(&self) -> u8 {
        self.xa
    }
    /// The 4-bit mode register, see `mode`.
    pub fn m(&self) -> u8 {
        self.m
    }
    /// The 9-bit flag register, see `flag`.
    pub fn f(&self) -> u16 {
        self.f
    }
    /// The real-time clock.  It counts clock periods, which an instruction
    /// level model cannot predict: always `None`.
    pub fn rtc(&self) -> Option<u64> {
        None
    }
    /// True if the active exchange package is in monitor mode.
    pub fn monitor_mode(&self) -> bool {
        self.m & mode::MONITOR != 0
    }
    /// The memory word at an absolute address; `None` if it is undefined or
    /// in the I/O page.
    pub fn mem(&self, addr: u32) -> Option<u64> {
        if addr < IO_PAGE {
            self.mem.get(addr)
        } else {
            None
        }
    }
    /// True if the run stored into this word (the I/O page never counts).
    pub fn mem_written(&self, addr: u32) -> bool {
        addr < IO_PAGE && self.mem.is_written(addr)
    }
    /// Every memory word the run stored into, in ascending address order,
    /// with its final value.
    pub fn written_words(&self) -> Vec<(u32, Option<u64>)> {
        let mut out = Vec::new();
        for (n, bits) in self.mem.written.iter().enumerate() {
            let mut bits = *bits;
            while bits != 0 {
                let addr = n as u32 * 64 + bits.trailing_zeros();
                out.push((addr, self.mem.get(addr)));
                bits &= bits - 1;
            }
        }
        out
    }
    /// Everything written to the console so far.
    pub fn console(&self) -> &[u8] {
        &self.console
    }
    /// The value written to TEST_EXIT, once the run has ended that way.
    pub fn exit_code(&self) -> Option<u64> {
        self.exit
    }
    /// Steps taken: instructions issued plus the dead start and any exchange
    /// that started from a flag already set in a newly loaded package.
    pub fn steps(&self) -> u64 {
        self.steps
    }
    /// Instructions issued.
    pub fn instructions(&self) -> u64 {
        self.instructions
    }
    /// True once the dead start exchange has been performed.
    pub fn started(&self) -> bool {
        self.started
    }

    // ---- state setters for test set-up (they report events like any write)

    pub fn set_a(&mut self, i: usize, value: Option<u32>) {
        let value = value.map(|v| v & A_MASK);
        self.a[i] = value;
        self.emit(Event::A { i: i as u8, value });
    }
    pub fn set_s(&mut self, i: usize, value: Option<u64>) {
        self.s[i] = value;
        self.emit(Event::S { i: i as u8, value });
    }
    pub fn set_b(&mut self, jk: usize, value: Option<u32>) {
        let value = value.map(|v| v & A_MASK);
        self.b[jk] = value;
        self.emit(Event::B {
            jk: jk as u8,
            value,
        });
    }
    pub fn set_t(&mut self, jk: usize, value: Option<u64>) {
        self.t[jk] = value;
        self.emit(Event::T {
            jk: jk as u8,
            value,
        });
    }
    pub fn set_v(&mut self, i: usize, elem: usize, value: Option<u64>) {
        self.v[i][elem] = value;
        self.emit(Event::V {
            i: i as u8,
            elem: elem as u8,
            value,
        });
    }
    pub fn set_vl(&mut self, value: Option<u8>) {
        let value = value.map(|v| v & 0x7f);
        self.vl = value;
        self.emit(Event::Vl(value));
    }
    pub fn set_vm(&mut self, value: Option<u64>) {
        self.vm = value;
        self.emit(Event::Vm(value));
    }
    pub(crate) fn set_xa(&mut self, value: u8) {
        self.xa = value;
        self.emit(Event::Xa(value));
    }
    pub(crate) fn set_m(&mut self, value: u8) {
        self.m = value & 0xf;
        self.emit(Event::Mode(self.m));
    }
    pub(crate) fn set_f(&mut self, value: u16) {
        self.f = value & 0o777;
        self.emit(Event::Flags(self.f));
    }
    /// Store a word at an absolute address as the run would (it counts as
    /// written), without range checks.  Not for the I/O page.
    pub fn store(&mut self, addr: u32, value: Option<u64>) {
        assert!(addr < IO_PAGE, "store is for memory, not the I/O page");
        self.mem.put(addr, value);
        self.mem.mark_written(addr);
        self.emit(Event::Mem { addr, value });
    }
    /// Put the machine past the dead start with the given control registers,
    /// without touching memory: for unit tests of single instructions.
    pub fn start_at(&mut self, p: u32, ba: u32, la: u32, m: u8) {
        self.started = true;
        self.p = p & P_MASK;
        self.ba = ba & 0x3ffff;
        self.la = la & 0x3ffff;
        self.m = m & 0xf;
        self.f = 0;
        self.xa = 0;
    }

    #[inline]
    pub(crate) fn emit(&mut self, event: Event) {
        if let Some(o) = self.observer.as_mut() {
            o.event(&event);
        }
    }

    // ---- errors

    pub(crate) fn error(&self, kind: ErrorKind, detail: impl Into<String>) -> TestError {
        TestError {
            kind,
            p: self.cur_p,
            parcels: self.cur_parcels,
            detail: detail.into(),
        }
    }

    /// The value, or the test error `undefined value used`.
    pub(crate) fn need<T>(&self, value: Option<T>, what: &str) -> Result<T, TestError> {
        value.ok_or_else(|| self.error(ErrorKind::UndefinedValue, what))
    }

    // ---- addressing (HRM pages 3-43, 3-44)

    /// The absolute address of a word address relative to BA, or `None` if
    /// it is outside the field.
    ///
    /// The field ends at (LA) * 16 - 1 (page 3-44), and nothing above the
    /// one million words of memory can be referenced (page 4-3: either of
    /// the two address bits above 2**20 set is a range error).  `rel` may be
    /// any 24-bit value; the sum is not wrapped.
    pub fn translate(&self, rel: u32) -> Option<u32> {
        let abs = rel as u64 + self.ba as u64 * 16;
        let limit = (self.la as u64 * 16).min(MEMORY_WORDS as u64);
        (abs < limit).then_some(abs as u32)
    }

    /// Set a flag in F and ask for the exchange, unless monitor mode keeps
    /// the flag clear (HRM page 3-36).
    pub(crate) fn interrupt(&mut self, flag: u16) {
        if !self.monitor_mode() {
            self.set_f(self.f | flag);
            self.want_exchange = true;
        }
    }

    // ---- data memory

    /// Read the word at an absolute address inside the field, with the side
    /// effects of the I/O page.
    pub(crate) fn read_abs(&mut self, abs: u32) -> Option<u64> {
        if abs < IO_PAGE {
            return self.mem.get(abs);
        }
        match abs {
            CON_STAT => Some(2 | !self.input.is_empty() as u64),
            CON_DATA => Some(self.input.pop_front().unwrap_or(0) as u64),
            CYCLES => None,
            _ => Some(0),
        }
    }

    /// Write the word at an absolute address inside the field.
    pub(crate) fn write_abs(&mut self, abs: u32, value: Option<u64>) -> Result<(), TestError> {
        if abs < IO_PAGE {
            self.mem.put(abs, value);
            self.mem.mark_written(abs);
            self.emit(Event::Mem { addr: abs, value });
            return Ok(());
        }
        match abs {
            CON_DATA => {
                let byte = self.need(value, "console output character")? as u8;
                self.console.push(byte);
                self.emit(Event::Console(byte));
            }
            TEST_EXIT => {
                let code = self.need(value, "exit code written to TEST_EXIT")?;
                if self.exit.is_none() {
                    self.exit = Some(code);
                    self.emit(Event::Exit(code));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Read a data word at an address relative to BA.  Outside the field the
    /// operand range flag is raised (unless in monitor mode) and the value
    /// is undefined: the manual does not say what the register receives.
    pub(crate) fn read_data(&mut self, rel: u32) -> Option<u64> {
        match self.translate(rel) {
            Some(abs) => self.read_abs(abs),
            None => {
                self.interrupt(flag::OPERAND_RANGE);
                None
            }
        }
    }

    /// Store a data word at an address relative to BA.  Outside the field
    /// memory is not altered (HRM page 3-43) and the operand range flag is
    /// raised unless in monitor mode.  Returns false if the store was
    /// outside the field.
    pub(crate) fn write_data(&mut self, rel: u32, value: Option<u64>) -> Result<bool, TestError> {
        match self.translate(rel) {
            Some(abs) => {
                self.write_abs(abs, value)?;
                Ok(true)
            }
            None => {
                self.interrupt(flag::OPERAND_RANGE);
                Ok(false)
            }
        }
    }

    // ---- exchange (HRM pages 3-35 to 3-41, figure 3-8)

    /// The exchange package of the active program as 16 words.  A word is
    /// undefined if an A or S register (or VL, for word 3) it holds is.
    /// Fields the figure leaves blank and the memory error fields are zero.
    pub fn exchange_package(&self) -> [Option<u64>; 16] {
        let a = |i: usize| self.a[i].map(|v| v as u64);
        let mut w = [None; 16];
        w[0] = a(0).map(|a0| (self.p as u64) << 24 | a0);
        w[1] = a(1).map(|a1| (self.ba as u64) << 28 | a1);
        w[2] = a(2).map(|a2| (self.la as u64) << 28 | (self.m as u64) << 24 | a2);
        w[3] = match (self.vl, a(3)) {
            (Some(vl), Some(a3)) => {
                Some((self.xa as u64) << 40 | (vl as u64) << 33 | (self.f as u64) << 24 | a3)
            }
            _ => None,
        };
        for (i, word) in w.iter_mut().enumerate().take(8).skip(4) {
            *word = a(i);
        }
        w[8..16].copy_from_slice(&self.s);
        w
    }

    /// The exchange sequence: swap the active package with the one at
    /// (XA) * 16.  `dead_start` writes an undefined outgoing package.
    fn exchange(&mut self, dead_start: bool) -> Result<(), TestError> {
        let base = self.xa as u32 * 16;
        self.emit(Event::ExchangeStart { xa: self.xa });
        let outgoing = if dead_start {
            [None; 16]
        } else {
            self.exchange_package()
        };
        let mut incoming = [None; 16];
        for (n, w) in incoming.iter_mut().enumerate() {
            *w = self.mem.get(base + n as u32);
        }
        // P, BA, LA, M, XA, VL and F steer the program: they must be known
        let mut control = [0u64; 4];
        for (n, c) in control.iter_mut().enumerate() {
            *c = incoming[n].ok_or_else(|| {
                self.error(
                    ErrorKind::UndefinedValue,
                    format!("word {} of the exchange package at {:o} (it holds P, BA, LA, M, XA, VL or F)", n, base),
                )
            })?;
        }
        for (n, w) in outgoing.iter().enumerate() {
            let addr = base + n as u32;
            self.mem.put(addr, *w);
            self.mem.mark_written(addr);
            self.emit(Event::Mem { addr, value: *w });
        }
        self.p = (control[0] >> 24) as u32 & P_MASK;
        self.ba = (control[1] >> 28) as u32 & 0x3ffff;
        self.la = (control[2] >> 28) as u32 & 0x3ffff;
        self.emit(Event::P(self.p));
        self.emit(Event::Ba(self.ba));
        self.emit(Event::La(self.la));
        self.set_m((control[2] >> 24) as u8 & 0xf);
        self.set_xa((control[3] >> 40) as u8);
        self.set_vl(Some((control[3] >> 33) as u8 & 0x7f));
        self.set_f((control[3] >> 24) as u16 & 0o777);
        for (i, word) in incoming.iter().enumerate() {
            if i < 8 {
                self.set_a(i, word.map(|w| w as u32 & A_MASK));
            } else {
                self.set_s(i - 8, *word);
            }
        }
        self.emit(Event::ExchangeEnd);
        Ok(())
    }

    /// True if a flag in F asks for an exchange: any flag outside monitor
    /// mode, the memory error flag in any mode (HRM page 3-36).
    fn interrupt_pending(&self) -> bool {
        self.f & flag::MEMORY_ERROR != 0 || (self.f != 0 && !self.monitor_mode())
    }

    // ---- fetch

    /// The parcel at P, `Ok(None)` for a program range error.
    fn fetch(&mut self, p: u32) -> Result<Option<u16>, TestError> {
        let Some(abs) = self.translate(p >> 2) else {
            return Ok(None);
        };
        if abs >= IO_PAGE {
            return Err(self.error(
                ErrorKind::UndefinedValue,
                format!("instruction fetch from the I/O page, word {:o}", abs),
            ));
        }
        let word = self.mem.get(abs).ok_or_else(|| {
            self.error(
                ErrorKind::UndefinedValue,
                format!("instruction fetch from undefined memory word {:o}", abs),
            )
        })?;
        Ok(Some((word >> (48 - 16 * (p & 3))) as u16))
    }

    /// A fetch outside the field: program range error.
    fn program_range_on_fetch(&mut self) -> Result<(), TestError> {
        if self.monitor_mode() {
            // The flag cannot set and nothing stops the machine; the manual
            // does not say what it executes.
            return Err(self.error(
                ErrorKind::NotDefinedByManual,
                "instruction fetch outside the field (BA, LA) in monitor mode, where the program range flag cannot set",
            ));
        }
        self.interrupt(flag::PROGRAM_RANGE);
        Ok(())
    }

    // ---- stepping

    /// Do one step: the dead start exchange, an exchange asked for by a flag
    /// that came in with the package, or one instruction (followed by its
    /// exchange if it exits or raises a flag).
    pub fn step(&mut self) -> StepResult {
        if let Some(h) = &self.halted {
            return h.clone();
        }
        self.steps += 1;
        let result = match self.step_inner() {
            Ok(()) => match self.exit {
                Some(code) => StepResult::Exit(code),
                None => StepResult::Running,
            },
            Err(e) => StepResult::Error(e),
        };
        if result != StepResult::Running {
            self.halted = Some(result.clone());
        }
        result
    }

    fn step_inner(&mut self) -> Result<(), TestError> {
        self.cur_p = self.p;
        self.cur_parcels = None;
        if !self.started {
            // HRM page 3-40: XA is forced to 0 and an error exit executed.
            self.started = true;
            self.xa = 0;
            return self.exchange(true);
        }
        if self.interrupt_pending() {
            return self.exchange(false);
        }
        let p = self.p;
        let Some(parcel0) = self.fetch(p)? else {
            self.program_range_on_fetch()?;
            self.want_exchange = false;
            return self.exchange(false);
        };
        self.cur_parcels = Some((parcel0, None));
        let mut d: Decoded = decode(parcel0, None);
        if d.parcels == 2 {
            let Some(parcel1) = self.fetch((p + 1) & P_MASK)? else {
                self.program_range_on_fetch()?;
                self.want_exchange = false;
                return self.exchange(false);
            };
            self.cur_parcels = Some((parcel0, Some(parcel1)));
            d = decode(parcel0, Some(parcel1));
        }
        self.instructions += 1;
        self.emit(Event::Issue {
            p,
            parcel0,
            parcel1: (d.parcels == 2).then_some(d.m),
        });
        self.p = (p + d.parcels as u32) & P_MASK;
        self.execute(&d)?;
        if self.want_exchange {
            self.want_exchange = false;
            self.exchange(false)?;
        }
        Ok(())
    }

    /// Step until the run ends or `max_steps` steps have been taken.
    pub fn run(&mut self, max_steps: u64) -> RunResult {
        for _ in 0..max_steps {
            match self.step() {
                StepResult::Running => {}
                StepResult::Exit(code) => return RunResult::Exit(code),
                StepResult::Error(e) => return RunResult::Error(e),
            }
        }
        match &self.halted {
            Some(StepResult::Exit(code)) => RunResult::Exit(*code),
            Some(StepResult::Error(e)) => RunResult::Error(e.clone()),
            _ => RunResult::Limit,
        }
    }
}

/// The number of operations for a VL register value (HRM page 4-10): one is
/// subtracted from VL and one added to the low six bits of the result.
pub fn vl_count(vl: u8) -> usize {
    ((vl.wrapping_sub(1) & 0o77) + 1) as usize
}

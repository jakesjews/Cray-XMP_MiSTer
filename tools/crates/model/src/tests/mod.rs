//! Unit tests of the model, one module per instruction group.  The
//! expectations name their page: of the CRAY-1 Hardware Reference Manual
//! 2240004 rev C, which the X-MP's manual repeats for most instructions, or
//! of the CRAY X-MP Series Model 14 mainframe reference manual CSM-0111000
//! ("X").

use crate::*;
use std::cell::RefCell;
use std::rc::Rc;

mod channels;
mod control;
mod formats;
mod io;
mod memory;
mod scalar;
mod shared;
mod undefined;
mod vectors;

/// Word address the test code is loaded at.
pub(crate) const CODE: u32 = 0o200;
/// The largest value of a 19-bit base or limit field: units of 32 words.
pub(crate) const LA_MAX: u32 = 0x7ffff;

/// Assemble CAL statements separated by `;`, each `RESULT OPERAND`, with
/// numeric operands only (numbers are octal unless written D'n or X'n).
pub(crate) fn cal(source: &str) -> Vec<u16> {
    let mut out = Vec::new();
    for line in source.split(';').map(str::trim).filter(|l| !l.is_empty()) {
        let mut fields = line.split_whitespace();
        let result = fields.next().unwrap();
        let operand = fields.next().unwrap_or("");
        assert!(fields.next().is_none(), "too many fields in `{}`", line);
        let a = cray_xmp_isa::assemble_numeric(result, operand)
            .unwrap_or_else(|e| panic!("`{}`: {}", line, e));
        out.extend(a.encoding.parcels());
    }
    out
}

/// Pack parcels four to a word, parcel 0 in bits 63 to 48 (HRM page 4-1).
pub(crate) fn words(parcels: &[u16]) -> Vec<u64> {
    parcels
        .chunks(4)
        .map(|c| {
            c.iter()
                .enumerate()
                .fold(0u64, |w, (n, p)| w | (*p as u64) << (48 - 16 * n))
        })
        .collect()
}

/// Give A0-A7, S0-S7 the value 0 and VL the value 0, so that exchange
/// packages are fully defined.
pub(crate) fn define_registers(m: &mut Machine) {
    for i in 0..8 {
        m.set_a(i, Some(0));
        m.set_s(i, Some(0));
    }
    m.set_vl(Some(0));
}

/// A machine past the dead start in monitor mode, both fields from 0 to the
/// end of memory, cluster 0, with `parcels` at word 200 and P there.  A, S
/// and VL are 0; B, T, V and VM are undefined.
pub(crate) fn monitor(parcels: &[u16]) -> Machine {
    let mut m = Machine::new();
    m.load_words(CODE, &words(parcels)).unwrap();
    m.start_at(CODE * 4, 0, LA_MAX, mode::MONITOR);
    define_registers(&mut m);
    m
}

/// `monitor` from CAL text.
pub(crate) fn monitor_cal(source: &str) -> Machine {
    monitor(&cal(source))
}

/// A machine past the dead start running a user program: monitor mode off,
/// the operand range interrupt on, both fields with the given base and
/// limit (units of 32 words), `parcels` at relative word 200.  XA is 0 and
/// words 0 to 17 hold a monitor package (P at word 100, monitor mode, the
/// largest limits, XA = 0, registers 0), so an exchange lands in monitor
/// mode at word 100, which holds `J 400` (parcel 400, a jump to itself).
pub(crate) fn user(parcels: &[u16], ba: u32, la: u32) -> Machine {
    let mut m = Machine::new();
    let mut package = [0u64; 16];
    package[0] = (0o100u64 * 4) << 24;
    package[2] = (LA_MAX as u64) << 29 | (mode::MONITOR as u64) << 24;
    package[5] = (LA_MAX as u64) << 29;
    m.load_words(0, &package).unwrap();
    m.load_words(0o100, &words(&cal("J 400"))).unwrap();
    m.load_words(ba * 32 + CODE, &words(parcels)).unwrap();
    m.start_at(CODE * 4, ba, la, mode::OPERAND_RANGE);
    define_registers(&mut m);
    m
}

/// `user` from CAL text, both fields from 0 to the end of memory.
pub(crate) fn user_cal(source: &str) -> Machine {
    user(&cal(source), 0, LA_MAX)
}

/// Step `n` times; every step must leave the machine running.
pub(crate) fn steps(m: &mut Machine, n: usize) {
    for k in 0..n {
        let r = m.step();
        assert_eq!(r, StepResult::Running, "step {} of {}", k + 1, n);
    }
}

/// Record every event from now on.
pub(crate) fn record(m: &mut Machine) -> Rc<RefCell<Vec<Event>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    m.set_observer(Some(Box::new(move |e: &Event| {
        sink.borrow_mut().push(e.clone())
    })));
    events
}

/// The test error of a step that must fail.
pub(crate) fn error_of(m: &mut Machine) -> TestError {
    match m.step() {
        StepResult::Error(e) => e,
        other => panic!("expected a test error, got {:?}", other),
    }
}

/// The exchange package fields of words 0 to 3 stored at `addr`:
/// (P, IBA, ILA, the modes of word 2, XA, VL, F), by figure 3-3 on page
/// X 3-10.  The deadlock flag is bit 9 of F here.
pub(crate) fn package_fields(m: &Machine, addr: u32) -> (u32, u32, u32, u8, u8, u8, u16) {
    let w = |n: u32| {
        m.mem(addr + n)
            .unwrap_or_else(|| panic!("package word {} is undefined", n))
    };
    (
        (w(0) >> 24) as u32 & 0xff_ffff,
        (w(1) >> 29) as u32 & 0x7_ffff,
        (w(2) >> 29) as u32 & 0x7_ffff,
        (w(2) >> 24) as u8 & 0x1f,
        (w(3) >> 40) as u8,
        (w(3) >> 33) as u8 & 0x7f,
        (w(3) >> 24) as u16 & 0o777 | ((w(3) >> 48) as u16 & 1) << 9,
    )
}

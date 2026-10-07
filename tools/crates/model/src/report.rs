//! The text formats of `cray-xmp-run`: the trace (one line per event) and the
//! end state.  Both are documented at the top of `src/bin/cray-xmp-run.rs`.

use crate::event::Event;
use crate::machine::{Machine, RunResult};
use cray_xmp_isa::{decode, disassemble};
use std::fmt::Write;

fn hex(value: Option<u64>, digits: usize) -> String {
    match value {
        Some(v) => format!("{:0digits$x}", v, digits = digits),
        None => "undef".to_string(),
    }
}

/// The trace line of an event, without the newline.
pub fn trace_line(event: &Event) -> String {
    match *event {
        Event::Issue {
            p,
            parcel0,
            parcel1,
        } => {
            let text = disassemble(&decode(parcel0, parcel1));
            match parcel1 {
                Some(m) => format!("I {:08o} {:06o} {:06o}  {}", p, parcel0, m, text),
                None => format!("I {:08o} {:06o}         {}", p, parcel0, text),
            }
        }
        Event::A { i, value } => format!("A{} {}", i, hex(value.map(u64::from), 6)),
        Event::S { i, value } => format!("S{} {}", i, hex(value, 16)),
        Event::B { jk, value } => format!("B{:02o} {}", jk, hex(value.map(u64::from), 6)),
        Event::T { jk, value } => format!("T{:02o} {}", jk, hex(value, 16)),
        Event::V { i, elem, value } => format!("V{}[{}] {}", i, elem, hex(value, 16)),
        Event::Vl(value) => format!("VL {}", hex(value.map(u64::from), 2)),
        Event::Vm(value) => format!("VM {}", hex(value, 16)),
        Event::P(p) => format!("P {:06x}", p),
        Event::Ba(ba) => format!("BA {:05x}", ba),
        Event::La(la) => format!("LA {:05x}", la),
        Event::Xa(xa) => format!("XA {:02x}", xa),
        Event::Mode(m) => format!("MODE {:x}", m),
        Event::Flags(f) => format!("F {:03x}", f),
        Event::Rtc(value) => format!("RT {}", hex(value, 16)),
        Event::Mem { addr, value } => format!("M {:07o} {}", addr, hex(value, 16)),
        Event::ExchangeStart { xa } => format!("X start {:02x}", xa),
        Event::ExchangeEnd => "X done".to_string(),
        Event::Console(byte) => format!("C {:02x}", byte),
        Event::Exit(code) => format!("E {}", code),
    }
}

/// The end state of a run as text: see `src/bin/cray-xmp-run.rs` for the
/// format.  `result` is what `Machine::run` returned.
pub fn state_text(m: &Machine, result: &RunResult) -> String {
    let mut out = String::new();
    match result {
        RunResult::Exit(code) => writeln!(out, "exit {}", code).unwrap(),
        _ => out.push_str("exit none\n"),
    }
    out.push_str("console");
    if !m.console().is_empty() {
        out.push(' ');
        for byte in m.console() {
            write!(out, "{:02x}", byte).unwrap();
        }
    }
    out.push('\n');
    for (addr, value) in m.written_words() {
        writeln!(out, "mem {:07o} {}", addr, hex(value, 16)).unwrap();
    }
    for i in 0..8 {
        writeln!(out, "A{} {}", i, hex(m.a(i).map(u64::from), 6)).unwrap();
    }
    for i in 0..8 {
        writeln!(out, "S{} {}", i, hex(m.s(i), 16)).unwrap();
    }
    writeln!(out, "VL {}", hex(m.vl().map(u64::from), 2)).unwrap();
    writeln!(out, "VM {}", hex(m.vm(), 16)).unwrap();
    writeln!(out, "P {:06x}", m.p()).unwrap();
    writeln!(out, "BA {:05x}", m.ba()).unwrap();
    writeln!(out, "LA {:05x}", m.la()).unwrap();
    writeln!(out, "XA {:02x}", m.xa()).unwrap();
    writeln!(out, "M {:x}", m.m()).unwrap();
    writeln!(out, "F {:03x}", m.f()).unwrap();
    for jk in 0..64 {
        if let Some(value) = m.b(jk) {
            writeln!(out, "B{:02o} {:06x}", jk, value).unwrap();
        }
    }
    for jk in 0..64 {
        if let Some(value) = m.t(jk) {
            writeln!(out, "T{:02o} {:016x}", jk, value).unwrap();
        }
    }
    for i in 0..8 {
        for elem in 0..64 {
            if let Some(value) = m.v(i, elem) {
                writeln!(out, "V{}[{}] {:016x}", i, elem, value).unwrap();
            }
        }
    }
    out
}

//! A disassembler for the I/O Processor: one instruction in the notation of
//! the manual's instruction summary [HW A-1 to A-4], for traces.
//!
//! The summary writes `d`, `k`, `dd` (operand register d) and `iod` (channel
//! d) for the fields.  Here d is a decimal number, k a hexadecimal one
//! (most are addresses), operand register d is `OR[d]`, and a channel is
//! its mnemonic for the channels every processor has (IOR, PFR, PXS, LME,
//! RTC, MOS) and `IO` with the octal channel number for the others.
//! Function codes are octal.  `(B)` is operand register B, as in the manual.

/// The number of parcels of the instruction that begins with `parcel`: 2
/// for 014 to 017 and for the branch modes with a constant (075, 077, 124
/// to 127, 134 to 137), else 1.
pub fn parcels(parcel: u16) -> u16 {
    match parcel >> 9 {
        0o014..=0o017 | 0o075 | 0o077 | 0o124..=0o127 | 0o134..=0o137 => 2,
        _ => 1,
    }
}

/// The channel of an I/O instruction that takes it from d.
fn channel(d: u16) -> String {
    match d {
        0 => "IOR".to_string(),
        1 => "PFR".to_string(),
        2 => "PXS".to_string(),
        3 => "LME".to_string(),
        4 => "RTC".to_string(),
        5 => "MOS".to_string(),
        _ => format!("IO{d:o}"),
    }
}

/// The eight forms of the groups 020, 030, 050 and 060 with operand `x`.
fn operation(f: u16, x: &str) -> String {
    match f & 7 {
        0 => format!("A = {x}"),
        1 => format!("A = A & {x}"),
        2 => format!("A = A + {x}"),
        3 => format!("A = A - {x}"),
        4 => format!("{x} = A"),
        5 => format!("{x} = A + {x}"),
        6 => format!("{x} = {x} + 1"),
        _ => format!("{x} = {x} - 1"),
    }
}

/// Disassemble the instruction whose first parcel is `parcel`.  `k` is the
/// parcel after it, used if the instruction has two (see `parcels`).
pub fn disassemble(parcel: u16, k: u16) -> String {
    let f = parcel >> 9;
    let d = parcel & 0o777;
    match f {
        // the kernel keeps halt codes in the d of a PASS
        0o000 if d != 0 => format!("PASS ({d})"),
        0o000 => "PASS".to_string(),
        0o001 => "EXIT".to_string(),
        0o002 => "I = 0".to_string(),
        0o003 => "I = 1".to_string(),
        0o004 => format!("A = A > {d}"),
        0o005 => format!("A = A < {d}"),
        0o006 => format!("A = A >> {d}"),
        0o007 => format!("A = A << {d}"),
        0o010..=0o013 => operation(f & 3, &d.to_string()),
        0o014..=0o017 => operation(f & 3, &format!("0x{k:04X}")),
        0o020..=0o027 => operation(f, &format!("OR[{d}]")),
        0o030..=0o037 => operation(f, &format!("(OR[{d}])")),
        0o040 => format!("C = 1, {} = DN", channel(d)),
        0o041 => format!("C = 1, {} = BZ", channel(d)),
        0o042 => "C = 1, IOB = DN".to_string(),
        0o043 => "C = 1, IOB = BZ".to_string(),
        0o044 => "A = A > B".to_string(),
        0o045 => "A = A < B".to_string(),
        0o046 => "A = A >> B".to_string(),
        0o047 => "A = A << B".to_string(),
        0o050..=0o057 => operation(f, "B"),
        0o060..=0o067 => operation(f, "(B)"),
        0o070..=0o137 => {
            let mode = if f < 0o100 { f & 7 } else { f >> 2 & 7 };
            let branch = match mode {
                0 => format!("P = P + {d}"),
                1 => format!("P = P - {d}"),
                2 => format!("R = P + {d}"),
                3 => format!("R = P - {d}"),
                4 => format!("P = OR[{d}]"),
                5 => format!("P = OR[{d}] + 0x{k:04X}"),
                6 => format!("R = OR[{d}]"),
                _ => format!("R = OR[{d}] + 0x{k:04X}"),
            };
            if f < 0o100 {
                branch
            } else {
                let condition = ["C = 0", "C # 0", "A = 0", "A # 0"][(f & 3) as usize];
                format!("{branch}, {condition}")
            }
        }
        0o140..=0o157 => format!("{} : {:o}", channel(d), f & 0o17),
        _ => format!("IOB : {:o}", f & 0o17),
    }
}

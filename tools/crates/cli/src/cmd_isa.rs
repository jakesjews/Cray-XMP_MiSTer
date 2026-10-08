//! `cray-xmp isa`: print the instruction table.

use crate::args::Args;
use cray_xmp_isa::{Form, Kind, RegRef, FORMS};
use std::process::ExitCode;

pub const DETAILS: &str = "
Prints the instruction table the assembler, the disassembler and the
other tools share: the instructions of a one-processor CRAY X-MP, with
the opcode pattern, the CAL syntax, the length in parcels, the functional
unit, flags, the registers read and written, and a description.  Most
rows are lines of Appendix D of the CRAY-1 Hardware Reference Manual
(2240004 rev C), which the X-MP's manual repeats; the others are marked.
";

/// The registers of a list that this row can really touch: a special form
/// that fixes a field at 0 uses the constant the machine substitutes for
/// register 0, not the register.
fn names(f: &Form, regs: &[RegRef]) -> String {
    let digit = |n: usize| f.pattern.as_bytes()[n];
    let listed: Vec<String> = regs
        .iter()
        .filter(|r| match r {
            RegRef::Ah => digit(2) != b'0',
            RegRef::Aj | RegRef::Sj => digit(4) != b'0',
            RegRef::Ak | RegRef::Sk => digit(5) != b'0',
            _ => true,
        })
        .map(|r| match r {
            RegRef::BBlock => "Bjk..".to_string(),
            RegRef::TBlock => "Tjk..".to_string(),
            RegRef::Vl => "VL".to_string(),
            RegRef::Vm => "VM".to_string(),
            RegRef::Rtc => "RTC".to_string(),
            RegRef::Xa => "XA".to_string(),
            other => format!("{:?}", other),
        })
        .collect();
    if listed.is_empty() {
        "-".to_string()
    } else {
        listed.join(",")
    }
}

fn flags(f: &Form) -> String {
    let mut s = String::new();
    for (set, letter) in [
        (f.is_conditional_branch(), 'C'),
        (f.is_branch(), 'B'),
        (f.reads_memory(), 'R'),
        (f.writes_memory(), 'W'),
        (f.is_monitor_only(), 'M'),
        (f.is_exit(), 'X'),
        (f.is_vector(), 'V'),
        (f.flags() & cray_xmp_isa::flag::OPTION != 0, 'O'),
        (f.flags() & cray_xmp_isa::flag::XMP != 0, 'P'),
    ] {
        if set {
            s.push(letter);
        }
    }
    if s.is_empty() {
        s.push('-');
    }
    s
}

pub fn table() -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "  {:<8} {:<9} {:<10} {:<3} {:<10} {:<5} {:<12} {:<6} {}\n",
        "OCTAL", "CAL", "", "LEN", "UNIT", "FLAGS", "READS", "WRITES", "DESCRIPTION"
    ));
    for f in FORMS {
        let mark = match f.kind {
            Kind::Base => ' ',
            Kind::Special => '+',
            Kind::Alt => '~',
        };
        out.push_str(
            format!(
                "{} {:<8} {:<9} {:<10} {:<3} {:<10} {:<5} {:<12} {:<6} {}",
                mark,
                f.pattern,
                f.result,
                f.operand,
                f.parcels,
                f.unit().name(),
                flags(f),
                names(f, f.reads()),
                names(f, f.writes()),
                f.desc
            )
            .trim_end(),
        );
        out.push('\n');
    }
    out.push_str(
        "
+  special syntax form of the instruction above it (dagger in Appendix D)
~  another spelling the assembler accepts
Pattern: octal digits of the first parcel; h i j k are operand fields, x is
ignored by the machine, m is a second parcel.  Rows are matched in order.
Flags: B branch, C conditional, R reads memory, W writes memory,
M monitor mode only, X exit (exchange), V vector (uses VL),
O came as an option of the CRAY-1 of 1982, P new with the X-MP.
Register 0 in the h, j or k field is a constant, not a register:
(Ah) = 0, (Aj) = 0, (Ak) = 1, (Sj) = 0, (Sk) = 2**63.
",
    );
    out
}

pub fn run(argv: &[String]) -> Result<ExitCode, String> {
    let args = Args::parse(argv, &[])?;
    if !args.positional.is_empty() {
        return Err(format!(
            "{}isa takes no arguments",
            crate::args::USAGE_PREFIX
        ));
    }
    print!("{}", table());
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_has_a_line_per_form() {
        let text = super::table();
        let rows = text
            .lines()
            .filter(|l| l.len() > 8 && l.as_bytes()[2].is_ascii_digit())
            .count();
        assert_eq!(rows, cray_xmp_isa::FORMS.len());
        assert!(text.contains("+ 030i0k   Ai        Ak         1   A Int Add  -     Ak           Ai     Transmit (Ak) to Ai"));
        assert!(text.contains(
            "+ 001000   PASS                 1   -          M     -            -      Pass"
        ));
        assert!(text.contains("  176ixk   Vi        ,A0,Ak     1   Memory     RV    A0,Ak,VL     Vi     Read (VL) words"));
    }
}

//! `cray-xmp dis`: disassemble a memory image.

use crate::args::Args;
use std::process::ExitCode;

pub const DETAILS: &str = "
Disassembles the raw memory image IMG (big-endian 64-bit words) as
instruction parcels, starting with parcel a of the first word.

  --start WORD   first word to disassemble (default 0)
  --words N      number of words (default: to the end of the image)

Numbers are decimal, or octal with 0o, or hex with 0x.

Each line has the address (octal word, parcel a to d), the octal parcels
and the instruction in CAL.  Notes follow a semicolon:

  straddles words     a two-parcel instruction that starts in parcel d
  ignored bits set    bits the machine ignores are not zero and CAL cannot
                      say so; the text assembles with them cleared
  no second parcel    the image ends inside a two-parcel instruction

An image does not say where code stops and data starts: data is shown as
whatever instructions its parcels spell.  Runs of zero words are folded
into one line.
";

fn parcel(words: &[u64], addr: u64) -> Option<u16> {
    words
        .get((addr / 4) as usize)
        .map(|w| (w >> (48 - 16 * (addr % 4))) as u16)
}

fn address(addr: u64) -> String {
    format!("{:07o}{}", addr / 4, (b'a' + (addr % 4) as u8) as char)
}

/// Disassemble `words[start..end]`; instructions may take their second
/// parcel from beyond `end`.
pub fn disassemble(words: &[u64], start: u64, end: u64) -> String {
    let mut out = String::new();
    let mut addr = start * 4;
    let limit = end * 4;
    while addr < limit {
        // fold runs of zero words
        if addr.is_multiple_of(4) {
            let first = addr / 4;
            let mut w = first;
            while w < end && words[w as usize] == 0 {
                w += 1;
            }
            if w - first >= 2 {
                out.push_str(&format!("{:07o}   {} zero words\n", first, w - first));
                addr = w * 4;
                continue;
            }
        }
        let p0 = parcel(words, addr).unwrap_or(0);
        let two = cray_xmp_isa::length(p0) == 2;
        let p1 = if two { parcel(words, addr + 1) } else { None };
        let d = cray_xmp_isa::decode(p0, p1);
        let mut notes = Vec::new();
        let code = match (two, p1) {
            (false, _) => format!("{:06o}", p0),
            (true, Some(m)) => format!("{:06o} {:06o}", p0, m),
            (true, None) => {
                notes.push("no second parcel");
                format!("{:06o} ------", p0)
            }
        };
        if two && addr % 4 == 3 && p1.is_some() {
            notes.push("straddles words");
        }
        let (result, operand) = cray_xmp_isa::disassemble_fields(&d);
        // does the text spell these very parcels, or only their canonical form?
        let exact = match cray_xmp_isa::assemble_numeric(&result, &operand) {
            Ok(a) => a.encoding.parcel0 == p0 && a.encoding.parcel1 == p1,
            Err(_) => d.is_canonical(),
        };
        if !exact && !(two && p1.is_none()) {
            notes.push("ignored bits set");
        }
        let text = if operand.is_empty() {
            result
        } else {
            format!("{:<9} {}", result, operand)
        };
        let mut line = format!("{}  {:<13}  {}", address(addr), code, text);
        if !notes.is_empty() {
            line = format!("{:<52}; {}", line, notes.join(", "));
        }
        out.push_str(line.trim_end());
        out.push('\n');
        addr += if two && p1.is_some() { 2 } else { 1 };
    }
    out
}

pub fn run(argv: &[String]) -> Result<ExitCode, String> {
    let args = Args::parse(argv, &["--start", "--words"])?;
    let path = args.one_positional("the image file")?;
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path, e))?;
    if bytes.len() % 8 != 0 {
        eprintln!(
            "cray-xmp dis: {}: {} bytes is not a whole number of words; the last {} bytes are ignored",
            path,
            bytes.len(),
            bytes.len() % 8
        );
    }
    let words: Vec<u64> = bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| u64::from_be_bytes(*c))
        .collect();
    let total = words.len() as u64;
    let start = args.number("--start")?.unwrap_or(0);
    if start > total {
        return Err(format!(
            "--start {} is past the end of the image ({} words)",
            start, total
        ));
    }
    let end = match args.number("--words")? {
        Some(n) => start.saturating_add(n).min(total),
        None => total,
    };
    print!("{}", disassemble(&words, start, end));
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(parcels: [u16; 4]) -> u64 {
        parcels.iter().fold(0, |acc, p| (acc << 16) | *p as u64)
    }

    #[test]
    fn straddles_zero_runs_and_ignored_bits() {
        let words = [
            word([0o030123, 0o003123, 0o001000, 0o006000]),
            word([0o000100, 0o020100, 0o000005, 0o054012]),
            0,
            0,
            0,
            word([0, 0, 0, 0o100200]),
        ];
        let text = disassemble(&words, 0, 6);
        let want = "\
0000000a  030123         A1        A2+A3
0000000b  003123         VM        S2               ; ignored bits set
0000000c  001000         PASS
0000000d  006000 000100  J         100              ; straddles words
0000001b  020100 000005  A1        #-6
0000001d  054012         VWD       D'16/O'054012
0000002   3 zero words
0000005a  000000         ERR
0000005b  000000         ERR
0000005c  000000         ERR
0000005d  100200 ------  A2        0,0              ; no second parcel
";
        assert_eq!(text, want);
        // a range that ends inside an instruction takes the second parcel from beyond it
        assert_eq!(
            disassemble(&words, 0, 1).lines().last().unwrap(),
            "0000000d  006000 000100  J         100              ; straddles words"
        );
        assert_eq!(
            disassemble(&words, 1, 2).lines().next().unwrap(),
            "0000001a  000100         ERR       100"
        );
    }
}

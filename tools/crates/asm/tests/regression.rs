//! Regression against the prior art in tests/asm: the CAL manual examples of
//! cal_dv.cal, the bootloader boot.cal with the output of the old Python
//! assembler (boot.SV.txt), and a round trip of every instruction form.

use cray1_asm::{assemble_file, assemble_source, Assembly};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/asm")
        .join(name)
}

fn assemble_fixture(name: &str) -> Assembly {
    assemble_file(&fixture(name), &[]).unwrap_or_else(|e| panic!("{}", e))
}

/// A word from its four parcels, parcel 0 first.
fn word(parcels: [u16; 4]) -> u64 {
    parcels.iter().fold(0, |acc, p| (acc << 16) | *p as u64)
}

fn octal(parcels: &[u16]) -> String {
    parcels
        .iter()
        .map(|p| format!("{:06o}", p))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The expected parcels of a cal_dv line: the six-digit octal groups that
/// open its comment (the text after the operand field).
fn expected_parcels(text: &str) -> Vec<u16> {
    // result at column 10, operand at column 20, comment from column 35
    let comment = text.get(34..).unwrap_or("");
    comment
        .split_whitespace()
        .take_while(|t| t.len() == 6 && t.bytes().all(|c| (b'0'..=b'7').contains(&c)))
        .map(|t| u16::from_str_radix(t, 8).unwrap())
        .collect()
}

/// cal_dv lines whose expected parcels we deliberately do not produce:
/// line number, what we emit, why.
const CAL_DV_DIFFERENCES: &[(u32, &str, &str)] = &[(
    82,
    "042200",
    "`S2 -1`: Appendix D gives the special form 042i00 (enter -1 into Si); cal_dv expects the two-parcel 041200 000000",
)];

#[test]
fn cal_dv_examples() {
    let a = assemble_fixture("cal_dv.cal");
    assert_eq!(a.diagnostics, [], "cal_dv.cal must assemble cleanly");
    assert_eq!(a.ident.as_deref(), Some("CALDV"));
    assert_eq!(a.entries, ["START"]);
    let mut checked = 0;
    let mut data_checked = 0;
    let mut differences = Vec::new();
    let mut unchecked = Vec::new();
    for l in &a.listing {
        if l.text.starts_with('*') {
            continue; // comment lines, including the NOT SUPPORTED notes
        }
        let want = expected_parcels(&l.text);
        if want.is_empty() {
            // directives carry no expected code
            assert!(
                l.parcels.is_empty(),
                "line {} emits code but has no expected parcels: {}",
                l.line,
                l.text
            );
            unchecked.push(l.line);
            continue;
        }
        if !l.words.is_empty() {
            // CON: cal.py lists the least significant parcel first and only
            // the non-zero ones; compare the word value
            let value = want
                .iter()
                .rev()
                .fold(0u64, |acc, p| (acc << 16) | *p as u64);
            assert_eq!(l.words, [value], "line {}: {}", l.line, l.text);
            data_checked += 1;
        } else if l.parcels == want {
            checked += 1;
        } else {
            differences.push((l.line, octal(&l.parcels)));
        }
    }
    let known: Vec<(u32, String)> = CAL_DV_DIFFERENCES
        .iter()
        .map(|(line, ours, _)| (*line, ours.to_string()))
        .collect();
    assert_eq!(
        differences, known,
        "lines that disagree with their expected parcels"
    );
    // IDENT, ORG, ENTRY, BSSZ and five equates carry no expected code
    assert_eq!(unchecked, [1, 2, 3, 5, 6, 7, 24, 41, 45]);
    assert_eq!(data_checked, 1, "the CON on line 4");
    assert_eq!(
        (checked, differences.len()),
        (214, 1),
        "instruction lines that match and that differ"
    );
    // ORG O'100 is a word address; STRTVAL is data, START is code
    let sym = |name: &str| a.symbol(name).map(|s| (s.value, s.attr.letter()));
    assert_eq!(sym("STRTVAL"), Some((0o100, 'W')));
    assert_eq!(sym("CON1"), Some((0o4520, 'V')));
    assert_eq!(a.words[0o100], 64);
    assert_eq!(&a.words[0o101..0o105], [0; 4]);
    // code starts after the BSSZ block with ERR, A2 D'64, VL A2
    assert_eq!(
        a.words[0o105],
        word([0o000000, 0o020200, 0o000100, 0o002002])
    );
    let start = sym("START").unwrap();
    assert_eq!(start.1, 'P');
    assert_eq!(a.parcel(start.0 as u64), 0o031102);
}

/// The numeric lines of a .SV file from cal.py: one parcel per line after a
/// five line header, the least significant parcel of each word first.
fn sv_parcels(text: &str) -> Vec<u16> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.len() == 6 && l.bytes().all(|c| c.is_ascii_digit()))
        .map(|l| u16::from_str_radix(l, 8).unwrap())
        .collect()
}

#[test]
fn boot_matches_the_python_assembler() {
    let a = assemble_fixture("boot.cal");
    assert_eq!(a.diagnostics, []);
    let sv = std::fs::read_to_string(fixture("boot.SV.txt")).unwrap();
    assert!(sv.contains("PROGRAM LENGTH:130"));
    let want = sv_parcels(&sv);
    assert_eq!(want.len(), 130);

    // Sixteen CON words of exchange package.  cal.py wrote data words least
    // significant parcel first; our image holds them as ordinary words.
    for w in 0..16 {
        let value = (0..4)
            .rev()
            .fold(0u64, |acc, p| (acc << 16) | want[4 * w + p] as u64);
        assert_eq!(a.words[w], value, "word {}", w);
    }
    assert_eq!(a.words[0], 0o10000000000);
    assert_eq!(
        a.words[0] >> 24,
        64,
        "P field of the exchange package is the entry parcel"
    );

    // Instruction parcels 64 to 129, in memory order.
    let ours: Vec<u16> = (64..130).map(|p| a.parcel(p)).collect();
    assert_eq!(octal(&ours), octal(&want[64..130]));
    // packed four to a word, parcel 0 in bits 63 to 48, last word zero padded
    assert_eq!(a.words.len(), 33);
    assert_eq!(a.words[16], word([0o120010, 0o000001, 0o014000, 0o000100]));
    assert_eq!(a.words[32], word([0o006000, 0o000100, 0, 0]));
    assert_eq!(a.image().len(), 33 * 8);
    assert_eq!(
        &a.image()[128..136],
        [0xa0, 0x08, 0x00, 0x01, 0x18, 0x00, 0x00, 0x40]
    );

    let sym = |name: &str| a.symbol(name).map(|s| (s.value, s.attr.letter()));
    assert_eq!(sym("RX_SPIN"), Some((64, 'P')));
    assert_eq!(sym("STRT_LD"), Some((0o127, 'P')));
    assert_eq!(sym("PBASE"), Some((512, 'V')));
    assert_eq!(a.entries, ["RX_SPIN"]);
}

/// A CAL source that uses every row of the instruction table once, written
/// from the table's own examples.
fn all_forms_source() -> String {
    let mut out = String::new();
    out.push_str("* Every instruction form of the Cray-1 once: one line per row of the\n");
    out.push_str("* instruction table (cray1 isa), in table order, with the expected parcels\n");
    out.push_str("* in the comment column.  Checked against the table by the regression test\n");
    out.push_str("* in tools/crates/asm; regenerate with CRAY1_UPDATE_FIXTURES=1 cargo test.\n");
    out.push_str("         IDENT     ALLFORMS\n");
    for f in cray1_isa::FORMS {
        let e = cray1_isa::encode(f, f.example_fields());
        let (result, operand) = match f.example() {
            Some(x) => x,
            None => ("VWD".to_string(), format!("D'16/O'{:06o}", e.parcel0)),
        };
        out.push_str(&format!(
            "         {:<9} {:<14} {}\n",
            result,
            operand,
            octal(&e.parcels())
        ));
    }
    out.push_str("         END\n");
    out
}

#[test]
fn every_instruction_form_round_trips() {
    let path = fixture("all_forms.cal");
    let source = all_forms_source();
    if std::env::var_os("CRAY1_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &source).unwrap();
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk, source,
        "tests/asm/all_forms.cal is out of date with the instruction table"
    );

    let a = assemble_fixture("all_forms.cal");
    assert_eq!(a.diagnostics, []);

    // every line assembles to the parcels in its comment column
    let mut lines = 0;
    let mut parcels = Vec::new();
    for l in a.listing.iter().filter(|l| !l.parcels.is_empty()) {
        assert_eq!(
            octal(&l.parcels),
            octal(&expected_parcels(&l.text)),
            "line {}: {}",
            l.line,
            l.text
        );
        parcels.extend(&l.parcels);
        lines += 1;
    }
    assert_eq!(lines, cray1_isa::FORMS.len());
    let total = parcels.len() as u64;
    assert_eq!((0..total).map(|p| a.parcel(p)).collect::<Vec<_>>(), parcels);

    // the image decodes to one instruction per line, each table row in turn
    let mut addr = 0;
    let mut straddles = 0;
    let mut text = String::from("         IDENT     AGAIN\n");
    for f in cray1_isa::FORMS {
        let p0 = a.parcel(addr);
        let len = cray1_isa::length(p0) as u64;
        let d = cray1_isa::decode(p0, (len == 2).then(|| a.parcel(addr + 1)));
        assert_eq!(d.op, f.op, "parcel {:o}: {}", addr, f.pattern);
        assert!(
            f.matches(d.parcel0, Some(d.m)),
            "parcel {:o}: {}",
            addr,
            f.pattern
        );
        straddles += (len == 2 && addr % 4 == 3) as u32;
        text.push_str(&format!("         {}\n", cray1_isa::disassemble(&d)));
        addr += len;
    }
    assert_eq!(addr, total);
    assert!(
        straddles > 0,
        "some two-parcel instruction should straddle a word boundary"
    );

    // and the disassembly assembles back to the same image
    let b = assemble_source(&text);
    assert_eq!(b.diagnostics, []);
    assert_eq!(b.words, a.words);
}

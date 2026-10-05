//! Behaviour of the assembler proper: source layout, expressions, address
//! attributes, directives, macros, includes, diagnostics and output files.

use cray1_asm::{assemble, assemble_source, Assembly, Severity};

fn ok(source: &str) -> Assembly {
    let a = assemble_source(source);
    assert_eq!(a.diagnostics, [], "source:\n{}", source);
    a
}

/// The error messages of a source that must not assemble.
fn errors(source: &str) -> Vec<(u32, String)> {
    let a = assemble_source(source);
    assert!(a.has_errors(), "expected errors in:\n{}", source);
    assert!(a.words.is_empty(), "no image on error");
    a.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.line, d.message.clone()))
        .collect()
}

fn one_error(source: &str) -> String {
    let e = errors(source);
    assert_eq!(e.len(), 1, "{:?}", e);
    e[0].1.clone()
}

fn parcels(a: &Assembly, from: u64, count: u64) -> Vec<u16> {
    (from..from + count).map(|p| a.parcel(p)).collect()
}

fn word(parcels: [u16; 4]) -> u64 {
    parcels.iter().fold(0, |acc, p| (acc << 16) | *p as u64)
}

fn sym(a: &Assembly, name: &str) -> (i64, char) {
    let s = a
        .symbol(name)
        .unwrap_or_else(|| panic!("no symbol {}", name));
    (s.value, s.attr.letter())
}

#[test]
fn code_is_packed_four_parcels_to_a_word_most_significant_first() {
    let a = ok(" A1 A2+A3\n S1 S2&S3\n J 20\n EX\n PASS\n");
    assert_eq!(
        a.words,
        [
            word([0o030123, 0o044123, 0o006000, 0o000020]),
            word([0o004000, 0o001000, 0, 0])
        ]
    );
    assert_eq!(
        a.image(),
        [0x30, 0x53, 0x48, 0x53, 0x0c, 0x00, 0x00, 0x10, 0x08, 0x00, 0x02, 0x00, 0, 0, 0, 0]
    );
}

#[test]
fn two_parcel_instructions_straddle_words_and_labels_are_parcel_addresses() {
    let a = ok("\
START    PASS
         PASS
         PASS
FAR      J         FAR
NEXT     A1        NEXT
");
    assert_eq!(
        a.words,
        [
            word([0o001000, 0o001000, 0o001000, 0o006000]),
            word([0o000003, 0o022105, 0, 0])
        ]
    );
    assert_eq!(sym(&a, "START"), (0, 'P'));
    assert_eq!(sym(&a, "FAR"), (3, 'P'));
    assert_eq!(sym(&a, "NEXT"), (5, 'P'));
    // the listing shows the straddling instruction at word 0 parcel d
    assert!(a.listing_text().contains("0000000d  006000 000003"));
    assert!(a.listing_text().contains("0000001b  022105"));
}

#[test]
fn data_starts_on_a_word_boundary_behind_pass_instructions() {
    // CAL fills the unused parcels of a word of code with `S1 S1&S1` (044111), so a
    // program can run through the boundary.  ALIGN fills with zero and goes on at the next
    // instruction buffer boundary, 20 octal words.
    const P: u16 = 0o044111;
    let a = ok("         A1        1
TABLE    CON       5,6
         A2        2
BUF      BSS       2
         A3        3
Z        BSSZ      1
         A4        4
         ALIGN
END1     A5        5
");
    assert_eq!(
        a.words,
        [
            word([0o022101, P, P, P]),
            5,
            6,
            word([0o022202, P, P, P]),
            0,
            0,
            word([0o022303, P, P, P]),
            0,
            word([0o022404, 0, 0, 0]),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            word([0o022505, 0, 0, 0]),
        ]
    );
    assert_eq!(sym(&a, "END1"), (0o20 * 4, 'P'));
    // 40 octal words on an X-MP
    let x = ok("         MACHINE   XMP
         A1        1
         ALIGN
HERE     A2        2
");
    assert_eq!(sym(&x, "HERE"), (0o40 * 4, 'P'));
    // raw parcels are data: zero behind them
    let v = ok("         VWD       D'16/O'123456
T        CON       7
");
    assert_eq!(v.words, [word([0o123456, 0, 0, 0]), 7]);
    assert_eq!(sym(&a, "TABLE"), (1, 'W'));
    assert_eq!(sym(&a, "BUF"), (4, 'W'));
    assert_eq!(sym(&a, "Z"), (7, 'W'));
}

#[test]
fn image_runs_from_word_zero_to_the_highest_word_used() {
    let a = ok(" ORG 3\n CON 7\n ORG 1\n CON 5\n");
    assert_eq!(a.words, [0, 5, 0, 7]);
    // reserved words at the end count as used
    let a = ok(" CON 1\n BSS 3\n");
    assert_eq!(a.words, [1, 0, 0, 0]);
    let a = ok(" ORG 2\nL EX\n");
    assert_eq!(a.words, [0, 0, word([0o004000, 0, 0, 0])]);
    assert_eq!(sym(&a, "L"), (8, 'P'));
    assert_eq!(ok("* nothing\n").words, []);
}

#[test]
fn numbers_and_character_constants() {
    let a = ok(
        " CON 17,D'17,O'17,X'1f,X'FFFFFFFFFFFFFFFF,1777777777777777777777
 CON 'A','AB','ABCDEFGH',A'AB','A'L,'A'H,'A'R,'it''s',''
 CON -1,+5,#0,#5,-D'10
",
    );
    assert_eq!(&a.words[..6], [0o17, 17, 0o17, 0x1f, u64::MAX, u64::MAX]);
    assert_eq!(
        &a.words[6..15],
        [
            0x41,
            0x4142,
            0x4142434445464748,
            0x4142,
            0x41 << 56,
            0x4120202020202020,
            0x41,
            0x69742773,
            0
        ]
    );
    assert_eq!(&a.words[15..], [u64::MAX, 5, u64::MAX, !5, -10i64 as u64]);
    assert!(one_error(" CON 18\n").contains("not an octal number"));
    assert!(one_error(" CON D'1x\n").contains("not a decimal number"));
    assert!(one_error(" CON 'ABCDEFGHI'\n").contains("longer than 8"));
    assert!(one_error(" CON 'ABC\n").contains("no closing quote"));
}

#[test]
fn expression_operators() {
    let a = ok("\
N        =         D'10
M        =         N*2+1
 CON N+1,N-1,N*3,N/3,2+3*4,(2+3)*4,-N+1,N-M,D'100/N/2
");
    assert_eq!(sym(&a, "M"), (21, 'V'));
    assert_eq!(
        a.words[..9],
        [11, 9, 30, 3, 14, 20, -9i64 as u64, -11i64 as u64, 5]
    );
    assert!(one_error(" CON 1/0\n").contains("division by zero"));
    assert!(one_error(" CON 1+\n").contains("expression ends"));
    assert!(one_error(" CON (1+2\n").contains("missing `)`"));
    assert!(one_error(" CON 1+&2\n").contains("unexpected"));
    assert!(one_error(" CON NOPE\n").contains("undefined symbol NOPE"));
}

#[test]
fn location_counter_and_address_attributes() {
    let a = ok("         ORG       10
CODE     PASS
MID      PASS
DATA1    CON       0
V        =         5
WV       =         W.MID
PW       =         P.DATA1
D1       =         DATA1-CODE
D2       =         MID-CODE
D3       =         DATA1+1
D4       =         MID+V
HERE     CON       *,W.*,P.*,*-CODE,W.*-DATA1,DATA1,MID,W.MID,P.DATA1,P.V,W.V
");
    assert_eq!(sym(&a, "CODE"), (0o40, 'P'));
    assert_eq!(sym(&a, "MID"), (0o41, 'P'));
    assert_eq!(sym(&a, "DATA1"), (0o11, 'W'));
    assert_eq!(sym(&a, "WV"), (0o10, 'W'));
    assert_eq!(sym(&a, "PW"), (0o44, 'P'));
    // word address minus parcel address: the word address counts in parcels
    assert_eq!(sym(&a, "D1"), (4, 'V'));
    assert_eq!(sym(&a, "D2"), (1, 'V'));
    assert_eq!(sym(&a, "D3"), (0o12, 'W'));
    assert_eq!(sym(&a, "D4"), (0o46, 'P'));
    assert_eq!(sym(&a, "HERE"), (0o12, 'W'));
    assert_eq!(
        &a.words[0o12..],
        [0o50, 0o12, 0o50, 0o10, 1, 0o11, 0o41, 0o10, 0o44, 5, 5]
    );
}

#[test]
fn branches_take_parcel_addresses_and_memory_references_word_addresses() {
    let a = ok("         ORG       10
CODE     J         CODE
         J         TABLE
         J         P.TABLE+2
         J         100
         A1        TABLE,A2
         A1        CODE,
         A1        W.CODE,
         A1        CODE
         A1        TABLE
         CODE,0    S1
TABLE    CON       0
");
    let table = sym(&a, "TABLE").0;
    assert_eq!(table, 0o15);
    assert_eq!(
        parcels(&a, 0o40, 20),
        [
            0o006000, 0o000040, // J CODE
            0o006000, 0o000064, // J TABLE: word address * 4
            0o006000, 0o000066, // J P.TABLE+2
            0o006000, 0o000100, // J 100: a plain value is a parcel address
            0o102100, 0o000015, // A1 TABLE,A2
            0o100100, 0o000010, // A1 CODE,: parcel address / 4
            0o100100, 0o000010, // A1 W.CODE,
            0o022140, // A1 CODE: the value as it is
            0o020100,
            0o000015, // A1 TABLE: the word address (two parcels, TABLE is defined later)
            0o130100, 0o000010, // CODE,0 S1
            0o044111, // a pass instruction fills the word before the CON
        ]
    );
}

#[test]
fn a_parcel_address_off_a_word_boundary_used_as_a_word_address_warns() {
    let a = assemble_source(" PASS\nODD PASS\n S1 ODD,\n S2 W.ODD,\n");
    assert_eq!(a.diagnostics.len(), 1);
    let d = &a.diagnostics[0];
    assert_eq!((d.severity, d.line), (Severity::Warning, 3));
    assert!(
        d.message.contains("not on a word boundary"),
        "{}",
        d.message
    );
    assert_eq!(d.to_string(), format!("<source>:3: warning: {}", d.message));
    // a warning does not stop the image
    assert!(a.has_warnings() && !a.has_errors());
    assert_eq!(parcels(&a, 2, 4), [0o120100, 0, 0o120200, 0]);
}

#[test]
fn forward_references() {
    let a = ok("         A1        SMALL
         A2        BACK
BACK     =         5
         A3        BACK
         A4        LATER
         A5        CHAIN
         S1        SMALL
         JAZ       LATER
         S2        <SMALL
LATER    EX
SMALL    =         7
CHAIN    =         LAST-1
LAST     =         D'9
");
    assert_eq!(
        parcels(&a, 0, 16),
        [
            0o020100, 7, // forward: two parcels although 7 fits 022
            0o020200, 5,        // BACK is defined below this line
            0o022305, // defined above: one parcel
            0o020400, 0o16, // LATER is parcel 14
            0o020500, 8, // CHAIN defined below, itself from a forward reference
            0o040100, 7, 0o010000, 0o16, 0o042271, // mask of 7 bits from the right
            0o004000, 0,
        ]
    );
    assert_eq!(sym(&a, "CHAIN"), (8, 'V'));
    assert_eq!(sym(&a, "LATER"), (0o16, 'P'));
    // an equate that uses a forward equate before it keeps its two parcels
    let a = ok("X = Y+1\n A1 X\nY = 2\n A2 Y\n");
    assert_eq!(parcels(&a, 0, 3), [0o020100, 3, 0o022202]);
}

#[test]
fn circular_and_undefined_symbols_are_errors() {
    let e = errors("X = Y\nY = X\n");
    assert_eq!(e.len(), 2);
    assert!(e[0].1.contains("has no value"), "{:?}", e);
    let e = errors(" A1 NOWHERE\n J NOWHERE\n");
    assert_eq!(e.iter().map(|x| x.0).collect::<Vec<_>>(), [1, 2]);
    assert!(e[0].1.contains("undefined symbol NOWHERE"));
    assert!(one_error("L PASS\nL PASS\n").contains("already defined on line 1"));
    assert!(one_error("A1 PASS\n").contains("register name"));
    assert!(one_error("VM = 5\n").contains("register name"));
    assert!(one_error("1ABC PASS\n").contains("not a valid symbol"));
    assert!(one_error(" = 5\n").contains("needs a symbol"));
    assert!(one_error(" ENTRY NOPE\n").contains("ENTRY symbol NOPE is not defined"));
}

#[test]
fn directives_that_move_the_location_counter_need_known_operands() {
    assert!(one_error(" ORG LATER\nLATER = 5\n").contains("not defined yet"));
    assert!(one_error(" BSS LATER\nLATER = 5\n").contains("not defined yet"));
    assert!(one_error(" BSSZ -1\n").contains("out of range"));
    assert!(one_error(" ORG 20000000\n").contains("out of range"));
    assert!(one_error(" PASS\nL PASS\n ORG L\n").contains("not on a word boundary"));
    // a parcel address on a word boundary is fine for ORG
    let a = ok(" ORG 2\nL PASS\n ORG 5\n CON 1\n ORG L\n EX\n");
    assert_eq!(a.words[2], word([0o004000, 0o044111, 0o044111, 0o044111]));
}

#[test]
fn data_directive() {
    let a = ok("\
MSG      DATA      'HELLO, WORLD'
         DATA      'ABCDEFGH','ABCDEFGHI',D'10,'AB'H,'AB'R,'A'+1
NEXT     CON       0
");
    assert_eq!(a.words[0], u64::from_be_bytes(*b"HELLO, W"));
    assert_eq!(a.words[1], u64::from_be_bytes(*b"ORLD\0\0\0\0"));
    assert_eq!(a.words[2], u64::from_be_bytes(*b"ABCDEFGH"));
    assert_eq!(a.words[3], u64::from_be_bytes(*b"ABCDEFGH"));
    assert_eq!(a.words[4], u64::from_be_bytes(*b"I\0\0\0\0\0\0\0"));
    assert_eq!(a.words[5], 10);
    assert_eq!(a.words[6], u64::from_be_bytes(*b"AB      "));
    assert_eq!(a.words[7], 0x4142);
    assert_eq!(a.words[8], 0x42);
    assert_eq!(sym(&a, "MSG"), (0, 'W'));
    assert_eq!(sym(&a, "NEXT"), (9, 'W'));
    assert!(one_error(" DATA\n").contains("needs a value"));
}

#[test]
fn vwd_puts_raw_parcels_in_the_code_stream() {
    let a = ok(" PASS\nRAW VWD D'16/O'054012,D'32/O'1234567,20/-1\n PASS\n");
    assert_eq!(
        parcels(&a, 0, 6),
        [0o001000, 0o054012, 0o000005, 0o034567, 0o177777, 0o001000]
    );
    assert_eq!(sym(&a, "RAW"), (1, 'P'));
    assert!(one_error(" VWD 16/1\n").contains("VWD width 14"));
    assert!(one_error(" VWD D'16/O'200000\n").contains("does not fit"));
    assert!(one_error(" VWD 5\n").contains("not width/value"));
    let a = ok(" VWD D'64/X'0123456789ABCDEF,D'48/1\n");
    assert_eq!(a.words[0], 0x0123456789abcdef);
    assert_eq!(a.words[1], 1 << 16);
}

#[test]
fn ident_entry_ext_end_and_ignored_directives() {
    let a = ok("         IDENT     DEMO
         ENTRY     GO,STOP
         EXT       OUTSIDE,OTHER
         ABS
         TITLE     'whatever'
         LIST
         NOLIST
GO       A1        OUTSIDE
         J         OTHER
STOP     EX
         END
         this line is not read
");
    assert_eq!(a.ident.as_deref(), Some("DEMO"));
    assert_eq!(a.entries, ["GO", "STOP"]);
    assert_eq!(sym(&a, "OUTSIDE"), (0, 'X'));
    assert_eq!(parcels(&a, 0, 4), [0o022100, 0o006000, 0, 0o004000]);
    assert_eq!(a.listing.len(), 11);
    assert!(one_error("X IDENT Y\n").contains("cannot have a label"));
}

#[test]
fn a_label_on_its_own_line_names_the_next_statement() {
    let a = ok("         PASS
LOOP
         J         LOOP
TABLE
ALSO
         CON       1
TAIL
");
    assert_eq!(sym(&a, "LOOP"), (1, 'P'));
    assert_eq!(sym(&a, "TABLE"), (1, 'W'));
    assert_eq!(sym(&a, "ALSO"), (1, 'W'));
    assert_eq!(sym(&a, "TAIL"), (8, 'P'));
}

#[test]
fn register_designators_by_symbol() {
    let a = ok("\
RTN      =         17
TMP      =         66
R        =         3
         J         B.RTN
         A5        B.RTN
         B.RTN     A6
         S5        T.TMP
         T.TMP     S5
         B.RTN,A4  ,A0
         A.R       A.R+A.R
         S.R       S.R&S.R
         V.R       V.R&V.R
         A1        B.7
");
    assert_eq!(
        parcels(&a, 0, 10),
        [
            0o005017, 0o024517, 0o025617, 0o074566, 0o075566, 0o034417, 0o030333, 0o044333,
            0o141333, 0o024107
        ]
    );
    assert!(one_error("BIG = 100\n J B.BIG\n").contains("out of range"));
    assert!(one_error(" J B.NOPE\n").contains("undefined symbol NOPE"));
}

#[test]
fn instruction_errors_carry_the_line_and_stop_the_image() {
    let e = errors(" PASS\n FOO BAR\n A1 20000000\n S1 S2<S3\n S1 <200\n A1 1,2,3\n");
    assert_eq!(e.iter().map(|x| x.0).collect::<Vec<_>>(), [2, 3, 4, 5, 6]);
    assert!(e[0].1.contains("no instruction form matches `FOO BAR`"));
    assert!(e[1].1.contains("does not fit"));
    assert!(e[2].1.contains("no instruction form matches"));
    assert!(e[3].1.contains("out of range"));
    let a = assemble_source(" PASS\n FOO BAR\n");
    assert_eq!(
        a.diagnostics[0].to_string(),
        "<source>:2: error: no instruction form matches `FOO BAR`"
    );
    assert!(a.words.is_empty());
    // the listing still shows what went wrong
    assert!(a
        .listing_text()
        .contains("***** error: no instruction form matches `FOO BAR`"));
    assert!(a.listing_text().contains("1 errors, 0 warnings"));
}

#[test]
fn an_operand_must_start_before_column_35() {
    // CAL layout: with no operand, the comment starts at column 35
    let a = ok("         EX                       27 is a comment here\n         EX       27\n");
    assert_eq!(parcels(&a, 0, 2), [0o004000, 0o004027]);
    // an instruction that needs its operand says where it went
    let e = one_error("                              A1  A2+A3\n");
    assert!(
        e.contains("no instruction form matches `A1`") && e.contains("before column 35"),
        "{}",
        e
    );
    // the result field itself may start anywhere
    let a = ok("\t\t\t\t\tPASS\n");
    assert_eq!(a.parcel(0), 0o001000);
}

#[test]
fn macros_substitute_text() {
    let a = ok("         MACRO     SWAP R1,R2,TMP
         TMP       R1
         R1        R2
         R2        TMP
         ENDM
         MACRO     LOADC REG,VALUE     load a constant
         REG       VALUE
         ENDM
         MACRO     STORE REG,INDEX
         0,INDEX   REG
         ENDM
         MACRO     HALT
         EX
         ENDM
FIRST    SWAP      S1,S2,S3
         LOADC     A1,D'100
         LOADC     S4,(<5)
         LOADC     V1,(,A0,A2)
         STORE     S1,A2
         STORE     S1
         HALT
");
    assert_eq!(
        parcels(&a, 0, 12),
        [
            0o051301, 0o051102, 0o051203, 0o020100, 100, 0o042473, 0o176102, 0o132100, 0, 0o130100,
            0, 0o004000
        ]
    );
    assert_eq!(sym(&a, "FIRST"), (0, 'P'));
    // expanded lines are in the listing, marked with their depth
    let expanded: Vec<&str> = a
        .listing
        .iter()
        .filter(|l| l.depth == 1)
        .map(|l| l.text.trim())
        .collect();
    assert_eq!(expanded.len(), 9);
    assert_eq!(expanded[6], "0,A2   S1");
    assert_eq!(expanded[7], "0,   S1");
    assert!(a.listing_text().contains("+"));
}

#[test]
fn macros_with_local_labels_nesting_and_the_cal_prototype_layout() {
    let a = ok("         MACRO
LBL      COUNT     REG,LIMIT
         LOCAL     LOOP
LBL      REG       0
LOOP     REG       REG+1
         A0        REG-LIMIT
         JAN       LOOP
COUNT    ENDM
         MACRO     TWICE REG
         COUNT     REG,A1
         COUNT     REG,A2
         ENDM
ONE      COUNT     A5,A1
         TWICE     A6
");
    // three expansions, each with its own LOOP
    assert_eq!(
        parcels(&a, 0, 5),
        [0o022500, 0o030550, 0o031051, 0o011000, 1]
    );
    assert_eq!(
        parcels(&a, 5, 5),
        [0o022600, 0o030660, 0o031061, 0o011000, 6]
    );
    assert_eq!(
        parcels(&a, 10, 5),
        [0o022600, 0o030660, 0o031062, 0o011000, 11]
    );
    assert_eq!(sym(&a, "ONE"), (0, 'P'));
    assert_eq!(
        a.symbols
            .iter()
            .filter(|s| s.name.starts_with("LOOP%"))
            .count(),
        3
    );
    assert_eq!(a.listing.iter().map(|l| l.depth).max(), Some(2));
}

#[test]
fn macro_errors() {
    assert!(one_error(" MACRO M A\n PASS\n ENDM\n M 1,2\n").contains("takes 1 arguments, 2 given"));
    assert!(one_error(" MACRO M\n PASS\n").contains("has no ENDM"));
    assert!(one_error(" ENDM\n").contains("ENDM without MACRO"));
    assert!(one_error(" MACRO M\n M\n ENDM\n M\n").contains("nested too deeply"));
    let a = assemble_source(" MACRO BAD R\n R NOPE\n ENDM\n PASS\n BAD A1\n");
    assert_eq!(a.diagnostics.len(), 1);
    assert_eq!(a.diagnostics[0].line, 5);
    assert!(
        a.diagnostics[0]
            .message
            .contains("undefined symbol NOPE (in macro BAD: A1 NOPE)"),
        "{}",
        a.diagnostics[0].message
    );
}

#[test]
fn include_files_come_from_the_resolver() {
    let mut asked = Vec::new();
    let mut resolver = |name: &str, from: &str| -> Result<(String, String), String> {
        asked.push((name.to_string(), from.to_string()));
        match name {
            "defs.cal" => Ok((
                "lib/defs.cal".to_string(),
                "FIVE = 5\n INCLUDE 'more.cal'\n".to_string(),
            )),
            "more.cal" => Ok((
                "lib/more.cal".to_string(),
                " MACRO HALT\n EX\n ENDM\nSIX = 6\n BAD LINE\n".to_string(),
            )),
            _ => Err("file not found".to_string()),
        }
    };
    let a = assemble(
        "main.cal",
        " INCLUDE \"defs.cal\"\n A1 FIVE+SIX\n HALT\n INCLUDE missing.cal\n",
        &mut resolver,
    );
    assert_eq!(
        asked,
        [
            ("defs.cal".to_string(), "main.cal".to_string()),
            ("more.cal".to_string(), "lib/defs.cal".to_string()),
            ("missing.cal".to_string(), "main.cal".to_string())
        ]
    );
    let messages: Vec<String> = a.diagnostics.iter().map(|d| d.to_string()).collect();
    assert_eq!(
        messages,
        [
            "lib/more.cal:5: error: no instruction form matches `BAD LINE`",
            "main.cal:4: error: cannot include \"missing.cal\": file not found",
        ]
    );
    assert!(a.listing_text().contains("* file lib/more.cal"));

    let mut resolver = |_: &str, _: &str| Ok(("x.cal".to_string(), "N = 13\n".to_string()));
    let a = assemble("main.cal", " INCLUDE x.cal\n A1 N\n", &mut resolver);
    assert_eq!(a.diagnostics, []);
    assert_eq!(a.words, [word([0o022113, 0, 0, 0])]);

    let mut forever =
        |_: &str, _: &str| Ok(("loop.cal".to_string(), " INCLUDE loop.cal\n".to_string()));
    let a = assemble("loop.cal", " INCLUDE loop.cal\n", &mut forever);
    assert!(a.diagnostics[0].message.contains("nested too deeply"));
}

#[test]
fn listing_and_symbol_file_formats() {
    let a = ok("\
* demo
N        =         D'8
START    A1        N
         S1        TABLE,A1
         NOLIST
         PASS
         LIST
TABLE    CON       -1
");
    let listing = a.listing_text();
    let want = "                                     1  * demo
          = 10                       2  N        =         D'8
0000000a  022110                     3  START    A1        N
0000000b  121100 000001              4           S1        TABLE,A1
                                     5           NOLIST
                                     7           LIST
0000001   1777777777777777777777     8  TABLE    CON       -1

0 errors, 0 warnings

Symbols (V value, W word address, P parcel address, X external)
N                      10 V
START                   0 P
TABLE                   1 W
";
    assert_eq!(listing, want);
    assert_eq!(
        a.symbols_text(),
        "N                      10 V\nSTART                   0 P\nTABLE                   1 W\n"
    );
}

#[test]
fn every_documented_choice_of_encoding() {
    let a = ok("         A1        77
         A1        100
         A1        -1
         A1        -2
         A1        #2
         S1        0
         S1        1
         S1        -1
         S1        2
         S1        -2
         S1        #2
         S0        S0<3
         S1        S1<100
         S1        <0
");
    assert_eq!(
        parcels(&a, 0, 21),
        [
            0o022177, 0o020100, 0o100, 0o031100, 0o021100, 1, 0o021100, 2, 0o043100, 0o042177,
            0o042100, 0o040100, 2, 0o041100, 1, 0o041100, 2, 0o052003, 0o055100, 0o043100, 0
        ]
    );
}

/// A small deterministic generator, enough to shake the parser.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }
}

#[test]
fn garbage_never_panics() {
    let pieces = [
        "A1", "S2", "V3", "B17", "T.X", "A.", "S0", "VM", "VL", "SB", "RT", "CI", "CA", "J", "R",
        "JAZ", "EX", "ERR", "PASS", "CON", "DATA", "BSS", "BSSZ", "ORG", "ALIGN", "VWD", "MACRO",
        "ENDM", "LOCAL", "INCLUDE", "END", "IDENT", "ENTRY", "EXT", "=", "M", "LBL", "X", "*", "+",
        "-", "/", "#", "<", ">", "&", "!", "\\", ",", ".", "(", ")", "'", "''", "D'", "O'", "X'",
        "A'", "W.", "P.", "0", "1", "7", "8", "64", "100", "177777", "0.6", "1.", " ", " ", " ",
        "  ", "\t", ";", "é", "\u{0}",
    ];
    let mut rng = Lcg(0x1975_0000_c4a1);
    for round in 0..300 {
        let mut source = String::new();
        for _ in 0..rng.next(40) + 1 {
            for _ in 0..rng.next(9) {
                source.push_str(pieces[rng.next(pieces.len())]);
            }
            source.push('\n');
        }
        let mut include = |name: &str, _: &str| -> Result<(String, String), String> {
            if name.len().is_multiple_of(2) {
                Ok((
                    name.to_string(),
                    " MACRO M P\n P 1\n ENDM\nX = 5\n".to_string(),
                ))
            } else {
                Err("no".to_string())
            }
        };
        let a = assemble("fuzz.cal", &source, &mut include);
        // outputs can always be rendered, and errors always mean no image
        let _ = (a.listing_text(), a.symbols_text(), a.image());
        assert!(!a.has_errors() || a.words.is_empty(), "round {}", round);
        for d in &a.diagnostics {
            assert!(d.line >= 1, "round {}: {}", round, d);
        }
    }
}

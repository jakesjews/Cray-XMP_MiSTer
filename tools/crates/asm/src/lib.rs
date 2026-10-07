//! A two-pass assembler for a subset of CAL, the Cray Assembly Language,
//! producing raw memory images for the Cray-1.
//!
//! Instruction syntax comes from the table in `cray-xmp-isa` (Appendix D of the
//! CRAY-1 Hardware Reference Manual); this crate adds source handling,
//! expressions, symbols, directives, macros and the output files.
//!
//! # Source lines
//!
//! ```text
//! LABEL    RESULT    OPERAND        comment
//! ```
//!
//! * A `*` or `;` in column 1 makes the line a comment.
//! * The location field (label) starts in column 1.  The result and operand
//!   fields are the next two runs of non-blanks; anything after the operand
//!   is a comment.  Blanks and tabs separate fields, columns are free.
//! * As in CAL, an operand field must start before column 35.  After a
//!   result with no operand (`EX`, `PASS`, `ALIGN`...) a comment has to
//!   start at column 35 or later, or with `;`.  A tab moves to the next
//!   multiple of 8 columns.
//! * A label alone on a line names whatever the next statement defines.
//! * Symbols are made of letters, digits, `_ $ @ %`, are case sensitive and
//!   cannot be register names (`A0`-`A7`, `S0`-`S7`, `V0`-`V7`, `B0`-`B77`,
//!   `T0`-`T77`) or `SB VM VL RT CI CA CE CL XA`.  Mnemonics, registers and
//!   directives are upper case.
//!
//! # Expressions
//!
//! Numbers are octal unless prefixed: `D'10`, `O'12`, `X'A`.  Character
//! constants are `'AB'` (also `A'AB'`), up to eight characters, right
//! justified zero filled, or with a suffix `'AB'L` left justified zero
//! filled, `'AB'H` left justified blank filled, `'AB'R`; `''` is a quote.
//! Operators are `+ - * /` with the usual precedence, a leading sign, and
//! parentheses.  `*` where an operand is expected is the location counter.
//! A `#` in front of a whole expression complements it (in `Ai #exp` and
//! `Si #exp` it selects the complemented instruction instead).  Arithmetic
//! is 64-bit two's complement.
//!
//! # Address attributes
//!
//! Every value is a plain value, a word address or a parcel address
//! (word * 4 + parcel).  A label on an instruction or `VWD` is a parcel
//! address; a label on `CON`, `DATA`, `BSS`, `BSSZ`, `ALIGN` or `ORG` is a
//! word address; `*` is a parcel address; a symbol defined with `=` takes the
//! attribute of its expression.
//!
//! * `W.x` is `x` as a word address (a parcel address is divided by 4,
//!   dropping the parcel), `P.x` is `x` as a parcel address (a word address
//!   is multiplied by 4).  Each applies to the one element that follows.
//! * Adding or subtracting a plain value keeps the attribute.  The
//!   difference of two addresses is a plain value.  When a word address
//!   meets a parcel address the word address is converted to parcels first.
//!   Products and quotients are plain values.
//! * A branch (`J`, `R`, `JAZ`...) wants a parcel address: a word address is
//!   multiplied by 4, anything else is used as it is.
//! * A memory reference (`exp,Ah`) wants a word address: a parcel address is
//!   divided by 4, with a warning if it was not on a word boundary.
//! * Everywhere else (`Ai exp`, `Si exp`, `CON`, counts) the value is used
//!   as it is, whatever its attribute.
//!
//! # Directives
//!
//! ```text
//!          IDENT   name          program name
//!          END                   stop reading
//!          ENTRY   sym,...       record entry points
//!          EXT     sym,...       accepted; the symbols are 0 (no linking)
//! sym      =       exp           equate
//!          ORG     exp           continue at word address exp
//! sym      BSS     n             reserve n words
//! sym      BSSZ    n             n words of zero
//! sym      CON     exp,...       one 64-bit word per expression
//! sym      DATA    'text',exp    characters packed 8 per word, left
//!                                justified, zero filled ('..'H blank filled,
//!                                '..'R right justified); or CON-style words
//!          ALIGN                 go on at the next instruction buffer
//!                                boundary: a multiple of 20 octal words (40
//!                                under MACHINE XMP); zero parcels fill the
//!                                rest of the current word
//!          MACHINE CRAY1 | XMP   the machine whose instructions follow; XMP
//!                                adds the X-MP forms (see `cray-xmp isa`)
//! sym      VWD     D'16/exp,...  raw parcels in the code stream; widths 16,
//!                                32, 48 or 64 bits
//!          INCLUDE "file"
//!          LIST / NOLIST         listing on and off
//!          MACRO   name p1,p2    ... ENDM   (LOCAL sym,... inside)
//! ```
//!
//! `ABS`, `COMMENT`, `EJECT`, `SPACE`, `TITLE` and `SUBTITLE` are accepted and
//! ignored.  `CON`, `DATA`, `BSS`, `BSSZ` and `ORG` start on a word boundary.
//! As in CAL, the unused parcels of a word of code before them are filled
//! with the pass instruction `S1 S1&S1` (044111), so a program may run
//! through the boundary; behind `VWD` parcels the fill is zero.  `ORG`, `BSS`, `BSSZ` and `VWD` widths
//! must not depend on symbols defined later.
//!
//! # Macros
//!
//! ```text
//!          MACRO   SWAP R1,R2,TMP
//!          TMP     R1
//!          R1      R2
//!          R2      TMP
//!          ENDM
//!          SWAP    S1,S2,S3
//! ```
//!
//! The body is copied with every whole symbol equal to a parameter replaced
//! by the argument text.  Arguments are separated by commas; wrap one in
//! parentheses to pass commas.  Missing arguments are empty.  `LOCAL a,b`
//! in the body makes `a` and `b` unique in each expansion.  A label on the
//! call names the first thing the macro generates.  The CAL layout (a line
//! with only `MACRO`, then a prototype line `LOC NAME P1,P2`) is accepted
//! too; there the location field of the call replaces `LOC`.  Macros may
//! call macros.
//!
//! # Instructions
//!
//! Every spelling in `cray_xmp_isa::FORMS`.  Where one spelling has several
//! encodings the choice is the documented one:
//!
//! * `Ai exp`: 022 for 0 to 77 octal when the expression has no forward
//!   reference, else 020; 021 for a negative value.  `Ai -1` is 031i00.
//! * `Si exp`: 040, or 041 for a negative value.  The exact operands `0`,
//!   `1` and `-1` are the one-parcel 043i00, 042i77 and 042i00.
//! * `Ai #exp`, `Si #exp`: 021/041 holding exp, or 020/040 holding the
//!   complement when exp is negative.
//! * `Si <n`, `Si >n`, `Si #<n`, `Si #>n` and the shifts `S0 Si<n`,
//!   `S0 Si>n`, `Si Si<n`, `Si Si>n` take 0 to 64 (100 octal); the counts one
//!   opcode cannot hold assemble as the opposite one.
//! * `S0 S0<n` is 052 (not 054).
//!
//! # Output
//!
//! [`assemble`] returns an [`Assembly`]: the image as 64-bit words from word
//! 0 to the highest word used (empty if there were errors), the symbols, the
//! listing and the diagnostics.  `Assembly::image()` gives the `.img` bytes
//! (big-endian words), `listing_text()` and `symbols_text()` the other files.

mod expr;
mod pass;
mod source;

use std::fmt;
use std::path::{Path, PathBuf};

/// The address attribute of a symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    /// A plain number.
    Value,
    /// A word address.
    Word,
    /// A parcel address: word * 4 + parcel.
    Parcel,
    /// Declared with `EXT`; its value is 0.
    External,
}

impl Attr {
    /// The letter used in the symbol file: V, W, P or X.
    pub fn letter(self) -> char {
        match self {
            Attr::Value => 'V',
            Attr::Word => 'W',
            Attr::Parcel => 'P',
            Attr::External => 'X',
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub value: i64,
    pub attr: Attr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let kind = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{}:{}: {}: {}", self.file, self.line, kind, self.message)
    }
}

/// One line of the listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListLine {
    pub file: String,
    pub line: u32,
    /// Macro nesting depth: 0 for a line read from a file.
    pub depth: u8,
    /// The source text (after substitution for macro lines).
    pub text: String,
    /// The parcel address (word * 4 + parcel) the line was assembled at.
    pub addr: Option<u64>,
    /// True if the line works in whole words (data, `ORG`, `ALIGN`): its
    /// address is listed without a parcel letter.
    pub word_address: bool,
    /// Instruction parcels the line produced.
    pub parcels: Vec<u16>,
    /// Data words the line produced (`BSSZ` shows only the first).
    pub words: Vec<u64>,
    /// The value of an `=` line.
    pub value: Option<i64>,
    /// False between `NOLIST` and `LIST`.
    pub listed: bool,
    /// Errors and warnings for the line.
    pub messages: Vec<String>,
}

/// The result of an assembly.
#[derive(Clone, Debug, Default)]
pub struct Assembly {
    /// The operand of `IDENT`.
    pub ident: Option<String>,
    /// The machine the program is for: that of its last `MACHINE` line.
    pub machine: cray_xmp_isa::Cpu,
    /// The memory image from word 0 to the highest word used.  Empty if
    /// there were errors.
    pub words: Vec<u64>,
    /// All symbols, sorted by name.
    pub symbols: Vec<Symbol>,
    /// The operands of `ENTRY`.
    pub entries: Vec<String>,
    pub listing: Vec<ListLine>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Assembly {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning)
    }

    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name == name)
    }

    /// The image file: each word as 8 bytes, most significant first.
    pub fn image(&self) -> Vec<u8> {
        self.words.iter().flat_map(|w| w.to_be_bytes()).collect()
    }

    /// The parcel at a parcel address of the image (0 beyond its end).
    pub fn parcel(&self, addr: u64) -> u16 {
        let word = self.words.get((addr / 4) as usize).copied().unwrap_or(0);
        (word >> (48 - 16 * (addr % 4))) as u16
    }

    /// The symbol file: one line per symbol with its name, its value in
    /// octal and its attribute letter (V value, W word address, P parcel
    /// address, X external).
    pub fn symbols_text(&self) -> String {
        let mut out = String::new();
        for s in &self.symbols {
            out.push_str(&format!(
                "{:<16} {:>8o} {}\n",
                s.name,
                s.value,
                s.attr.letter()
            ));
        }
        out
    }

    /// The listing: address (octal word and parcel letter a-d), octal
    /// parcels or words, line number, source line.
    pub fn listing_text(&self) -> String {
        let mut out = String::new();
        let address = |addr: u64, data: bool| {
            let parcel = if data {
                ' '
            } else {
                (b'a' + (addr % 4) as u8) as char
            };
            format!("{:07o}{}", addr / 4, parcel)
        };
        let several_files = self.listing.iter().any(|l| l.file != self.listing[0].file);
        let mut file = "";
        for l in &self.listing {
            if several_files && l.file != file {
                file = &l.file;
                out.push_str(&format!("* file {}\n", file));
            }
            if !l.listed && l.messages.is_empty() {
                continue;
            }
            // rows of code: up to three parcels or one word each
            let mut rows: Vec<(String, String)> = Vec::new();
            let data = l.word_address;
            if let Some(v) = l.value {
                rows.push((String::new(), format!("= {:o}", v)));
            }
            let mut addr = l.addr.unwrap_or(0);
            for chunk in l.parcels.chunks(3) {
                let text: Vec<String> = chunk.iter().map(|p| format!("{:06o}", p)).collect();
                rows.push((address(addr, false), text.join(" ")));
                addr += chunk.len() as u64;
            }
            for w in &l.words {
                rows.push((address(addr, true), format!("{:022o}", w)));
                addr += 4;
            }
            if rows.is_empty() {
                rows.push((
                    l.addr.map(|a| address(a, data)).unwrap_or_default(),
                    String::new(),
                ));
            }
            let mark = if l.depth > 0 { '+' } else { ' ' };
            for (n, (a, code)) in rows.iter().enumerate() {
                if n == 0 {
                    out.push_str(
                        format!("{:<8}  {:<22} {:>5}{} {}", a, code, l.line, mark, l.text)
                            .trim_end(),
                    );
                } else {
                    out.push_str(format!("{:<8}  {}", a, code).trim_end());
                }
                out.push('\n');
            }
            for m in &l.messages {
                out.push_str(&format!("***** {}\n", m));
            }
        }
        let errors = self
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warnings = self.diagnostics.len() - errors;
        out.push_str(&format!("\n{} errors, {} warnings\n", errors, warnings));
        if !self.symbols.is_empty() {
            out.push_str("\nSymbols (V value, W word address, P parcel address, X external)\n");
            out.push_str(&self.symbols_text());
        }
        out
    }
}

/// Resolves `INCLUDE "file"`: called with the name as written and the name
/// of the file containing the directive, it returns the name to report in
/// diagnostics and the text of the file, or why it cannot.
pub type IncludeResolver<'a> = dyn FnMut(&str, &str) -> Result<(String, String), String> + 'a;

/// Assemble `source`.  `name` is the file name used in diagnostics.
pub fn assemble(name: &str, source: &str, include: &mut IncludeResolver) -> Assembly {
    let mut pre = source::Preprocessor::new(include);
    pre.file(name, source, 0);
    let (files, lines, mut msgs) = (pre.files, pre.lines, pre.msgs);
    let mut engine = pass::Engine::new(&lines);
    engine.run();
    msgs.append(&mut engine.msgs);
    msgs.sort_by_key(|m| m.line);

    let mut out = Assembly {
        ident: engine.ident.take(),
        machine: engine.cpu,
        entries: std::mem::take(&mut engine.entries),
        ..Assembly::default()
    };
    for (line, emit) in lines.iter().zip(std::mem::take(&mut engine.emits)) {
        out.listing.push(ListLine {
            file: files[line.file].clone(),
            line: line.line,
            depth: line.depth,
            text: line.text.clone(),
            addr: emit.addr,
            word_address: emit.word_address,
            parcels: emit.parcels,
            words: emit.words,
            value: emit.value,
            listed: emit.listed,
            messages: Vec::new(),
        });
    }
    for m in msgs {
        let line = &lines[m.line];
        let message = match &line.origin {
            Some(mac) => format!("{} (in macro {}: {})", m.text, mac, line.text.trim()),
            None => m.text,
        };
        let d = Diagnostic {
            severity: m.severity,
            file: files[line.file].clone(),
            line: line.line,
            message,
        };
        let kind = if d.severity == Severity::Error {
            "error"
        } else {
            "warning"
        };
        out.listing[m.line]
            .messages
            .push(format!("{}: {}", kind, d.message));
        out.diagnostics.push(d);
    }
    for (name, sym) in &engine.syms {
        out.symbols.push(Symbol {
            name: name.clone(),
            value: sym.value.unwrap_or(0),
            attr: sym.attr,
        });
    }
    out.symbols.sort_by(|a, b| a.name.cmp(&b.name));
    if !out.has_errors() {
        out.words = std::mem::take(&mut engine.mem);
    }
    out
}

/// Assemble a source with no `INCLUDE` files available.
pub fn assemble_source(source: &str) -> Assembly {
    assemble("<source>", source, &mut |name, _| {
        Err(format!("no include files here ({})", name))
    })
}

/// Assemble a file.  `INCLUDE` looks in the directory of the including file
/// and then in `include_dirs`.
pub fn assemble_file(path: &Path, include_dirs: &[PathBuf]) -> Result<Assembly, String> {
    let source = read_source(path)?;
    let mut include = |name: &str, from: &str| -> Result<(String, String), String> {
        let beside = Path::new(from)
            .parent()
            .map(|d| d.join(name))
            .unwrap_or_else(|| PathBuf::from(name));
        let mut tried = vec![beside];
        tried.extend(include_dirs.iter().map(|d| d.join(name)));
        for candidate in &tried {
            if candidate.is_file() {
                return Ok((candidate.display().to_string(), read_source(candidate)?));
            }
        }
        Err("file not found".to_string())
    };
    Ok(assemble(&path.display().to_string(), &source, &mut include))
}

fn read_source(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    // old listings are not always UTF-8; comments are the only place it matters
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

//! The two assembler passes over the preprocessed lines.

use crate::expr::{self, Val};
use crate::source::{split_list, Line, Msg};
use crate::{Attr, Severity};
use cray_xmp_isa::{is_reserved_name, is_symbol_char, Cpu, ExpUse, ExpValue};

/// `S1 S1&S1`, the pass instruction CAL fills unused parcels with (CAL reference manual
/// SR-0000, "unused parcels are filled with pass instructions").
const PAD: u16 = 0o044111;
use std::collections::HashMap;

/// Largest image the assembler will build: the 22-bit address space.
const MAX_WORDS: u64 = 1 << 22;

pub(crate) struct Sym {
    /// `None` while an equate waits for symbols defined later.
    pub value: Option<i64>,
    pub attr: Attr,
    pub line: usize,
    /// Defined from an expression with a forward reference.
    pub tainted: bool,
}

/// What the first pass decided about a line.
#[derive(Clone, Copy, Default)]
struct Info {
    /// Instruction length in parcels.
    size: u8,
    /// ORG target, BSS length.
    aux: u64,
}

/// What a line put in the image, for the listing.
#[derive(Clone, Default)]
pub(crate) struct Emit {
    pub addr: Option<u64>,
    /// The line defines a word address (data, ORG, ALIGN) rather than code.
    pub word_address: bool,
    pub parcels: Vec<u16>,
    pub words: Vec<u64>,
    pub value: Option<i64>,
    pub listed: bool,
}

pub(crate) struct Engine<'a> {
    lines: &'a [Line],
    pub syms: HashMap<String, Sym>,
    pub msgs: Vec<Msg>,
    pass: u8,
    /// Location counter as a parcel address.
    loc: u64,
    /// What `*` stands for: the location where the current statement starts
    /// (after any padding to a word boundary).
    star: u64,
    /// Labels on lines of their own, waiting for the next statement.
    waiting: Vec<(String, usize)>,
    info: Vec<Info>,
    equates: Vec<(usize, u64)>,
    pub mem: Vec<u64>,
    pub emits: Vec<Emit>,
    pub ident: Option<String>,
    pub entries: Vec<String>,
    listing_on: bool,
    overflow: bool,
    /// The machine whose instructions are accepted (`MACHINE`).
    pub cpu: Cpu,
    /// The parcels just assembled are instructions, not data.
    code_last: bool,
}

fn valid_symbol(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_symbol_char) && !name.as_bytes()[0].is_ascii_digit()
}

impl<'a> Engine<'a> {
    pub fn new(lines: &'a [Line]) -> Self {
        Engine {
            lines,
            syms: HashMap::new(),
            msgs: Vec::new(),
            pass: 1,
            loc: 0,
            star: 0,
            waiting: Vec::new(),
            info: vec![Info::default(); lines.len()],
            equates: Vec::new(),
            mem: Vec::new(),
            emits: vec![Emit::default(); lines.len()],
            ident: None,
            entries: Vec::new(),
            listing_on: true,
            overflow: false,
            cpu: Cpu::Cray1,
            code_last: false,
        }
    }

    pub fn run(&mut self) {
        for pass in [1, 2] {
            self.pass = pass;
            self.loc = 0;
            self.listing_on = true;
            self.cpu = Cpu::Cray1;
            self.code_last = false;
            self.waiting.clear();
            for idx in 0..self.lines.len() {
                self.emits[idx].listed = self.listing_on;
                self.statement(idx);
            }
            // labels with nothing after them name the end of the program
            if let Some(last) = self.lines.len().checked_sub(1) {
                self.place_labels(last, "", Attr::Parcel);
            }
            if pass == 1 {
                self.resolve_equates();
            }
        }
    }

    fn error(&mut self, idx: usize, text: impl Into<String>) {
        self.msgs.push(Msg {
            line: idx,
            severity: Severity::Error,
            text: text.into(),
        });
    }

    fn warning(&mut self, idx: usize, text: impl Into<String>) {
        self.msgs.push(Msg {
            line: idx,
            severity: Severity::Warning,
            text: text.into(),
        });
    }

    /// Report an error found while evaluating operands.  Operands are
    /// evaluated in both passes; the message is given once, in the second.
    fn operand_error(&mut self, idx: usize, text: impl Into<String>) {
        if self.pass == 2 {
            self.error(idx, text);
        }
    }

    fn lookup(&self, name: &str, idx: usize) -> Result<Val, String> {
        match self.syms.get(name) {
            Some(Sym { value: Some(value), attr, line, tainted }) => Ok(Val {
                value: *value,
                attr: if *attr == Attr::External { Attr::Value } else { *attr },
                forward: *tainted || *line > idx,
                unknown: false,
            }),
            Some(_) if self.pass == 1 => Ok(Val::unknown()),
            Some(_) => Err(format!("symbol {} has no value (its definition depends on itself or on an undefined symbol)", name)),
            None if self.pass == 1 => Ok(Val::unknown()),
            None => Err(format!("undefined symbol {}", name)),
        }
    }

    fn eval(&self, text: &str, idx: usize) -> Result<Val, String> {
        expr::eval(text, self.star, &mut |name| self.lookup(name, idx))
    }

    /// Evaluate an operand that must be known in the first pass because the
    /// location counter depends on it.
    fn eval_now(&mut self, text: &str, idx: usize, what: &str) -> Option<Val> {
        match self.eval(text, idx) {
            Ok(v) if v.unknown => {
                self.error(
                    idx,
                    format!("{} `{}` uses a symbol that is not defined yet", what, text),
                );
                None
            }
            Ok(v) => Some(v),
            Err(e) => {
                if self.pass == 1 {
                    self.error(idx, e);
                }
                None
            }
        }
    }

    fn define(&mut self, idx: usize, name: &str, value: Option<i64>, attr: Attr, tainted: bool) {
        if self.pass == 2 {
            // labels must land where the first pass put them
            if let (Some(sym), Some(value)) = (self.syms.get(name), value) {
                if sym.line == idx && sym.value.is_some_and(|old| old != value) && !sym.tainted {
                    let old = sym.value.unwrap();
                    self.error(
                        idx,
                        format!(
                            "phase error: {} was {:o} in the first pass and is {:o} in the second",
                            name, old, value
                        ),
                    );
                }
            }
            return;
        }
        if !valid_symbol(name) {
            self.error(idx, format!("`{}` is not a valid symbol name", name));
        } else if is_reserved_name(name) {
            self.error(
                idx,
                format!(
                    "`{}` cannot be a symbol: the instruction syntax uses it as a register name",
                    name
                ),
            );
        } else if let Some(old) = self.syms.get(name) {
            let first = &self.lines[old.line];
            let text = format!("symbol {} is already defined on line {}", name, first.line);
            self.error(idx, text);
        } else {
            self.syms.insert(
                name.to_string(),
                Sym {
                    value,
                    attr,
                    line: idx,
                    tainted,
                },
            );
        }
    }

    /// Give the waiting labels and `label` the current location: a parcel
    /// address for code, a word address for data.
    fn place_labels(&mut self, idx: usize, label: &str, attr: Attr) {
        let value = if attr == Attr::Word {
            self.loc / 4
        } else {
            self.loc
        } as i64;
        for (name, at) in std::mem::take(&mut self.waiting) {
            self.define(at, &name, Some(value), attr, false);
        }
        if !label.is_empty() {
            self.define(idx, label, Some(value), attr, false);
        }
    }

    /// Give the waiting labels and the label of this line the current
    /// location, and show that location in the listing.
    fn begin(&mut self, idx: usize, label: &str, attr: Attr) {
        self.place_labels(idx, label, attr);
        self.star = self.loc;
        self.emits[idx].addr = Some(self.loc);
        self.emits[idx].word_address = attr == Attr::Word;
    }

    fn no_label(&mut self, idx: usize, label: &str, what: &str) {
        if !label.is_empty() && self.pass == 1 {
            self.error(idx, format!("{} cannot have a label", what));
        }
    }

    fn put_parcel(&mut self, idx: usize, parcel: u16) {
        if self.pass == 2 {
            let word = (self.loc / 4) as usize;
            if (word as u64) < MAX_WORDS {
                if self.mem.len() <= word {
                    self.mem.resize(word + 1, 0);
                }
                let shift = 48 - 16 * (self.loc % 4);
                self.mem[word] = (self.mem[word] & !(0xffff << shift)) | ((parcel as u64) << shift);
            }
        }
        self.advance(idx, 1);
    }

    fn put_word(&mut self, idx: usize, value: u64) {
        debug_assert_eq!(self.loc % 4, 0);
        if self.pass == 2 {
            let word = (self.loc / 4) as usize;
            if (word as u64) < MAX_WORDS {
                if self.mem.len() <= word {
                    self.mem.resize(word + 1, 0);
                }
                self.mem[word] = value;
            }
            self.emits[idx].words.push(value);
        }
        self.advance(idx, 4);
    }

    fn advance(&mut self, idx: usize, parcels: u64) {
        let end = self.loc.saturating_add(parcels);
        if end > MAX_WORDS * 4 {
            if !self.overflow {
                self.overflow = true;
                self.error(
                    idx,
                    "the program runs past the end of the 22-bit address space",
                );
            }
            self.loc = MAX_WORDS * 4;
        } else {
            self.loc = end;
        }
    }

    /// Make the image reach up to (not including) the current location.
    fn reserve(&mut self) {
        let words = self.loc.div_ceil(4).min(MAX_WORDS) as usize;
        if self.pass == 2 && self.mem.len() < words {
            self.mem.resize(words, 0);
        }
    }

    /// Pad to a word boundary: with pass instructions behind code, so that a
    /// program can run through, and with zero parcels behind data.
    fn align(&mut self, idx: usize) {
        let fill = if self.code_last { PAD } else { 0 };
        while !self.loc.is_multiple_of(4) {
            self.put_parcel(idx, fill);
        }
        self.code_last = false;
    }

    fn statement(&mut self, idx: usize) {
        let lines = self.lines;
        let line = &lines[idx];
        let (label, result, operand) = (
            line.label.as_str(),
            line.result.as_str(),
            line.operand.as_str(),
        );
        self.star = self.loc;
        if result.is_empty() {
            if !label.is_empty() {
                self.waiting.push((label.to_string(), idx));
            }
            return;
        }
        match result {
            "IDENT" => {
                self.no_label(idx, label, "IDENT");
                self.ident = Some(operand.to_string());
            }
            "END" | "ABS" | "EJECT" | "SPACE" | "TITLE" | "SUBTITLE" | "COMMENT" => {
                self.no_label(idx, label, result)
            }
            "MACHINE" => {
                self.no_label(idx, label, result);
                match Cpu::from_name(operand) {
                    Some(cpu) => self.cpu = cpu,
                    None if self.pass == 1 => self.error(
                        idx,
                        format!("MACHINE `{}` is not known: CRAY1 or XMP", operand),
                    ),
                    None => {}
                }
            }
            "LIST" => {
                self.no_label(idx, label, result);
                self.listing_on = true;
                self.emits[idx].listed = true;
            }
            "NOLIST" => {
                self.no_label(idx, label, result);
                self.listing_on = false;
            }
            "ENTRY" | "EXT" => {
                self.no_label(idx, label, result);
                for name in split_list(operand) {
                    if result == "EXT" {
                        if self.pass == 1 && !self.syms.contains_key(name) {
                            self.define(idx, name, Some(0), Attr::External, false);
                        }
                    } else if self.pass == 1 {
                        self.entries.push(name.to_string());
                    } else if !self.syms.contains_key(name) {
                        self.error(idx, format!("ENTRY symbol {} is not defined", name));
                    }
                }
                if operand.is_empty() && self.pass == 1 {
                    self.error(idx, format!("{} needs a symbol name", result));
                }
            }
            "=" => self.equate(idx, label, operand),
            "ORG" => self.org(idx, label, operand),
            "BSS" | "BSSZ" => {
                self.align(idx);
                self.begin(idx, label, Attr::Word);
                if self.pass == 1 {
                    let n = self
                        .eval_now(operand, idx, &format!("{} length", result))
                        .map_or(0, |v| v.value);
                    if (0..=MAX_WORDS as i64).contains(&n) {
                        self.info[idx].aux = n as u64;
                    } else {
                        self.error(
                            idx,
                            format!(
                                "{} length {} is out of range",
                                result,
                                cray_xmp_isa::signed_octal(n)
                            ),
                        );
                    }
                }
                let words = self.info[idx].aux;
                if result == "BSSZ" {
                    for _ in 0..words {
                        self.put_word(idx, 0);
                    }
                    self.emits[idx].words.truncate(1);
                } else {
                    // reserved, not written: the image is zero filled anyway
                    self.advance(idx, words * 4);
                    self.reserve();
                }
            }
            "CON" => {
                self.align(idx);
                self.begin(idx, label, Attr::Word);
                if operand.is_empty() {
                    self.operand_error(idx, "CON needs a value");
                }
                for item in split_list(operand) {
                    let value = match self.eval(item, idx) {
                        Ok(v) => v.value,
                        Err(e) => {
                            self.operand_error(idx, e);
                            0
                        }
                    };
                    self.put_word(idx, value as u64);
                }
            }
            "DATA" => {
                self.align(idx);
                self.begin(idx, label, Attr::Word);
                if operand.is_empty() {
                    self.operand_error(idx, "DATA needs a value");
                }
                for item in split_list(operand) {
                    for word in self.data_item(idx, item) {
                        self.put_word(idx, word);
                    }
                }
            }
            "ALIGN" => {
                // zero parcels to the word boundary, then on to the next
                // instruction buffer boundary: 20 octal words, 40 on an X-MP
                self.code_last = false;
                self.align(idx);
                let block = if self.cpu == Cpu::Xmp { 128 } else { 64 };
                self.advance(idx, (block - self.loc % block) % block);
                self.reserve();
                self.begin(idx, label, Attr::Word);
            }
            "VWD" => self.vwd(idx, label, operand),
            _ => self.instruction(idx, label, result, operand),
        }
    }

    fn equate(&mut self, idx: usize, label: &str, operand: &str) {
        if label.is_empty() {
            if self.pass == 1 {
                self.error(idx, "`=` needs a symbol in the location field");
            }
            return;
        }
        match self.eval(operand, idx) {
            Ok(v) if self.pass == 1 => {
                self.define(
                    idx,
                    label,
                    (!v.unknown).then_some(v.value),
                    v.attr,
                    v.forward,
                );
                if v.unknown {
                    self.equates.push((idx, self.star));
                }
            }
            Ok(v) => self.emits[idx].value = Some(v.value),
            Err(e) if self.pass == 1 => {
                // reported in the second pass; keep later uses quiet
                let _ = e;
                self.define(idx, label, Some(0), Attr::Value, false);
            }
            Err(e) => self.error(idx, e),
        }
    }

    /// Give values to equates that referred to symbols defined after them.
    fn resolve_equates(&mut self) {
        loop {
            let mut progress = false;
            for n in 0..self.equates.len() {
                let (idx, loc) = self.equates[n];
                let lines = self.lines;
                let line = &lines[idx];
                if self
                    .syms
                    .get(&line.label)
                    .is_none_or(|s| s.value.is_some() || s.line != idx)
                {
                    continue;
                }
                self.star = loc;
                if let Ok(v) = self.eval(&line.operand, idx) {
                    if !v.unknown {
                        let sym = self.syms.get_mut(&line.label).unwrap();
                        sym.value = Some(v.value);
                        sym.attr = v.attr;
                        sym.tainted = true;
                        progress = true;
                    }
                }
            }
            if !progress {
                break;
            }
        }
    }

    fn org(&mut self, idx: usize, label: &str, operand: &str) {
        self.align(idx);
        if self.pass == 1 {
            let mut target = self.loc;
            if let Some(v) = self.eval_now(operand, idx, "ORG address") {
                let word = match v.attr {
                    Attr::Parcel if v.value & 3 != 0 => {
                        self.error(idx, format!("ORG address `{}` is a parcel address that is not on a word boundary", operand));
                        v.value >> 2
                    }
                    Attr::Parcel => v.value >> 2,
                    _ => v.value,
                };
                if (0..MAX_WORDS as i64).contains(&word) {
                    target = word as u64 * 4;
                } else {
                    self.error(
                        idx,
                        format!(
                            "ORG address {} is out of range",
                            cray_xmp_isa::signed_octal(word)
                        ),
                    );
                }
            }
            self.info[idx].aux = target;
        }
        self.loc = self.info[idx].aux;
        self.begin(idx, label, Attr::Word);
    }

    /// The words of one DATA item: a character string packed eight to a
    /// word, or an expression.
    fn data_item(&mut self, idx: usize, item: &str) -> Vec<u64> {
        let quote = match item.as_bytes() {
            [b'\'', ..] => Some(0),
            [b'A', b'\'', ..] => Some(1),
            _ => None,
        };
        if let Some(q) = quote {
            match expr::string_body(item, q) {
                Ok((bytes, end)) => {
                    let suffix = &item[end..];
                    let justify = match suffix {
                        "" | "L" => Some(b'L'),
                        "H" => Some(b'H'),
                        "R" => Some(b'R'),
                        _ => None,
                    };
                    if let Some(j) = justify {
                        return expr::pack(&bytes, j);
                    }
                    // otherwise it is an expression that starts with a constant
                }
                Err(e) => {
                    self.operand_error(idx, e);
                    return vec![0];
                }
            }
        }
        match self.eval(item, idx) {
            Ok(v) => vec![v.value as u64],
            Err(e) => {
                self.operand_error(idx, e);
                vec![0]
            }
        }
    }

    /// `VWD n/exp,...`: raw fields of whole parcels in the code stream.
    fn vwd(&mut self, idx: usize, label: &str, operand: &str) {
        self.begin(idx, label, Attr::Parcel);
        self.code_last = false;
        if operand.is_empty() {
            self.operand_error(idx, "VWD needs width/value items");
        }
        for item in split_list(operand) {
            let Some((width_text, value_text)) = item.split_once('/') else {
                self.operand_error(idx, format!("VWD item `{}` is not width/value", item));
                continue;
            };
            // Widths move the location counter, so they must be known in the
            // first pass; the second pass computes the same value again.
            let width = if self.pass == 1 {
                self.eval_now(width_text, idx, "VWD width")
                    .map_or(16, |v| v.value)
            } else {
                self.eval(width_text, idx).map_or(16, |v| v.value)
            };
            let parcels = if width <= 0 || width > 64 || width % 16 != 0 {
                if self.pass == 1 {
                    self.error(idx, format!("VWD width {} is not 16, 32, 48 or 64 bits (numbers are octal: write D'16)", width));
                }
                1
            } else {
                (width / 16) as u64
            };
            let value = match self.eval(value_text, idx) {
                Ok(v) => v.value,
                Err(e) => {
                    self.operand_error(idx, e);
                    0
                }
            };
            let bits = parcels * 16;
            if bits < 64 && self.pass == 2 {
                let limit = 1i64 << bits;
                if value >= limit || value < -(limit / 2) {
                    self.error(
                        idx,
                        format!(
                            "VWD value {} does not fit in {} bits",
                            cray_xmp_isa::signed_octal(value),
                            bits
                        ),
                    );
                }
            }
            for p in (0..parcels).rev() {
                let parcel = (value as u64 >> (16 * p)) as u16;
                if self.pass == 2 {
                    self.emits[idx].parcels.push(parcel);
                }
                self.put_parcel(idx, parcel);
            }
        }
    }

    fn instruction(&mut self, idx: usize, label: &str, result: &str, operand: &str) {
        self.begin(idx, label, Attr::Parcel);
        self.code_last = true;
        let cpu = self.cpu;
        let pass = self.pass;
        let mut warnings = Vec::new();
        let assembled = {
            let mut eval = |text: &str, usage: ExpUse| -> Result<ExpValue, String> {
                let v = self.eval(text, idx)?;
                let mut value = v.value;
                if !v.unknown {
                    match (usage, v.attr) {
                        (ExpUse::Parcel, Attr::Word) => value = value.wrapping_mul(4),
                        (ExpUse::Word, Attr::Parcel) => {
                            if value & 3 != 0 && pass == 2 {
                                warnings.push(format!(
                                    "`{}` is a parcel address that is not on a word boundary; its word address is used",
                                    text
                                ));
                            }
                            value >>= 2;
                        }
                        _ => {}
                    }
                }
                Ok(ExpValue {
                    value,
                    forward: v.forward,
                })
            };
            cray_xmp_isa::assemble_cpu(cpu, result, operand, &mut eval)
        };
        for w in warnings {
            self.warning(idx, w);
        }
        if pass == 1 {
            // errors are reported in the second pass, where symbols are known
            self.info[idx].size = assembled.map_or(1, |a| a.encoding.len() as u8);
            self.advance(idx, self.info[idx].size as u64);
            return;
        }
        let size = self.info[idx].size as usize;
        let mut parcels = vec![0u16; size];
        match assembled {
            Ok(a) if a.encoding.len() == size => parcels = a.encoding.parcels(),
            Ok(a) => self.error(
                idx,
                format!("phase error: the instruction took {} parcels in the first pass and {} in the second", size, a.encoding.len()),
            ),
            Err(e) if operand.is_empty() && self.lines[idx].late => self.error(
                idx,
                format!("{} (an operand must start before column 35; the text after `{}` is taken as a comment)", e, result),
            ),
            Err(e) => self.error(idx, e),
        }
        for p in parcels {
            self.emits[idx].parcels.push(p);
            self.put_parcel(idx, p);
        }
    }
}

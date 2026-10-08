//! CAL syntax of single instructions, driven by the templates in the table:
//! `assemble` matches a result and operand field against them, `disassemble`
//! fills them in.

use crate::code::*;
use crate::table::*;
use std::sync::OnceLock;

/// How an instruction uses the value of its expression operand.  The
/// assembler uses this to convert between word and parcel addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpUse {
    /// A plain number: constants, counts, register numbers.
    Value,
    /// A parcel address (branch targets).
    Parcel,
    /// A word address or displacement (memory references).
    Word,
}

/// The value of an expression operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpValue {
    pub value: i64,
    /// True if the expression refers to a symbol defined later in the
    /// source.  `Ai exp` then always assembles as the two-parcel 020 or 021
    /// so that instruction lengths are known in the first pass.
    pub forward: bool,
}

/// The result of assembling one instruction.
#[derive(Clone, Copy, Debug)]
pub struct Assembled {
    /// The base row of the instruction that was encoded.
    pub form: &'static Form,
    /// The row whose spelling matched the source.
    pub syntax: &'static Form,
    pub fields: Fields,
    pub encoding: Encoding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    /// A literal character.
    Lit(u8),
    /// `Ai`, `Sj`, `Vk`, `Ah`...: register letter and field letter.  Also
    /// the `Bj` and `Tj` of the shared registers `SBj` and `STj`.
    Reg(u8, u8),
    /// `Bjk` or `Tjk`, and the `Mjk` of a semaphore `SMjk`.
    Blk(u8),
    /// `exp`.
    Exp,
}

fn tokenize(template: &str) -> Vec<Tok> {
    let b = template.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"exp") {
            out.push(Tok::Exp);
            i += 3;
        } else if matches!(b[i], b'B' | b'T' | b'M') && b[i + 1..].starts_with(b"jk") {
            out.push(Tok::Blk(b[i]));
            i += 3;
        } else if matches!(b[i], b'B' | b'T') && b[i + 1..].starts_with(b"j") {
            out.push(Tok::Reg(b[i], b'j'));
            i += 2;
        } else if matches!(b[i], b'A' | b'S' | b'V')
            && i + 1 < b.len()
            && matches!(b[i + 1], b'h' | b'i' | b'j' | b'k')
        {
            out.push(Tok::Reg(b[i], b[i + 1]));
            i += 2;
        } else {
            out.push(Tok::Lit(b[i]));
            i += 1;
        }
    }
    out
}

struct Templates {
    result: Vec<Tok>,
    operand: Vec<Tok>,
    has_exp: bool,
}

fn templates() -> &'static [Templates] {
    static T: OnceLock<Vec<Templates>> = OnceLock::new();
    T.get_or_init(|| {
        FORMS
            .iter()
            .map(|f| {
                let result = tokenize(f.result);
                let operand = tokenize(f.operand);
                let has_exp = result.contains(&Tok::Exp) || operand.contains(&Tok::Exp);
                Templates {
                    result,
                    operand,
                    has_exp,
                }
            })
            .collect()
    })
}

/// True for a character that can be part of a symbol name.
pub fn is_symbol_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'@' | b'%')
}

/// True if `name` is spelled like a register: `A0` to `A7`, `S0` to `S7`,
/// `V0` to `V7`, `B0` to `B77`, `T0` to `T77`.
pub fn is_register_name(name: &str) -> bool {
    let b = name.as_bytes();
    let octal = |c: &u8| (b'0'..=b'7').contains(c);
    match b {
        [b'A' | b'S' | b'V', d] => octal(d),
        [b'B' | b'T', d] => octal(d),
        [b'B' | b'T', d, e] => octal(d) && octal(e),
        _ => false,
    }
}

/// True if `name` cannot be a symbol because the CAL instruction syntax
/// already gives it a meaning: a register name, or one of the keywords
/// `SB VM VL RT CI CA CE CL XA`.
pub fn is_reserved_name(name: &str) -> bool {
    is_register_name(name)
        || matches!(
            name,
            "SB" | "VM" | "VL" | "RT" | "CI" | "CA" | "CE" | "CL" | "XA"
        )
}

/// For each byte of `text`, whether it lies inside a character constant
/// (`'...'`, with `''` for a quote inside it; the quotes themselves count).
/// The quote of a `D'`, `O'` or `X'` number prefix does not open a constant.
pub fn quoted_mask(text: &str) -> Vec<bool> {
    let b = text.as_bytes();
    let mut mask = vec![false; b.len()];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\'' {
            let radix = i >= 1
                && matches!(b[i - 1], b'D' | b'O' | b'X')
                && (i < 2 || !is_symbol_char(b[i - 2]));
            if radix {
                i += 1;
                continue;
            }
            let start = i;
            i += 1;
            loop {
                if i >= b.len() {
                    break;
                }
                if b[i] == b'\'' {
                    if i + 1 < b.len() && b[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            for m in &mut mask[start..i] {
                *m = true;
            }
        } else {
            i += 1;
        }
    }
    mask
}

/// A cheap syntactic test: could `text` be an expression operand?  It must
/// be made of numbers, symbols, character constants, `W.`/`P.` prefixes, the
/// operators `+ - * /` and parentheses, and must not name a register.
fn looks_like_expression(text: &str) -> bool {
    let b = text.as_bytes();
    if b.is_empty() {
        return false;
    }
    let quoted = quoted_mask(text);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if quoted[i] {
            let start = i;
            while i < b.len() && quoted[i] {
                i += 1;
            }
            // an unterminated constant is not an expression
            if i - start < 2 || b[i - 1] != b'\'' {
                return false;
            }
            // optional justification suffix
            if i < b.len() && matches!(b[i], b'L' | b'R' | b'H') {
                i += 1;
            }
            if i < b.len() && is_symbol_char(b[i]) {
                return false;
            }
        } else if c.is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i < b.len() && (is_symbol_char(b[i]) || b[i] == b'.' || b[i] == b'\'') {
                return false;
            }
        } else if is_symbol_char(c) {
            let start = i;
            while i < b.len() && is_symbol_char(b[i]) {
                i += 1;
            }
            let word = &text[start..i];
            if i < b.len() && b[i] == b'\'' {
                match word {
                    // radix prefix: digits follow
                    "D" | "O" | "X" => {
                        i += 1;
                        let digits = i;
                        while i < b.len() && b[i].is_ascii_alphanumeric() {
                            i += 1;
                        }
                        if i == digits {
                            return false;
                        }
                    }
                    // A'...' character constant: handled by the quoted branch
                    "A" => {}
                    _ => return false,
                }
            } else if i < b.len() && b[i] == b'.' {
                if word != "W" && word != "P" {
                    return false;
                }
                i += 1;
                if i >= b.len() {
                    return false;
                }
            } else if is_register_name(word) {
                return false;
            }
        } else if matches!(c, b'+' | b'-' | b'*' | b'/' | b'(' | b')') {
            i += 1;
        } else {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug)]
enum RegText<'a> {
    Num(u8),
    Sym(&'a str),
}

#[derive(Clone, Copy, Debug)]
enum Cap<'a> {
    Reg(u8, RegText<'a>),
    Blk(RegText<'a>),
    Exp(&'a str),
}

/// Parse a register designator at the start of `src`: the letter followed by
/// octal digits, or by `.` and a symbol or number.
fn parse_reg(src: &str, letter: u8, max_digits: usize) -> Option<(usize, RegText<'_>)> {
    let b = src.as_bytes();
    if b.first() != Some(&letter) {
        return None;
    }
    if b.get(1) == Some(&b'.') {
        let mut n = 2;
        while n < b.len() && is_symbol_char(b[n]) {
            n += 1;
        }
        if n == 2 {
            return None;
        }
        return Some((n, RegText::Sym(&src[2..n])));
    }
    let mut n = 1;
    let mut value = 0u8;
    while n < b.len() && n <= max_digits && (b'0'..=b'7').contains(&b[n]) {
        value = value * 8 + (b[n] - b'0');
        n += 1;
    }
    if n == 1 {
        return None;
    }
    if n < b.len() && (is_symbol_char(b[n]) || b[n] == b'.' || b[n] == b'\'') {
        return None;
    }
    Some((n, RegText::Num(value)))
}

fn match_tokens<'a>(toks: &[Tok], src: &'a str, caps: &mut Vec<Cap<'a>>) -> bool {
    let Some(tok) = toks.first() else {
        return src.is_empty();
    };
    match *tok {
        Tok::Lit(c) => {
            src.as_bytes().first() == Some(&c) && match_tokens(&toks[1..], &src[1..], caps)
        }
        Tok::Reg(letter, field) => {
            let Some((n, text)) = parse_reg(src, letter, 1) else {
                return false;
            };
            caps.push(Cap::Reg(field, text));
            if match_tokens(&toks[1..], &src[n..], caps) {
                return true;
            }
            caps.pop();
            false
        }
        Tok::Blk(letter) => {
            let Some((n, text)) = parse_reg(src, letter, 2) else {
                return false;
            };
            caps.push(Cap::Blk(text));
            if match_tokens(&toks[1..], &src[n..], caps) {
                return true;
            }
            caps.pop();
            false
        }
        Tok::Exp => match toks.get(1) {
            None => {
                if looks_like_expression(src) {
                    caps.push(Cap::Exp(src));
                    true
                } else {
                    false
                }
            }
            Some(&Tok::Lit(c)) => {
                let quoted = quoted_mask(src);
                let b = src.as_bytes();
                for p in (0..b.len()).rev() {
                    if b[p] == c && !quoted[p] && looks_like_expression(&src[..p]) {
                        caps.push(Cap::Exp(&src[..p]));
                        if match_tokens(&toks[1..], &src[p..], caps) {
                            return true;
                        }
                        caps.pop();
                    }
                }
                false
            }
            Some(_) => false,
        },
    }
}

fn first_byte_fits(toks: &[Tok], src: &str) -> bool {
    match (toks.first(), src.as_bytes().first()) {
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
        (Some(Tok::Lit(c)), Some(s)) => c == s,
        (Some(Tok::Reg(l, _)), Some(s)) | (Some(Tok::Blk(l)), Some(s)) => l == s,
        (Some(Tok::Exp), Some(_)) => true,
    }
}

type Eval<'e> = dyn FnMut(&str, ExpUse) -> Result<ExpValue, String> + 'e;

fn register_number(text: RegText, limit: i64, what: &str, eval: &mut Eval) -> Result<u8, String> {
    match text {
        RegText::Num(n) => Ok(n),
        RegText::Sym(s) => {
            let v = eval(s, ExpUse::Value)?;
            if v.value < 0 || v.value > limit {
                return Err(format!(
                    "{} register number {} is out of range (0 to {})",
                    what,
                    signed_octal(v.value),
                    signed_octal(limit)
                ));
            }
            Ok(v.value as u8)
        }
    }
}

/// Assemble one instruction from its CAL result and operand fields.
///
/// `eval` is called for each expression in the operands (and for the symbol
/// of register designators such as `B.NAME`) with the way the instruction
/// uses the value.  Rows of the table whose templates contain no `exp` are
/// tried first, then the others, each group in table order; the first
/// spelling that matches is used.  Where CAL lets the value choose the
/// encoding (`Ai exp`, `Si exp`, masks and shifts by 0 or 64) the choice
/// follows `Sel`.
pub fn assemble(result: &str, operand: &str, eval: &mut Eval) -> Result<Assembled, String> {
    let all = templates();
    for want_exp in [false, true] {
        for (n, t) in all.iter().enumerate() {
            let form = &FORMS[n];
            if t.has_exp != want_exp || !form.has_syntax() {
                continue;
            }
            if !first_byte_fits(&t.result, result) || !first_byte_fits(&t.operand, operand) {
                continue;
            }
            let mut caps = Vec::new();
            if !match_tokens(&t.result, result, &mut caps)
                || !match_tokens(&t.operand, operand, &mut caps)
            {
                continue;
            }
            if let Some(done) = bind(form, &caps, eval)? {
                return Ok(done);
            }
        }
    }
    Err(if operand.is_empty() {
        format!("no instruction form matches `{}`", result)
    } else {
        format!("no instruction form matches `{} {}`", result, operand)
    })
}

/// Turn the captures of a matched row into an encoding.  `Ok(None)` means the
/// row does not apply after all (the same register field was given two
/// different numbers).
fn bind(form: &'static Form, caps: &[Cap], eval: &mut Eval) -> Result<Option<Assembled>, String> {
    let mut regs: [Option<u8>; 4] = [None; 4]; // h, i, j, k
    let mut jk: Option<u8> = None;
    let mut exp_text: Option<&str> = None;
    for cap in caps {
        match *cap {
            Cap::Reg(field, text) => {
                let n = register_number(text, 7, "A, S or V", eval)?;
                let slot = match field {
                    b'h' => 0,
                    b'i' => 1,
                    b'j' => 2,
                    _ => 3,
                };
                if regs[slot].is_some_and(|old| old != n) {
                    return Ok(None);
                }
                regs[slot] = Some(n);
            }
            Cap::Blk(text) => jk = Some(register_number(text, 0o77, "B or T", eval)?),
            Cap::Exp(text) => exp_text = Some(text),
        }
    }
    let mut fields = Fields {
        h: regs[0].unwrap_or(0),
        i: regs[1].unwrap_or(0),
        j: regs[2].unwrap_or(0),
        k: regs[3].unwrap_or(0),
        m: 0,
    };
    if let Some(jk) = jk {
        fields.set_jk(jk);
    }
    if form.flags & flag::J_NONZERO != 0 && fields.j == 0 {
        return Err(format!(
            "`{} {}` needs a j register other than register 0",
            form.result, form.operand
        ));
    }
    let mut target = form;
    if let Some(text) = exp_text {
        let usage = match form.exp {
            ExpKind::Ijkm => ExpUse::Parcel,
            ExpKind::JkmSigned => ExpUse::Word,
            _ => ExpUse::Value,
        };
        let v = eval(text, usage)?;
        let x = v.value;
        let fits22 = |x: i64| (0..1 << 22).contains(&x);
        let neg22 = |x: i64| (-(1 << 22)..0).contains(&x);
        let too_big = |x: i64, reg: &str| {
            format!(
                "constant {} does not fit: {} takes 24 bits",
                signed_octal(x),
                reg
            )
        };
        match form.sel {
            Sel::None => fields.set_exp(form.exp, x)?,
            Sel::ImmA => {
                // the register is in i for `Ai exp` and in h for `Ah exp`
                let reg = if form.op == Op::ImmALong {
                    fields.h
                } else {
                    fields.i
                };
                (fields.h, fields.i) = (0, reg);
                // a 24-bit pattern with the top two bits set is a negative A value
                let all24 = x;
                let x = if (0xc0_0000..0x100_0000).contains(&x) {
                    x - 0x100_0000
                } else {
                    x
                };
                if (0..64).contains(&x) && !v.forward {
                    target = base_form(Op::ImmAShort);
                    fields.set_jk(x as u8);
                } else if fits22(x) {
                    target = base_form(Op::ImmA);
                    fields.set_jkm(x as u32);
                } else if neg22(x) {
                    target = base_form(Op::ImmANot);
                    fields.set_jkm(!(x as u32) & 0x3f_ffff);
                } else if (0..1 << 24).contains(&all24) {
                    // neither 22 bits nor their complement: the 24-bit form
                    target = base_form(Op::ImmALong);
                    fields.h = reg;
                    fields.set_exp(ExpKind::Ijkm24, all24)?;
                } else {
                    return Err(too_big(x, "Ai"));
                }
            }
            Sel::ImmANot | Sel::ImmSNot => {
                let (plain, not) = if form.sel == Sel::ImmANot {
                    (Op::ImmA, Op::ImmANot)
                } else {
                    (Op::ImmS, Op::ImmSNot)
                };
                if fits22(x) {
                    target = base_form(not);
                    fields.set_jkm(x as u32);
                } else if neg22(x) {
                    target = base_form(plain);
                    fields.set_jkm(!(x as u32) & 0x3f_ffff);
                } else {
                    return Err(too_big(
                        x,
                        if form.sel == Sel::ImmANot { "Ai" } else { "Si" },
                    ));
                }
            }
            Sel::ImmS => {
                if fits22(x) {
                    target = base_form(Op::ImmS);
                    fields.set_jkm(x as u32);
                } else if neg22(x) {
                    target = base_form(Op::ImmSNot);
                    fields.set_jkm(!(x as u32) & 0x3f_ffff);
                } else {
                    return Err(too_big(x, "Si"));
                }
            }
            Sel::Edge(other) => {
                let edge = match form.exp {
                    ExpKind::Jk => 64,
                    _ => 0,
                };
                if x == edge {
                    target = base_form(other);
                    fields.set_jk(0);
                } else if !(0..=64).contains(&x) {
                    return Err(format!(
                        "count {} is out of range (0 to 100)",
                        signed_octal(x)
                    ));
                } else {
                    fields.set_exp(form.exp, x)?;
                }
            }
        }
    }
    let encoding = if target.op == form.op {
        encode(form, fields)
    } else {
        encode(target, fields)
    };
    Ok(Some(Assembled {
        form: target.base(),
        syntax: form,
        fields,
        encoding,
    }))
}

/// Evaluate a numeric literal: octal digits by default, or `D'` decimal,
/// `O'` octal, `X'` hexadecimal, with an optional sign.
pub fn eval_number(text: &str) -> Option<i64> {
    let (neg, body) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let (radix, digits) = match body.as_bytes() {
        [b'D', b'\'', ..] => (10, &body[2..]),
        [b'O', b'\'', ..] => (8, &body[2..]),
        [b'X', b'\'', ..] => (16, &body[2..]),
        _ => (8, body),
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let v = u64::from_str_radix(digits, radix).ok()? as i64;
    Some(if neg { v.wrapping_neg() } else { v })
}

/// `assemble` for operands whose expressions are plain numbers.
pub fn assemble_numeric(result: &str, operand: &str) -> Result<Assembled, String> {
    assemble(result, operand, &mut |text: &str, _| {
        eval_number(text)
            .map(|value| ExpValue {
                value,
                forward: false,
            })
            .ok_or_else(|| format!("`{}` is not a number", text))
    })
}

fn render(toks: &[Tok], form: &Form, f: &Fields) -> String {
    let mut out = String::new();
    for tok in toks {
        match *tok {
            Tok::Lit(c) => out.push(c as char),
            Tok::Reg(letter, field) => {
                out.push(letter as char);
                let n = match field {
                    b'h' => f.h,
                    b'i' => f.i,
                    b'j' => f.j,
                    _ => f.k,
                };
                out.push((b'0' + (n & 7)) as char);
            }
            Tok::Blk(letter) => {
                out.push(letter as char);
                out.push_str(&format!("{:02o}", f.jk()));
            }
            Tok::Exp => out.push_str(&signed_octal(f.exp(form.exp).unwrap_or(0))),
        }
    }
    out
}

/// The CAL result and operand fields of row `n` of the table for the given
/// operand fields.
fn render_row(n: usize, f: &Fields) -> (String, String) {
    let t = &templates()[n];
    (
        render(&t.result, &FORMS[n], f),
        render(&t.operand, &FORMS[n], f),
    )
}

/// Disassemble into CAL result and operand fields.
///
/// Among the spellings of the instruction (special forms first, then the
/// general form, then alternates) this takes the first that assembles back
/// to the given parcels, or failing that to their canonical encoding
/// (ignored fields zero).  When CAL has no spelling that does either the
/// fields are `VWD` and `D'16/O'pppppp` (one item per parcel), which the
/// assembler turns back into the same parcels.
pub fn disassemble_fields(d: &Decoded) -> (String, String) {
    let original = Encoding {
        parcel0: d.parcel0,
        parcel1: (d.parcels == 2).then_some(d.m),
    };
    let canonical = d.canonical();
    let fields = d.fields();
    let gh = d.gh();
    let p1 = Some(d.m);
    for target in [original, canonical] {
        for kind in [Kind::Special, Kind::Base, Kind::Alt] {
            for (n, form) in rows_for_gh(gh) {
                if form.kind != kind || form.op != d.op || !form.has_syntax() {
                    continue;
                }
                if !form.matches(target.parcel0, p1) {
                    continue;
                }
                let (result, operand) = render_row(n, &fields);
                if let Ok(a) = assemble_numeric(&result, &operand) {
                    if a.encoding == target {
                        return (result, operand);
                    }
                }
            }
        }
    }
    let mut operand = format!("D'16/O'{:06o}", canonical.parcel0);
    if let Some(m) = canonical.parcel1 {
        operand.push_str(&format!(",D'16/O'{:06o}", m));
    }
    ("VWD".to_string(), operand)
}

/// Disassemble into one line of CAL text: the result field padded to ten
/// columns, then the operand field.  The assembler accepts the text back and
/// produces the canonical encoding of the instruction.
pub fn disassemble(d: &Decoded) -> String {
    let (result, operand) = disassemble_fields(d);
    if operand.is_empty() {
        result
    } else {
        format!("{:<9} {}", result, operand)
    }
}

impl Form {
    /// Operand fields that exercise this row: registers i=1, j=2, k=3, h=4
    /// and a typical expression value.
    pub fn example_fields(&self) -> Fields {
        let mut f = Fields {
            h: 4,
            i: 1,
            j: 2,
            k: 3,
            m: 0,
        };
        let value = match self.exp {
            ExpKind::None => None,
            ExpKind::Ijk => Some(0o123),
            ExpKind::J => Some(2),
            ExpKind::Jk | ExpKind::JkRev => Some(0o12),
            ExpKind::Jkm | ExpKind::Ijkm => Some(0o1234567),
            ExpKind::Ijkm24 => Some(0o23456701),
            ExpKind::JkmNot => Some(-0o1234567),
            ExpKind::JkmSigned => Some(0o1234),
        };
        if let Some(v) = value {
            f.set_exp(self.exp, v).expect("example value fits");
        }
        // The catch-all rows (001ixx, 002ixx) only own the larger values of i,
        // and 0014xk some values of k.  Take the first fields that mean this
        // row, and keep only what the pattern lets vary, so the fields equal
        // a decode.
        for i in f.i..=7 {
            for k in [f.k, 1, 2, 4, 5, 6, 7, 0] {
                let g = Fields { i, k, ..f };
                let e = encode(self, g);
                let d = decode(e.parcel0, e.parcel1);
                if d.op == self.op {
                    return d.fields();
                }
            }
        }
        let e = encode(self, f);
        decode(e.parcel0, e.parcel1).fields()
    }
    /// An example of this row in CAL: result and operand fields.  `None` for
    /// the rows CAL cannot spell.
    pub fn example(&self) -> Option<(String, String)> {
        let n = FORMS.iter().position(|f| std::ptr::eq(f, self))?;
        self.has_syntax()
            .then(|| render_row(n, &self.example_fields()))
    }
}

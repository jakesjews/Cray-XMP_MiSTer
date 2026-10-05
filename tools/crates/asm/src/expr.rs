//! Expressions: numbers, character constants, symbols, the location counter
//! `*`, the operators `+ - * /`, parentheses, and the `W.` and `P.` address
//! attribute prefixes.

use crate::Attr;
use cray1_isa::is_symbol_char;

/// The value of an expression or symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Val {
    pub value: i64,
    pub attr: Attr,
    /// Uses a symbol defined later in the source.
    pub forward: bool,
    /// Uses a symbol with no value yet (first pass only).
    pub unknown: bool,
}

impl Val {
    pub fn number(value: i64) -> Val {
        Val {
            value,
            attr: Attr::Value,
            forward: false,
            unknown: false,
        }
    }
    pub fn unknown() -> Val {
        Val {
            value: 0,
            attr: Attr::Value,
            forward: true,
            unknown: true,
        }
    }
    fn with(self, value: i64, attr: Attr, other: Val) -> Val {
        Val {
            value,
            attr,
            forward: self.forward || other.forward,
            unknown: self.unknown || other.unknown,
        }
    }
    /// The value as a word address (`W.` prefix).
    fn to_word(self) -> Val {
        match self.attr {
            Attr::Parcel => Val {
                value: self.value >> 2,
                attr: Attr::Word,
                ..self
            },
            _ => Val {
                attr: Attr::Word,
                ..self
            },
        }
    }
    /// The value as a parcel address (`P.` prefix).
    fn to_parcel(self) -> Val {
        match self.attr {
            Attr::Word => Val {
                value: self.value.wrapping_mul(4),
                attr: Attr::Parcel,
                ..self
            },
            _ => Val {
                attr: Attr::Parcel,
                ..self
            },
        }
    }
}

fn add(a: Val, b: Val) -> Val {
    use Attr::*;
    match (a.attr, b.attr) {
        (Value, x) | (x, Value) => a.with(a.value.wrapping_add(b.value), x, b),
        (Word, Word) => a.with(a.value.wrapping_add(b.value), Word, b),
        (Parcel, Parcel) => a.with(a.value.wrapping_add(b.value), Parcel, b),
        _ => {
            let (a, b) = (a.to_parcel(), b.to_parcel());
            a.with(a.value.wrapping_add(b.value), Parcel, b)
        }
    }
}

fn sub(a: Val, b: Val) -> Val {
    use Attr::*;
    match (a.attr, b.attr) {
        (x, Value) => a.with(a.value.wrapping_sub(b.value), x, b),
        (Value, _) => a.with(a.value.wrapping_sub(b.value), Value, b),
        (Word, Word) | (Parcel, Parcel) => a.with(a.value.wrapping_sub(b.value), Value, b),
        _ => {
            let (a, b) = (a.to_parcel(), b.to_parcel());
            a.with(a.value.wrapping_sub(b.value), Value, b)
        }
    }
}

struct Parser<'a, 'f> {
    text: &'a str,
    b: &'a [u8],
    pos: usize,
    loc: u64,
    lookup: &'f mut dyn FnMut(&str) -> Result<Val, String>,
}

impl Parser<'_, '_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn unexpected(&self) -> String {
        match self.peek() {
            None => "expression ends where a number or symbol is expected".to_string(),
            Some(_) => format!(
                "unexpected `{}` in expression `{}`",
                &self.text[self.pos..],
                self.text
            ),
        }
    }

    fn sum(&mut self) -> Result<Val, String> {
        let negate = if self.eat(b'-') {
            true
        } else {
            self.eat(b'+');
            false
        };
        let mut acc = self.term()?;
        if negate {
            acc = Val {
                value: acc.value.wrapping_neg(),
                attr: Attr::Value,
                ..acc
            };
        }
        loop {
            if self.eat(b'+') {
                let t = self.term()?;
                acc = add(acc, t);
            } else if self.eat(b'-') {
                let t = self.term()?;
                acc = sub(acc, t);
            } else {
                return Ok(acc);
            }
        }
    }

    fn term(&mut self) -> Result<Val, String> {
        let mut acc = self.prim()?;
        loop {
            if self.eat(b'*') {
                let p = self.prim()?;
                acc = acc.with(acc.value.wrapping_mul(p.value), Attr::Value, p);
            } else if self.eat(b'/') {
                let p = self.prim()?;
                if p.value == 0 && !p.unknown {
                    return Err(format!("division by zero in `{}`", self.text));
                }
                let q = if p.value == 0 {
                    0
                } else {
                    acc.value.wrapping_div(p.value)
                };
                acc = acc.with(q, Attr::Value, p);
            } else {
                return Ok(acc);
            }
        }
    }

    fn digits(&mut self, radix: u32) -> Result<Val, String> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
            self.pos += 1;
        }
        let digits = &self.text[start..self.pos];
        if digits.is_empty() {
            return Err(format!("digits expected in `{}`", self.text));
        }
        match u64::from_str_radix(digits, radix) {
            Ok(v) => Ok(Val::number(v as i64)),
            Err(_) => Err(match radix {
                8 => format!(
                    "`{}` is not an octal number (numbers are octal unless written D'{} or X'..)",
                    digits, digits
                ),
                10 => format!("`{}` is not a decimal number", digits),
                _ => format!("`{}` is not a hexadecimal number", digits),
            }),
        }
    }

    fn character_constant(&mut self) -> Result<Val, String> {
        let (bytes, end) = string_body(self.text, self.pos)?;
        self.pos = end;
        if bytes.len() > 8 {
            return Err(format!(
                "character constant `{}` is longer than 8 characters",
                String::from_utf8_lossy(&bytes)
            ));
        }
        let mut justify = b'R';
        if let Some(c) = self.peek() {
            if matches!(c, b'L' | b'R' | b'H')
                && !self.b.get(self.pos + 1).is_some_and(|n| is_symbol_char(*n))
            {
                justify = c;
                self.pos += 1;
            }
        }
        Ok(Val::number(pack(&bytes, justify)[0] as i64))
    }

    fn prim(&mut self) -> Result<Val, String> {
        let Some(c) = self.peek() else {
            return Err(self.unexpected());
        };
        match c {
            b'(' => {
                self.pos += 1;
                let v = self.sum()?;
                if !self.eat(b')') {
                    return Err(format!("missing `)` in `{}`", self.text));
                }
                Ok(v)
            }
            b'*' => {
                self.pos += 1;
                Ok(Val {
                    value: self.loc as i64,
                    attr: Attr::Parcel,
                    forward: false,
                    unknown: false,
                })
            }
            b'\'' => self.character_constant(),
            b'0'..=b'9' => self.digits(8),
            _ if is_symbol_char(c) => {
                let start = self.pos;
                while self.peek().is_some_and(is_symbol_char) {
                    self.pos += 1;
                }
                let word = &self.text[start..self.pos];
                match (word, self.peek()) {
                    ("D", Some(b'\'')) => {
                        self.pos += 1;
                        self.digits(10)
                    }
                    ("O", Some(b'\'')) => {
                        self.pos += 1;
                        self.digits(8)
                    }
                    ("X", Some(b'\'')) => {
                        self.pos += 1;
                        self.digits(16)
                    }
                    ("A", Some(b'\'')) => self.character_constant(),
                    ("W", Some(b'.')) => {
                        self.pos += 1;
                        Ok(self.prim()?.to_word())
                    }
                    ("P", Some(b'.')) => {
                        self.pos += 1;
                        Ok(self.prim()?.to_parcel())
                    }
                    _ => (self.lookup)(word),
                }
            }
            _ => Err(self.unexpected()),
        }
    }
}

/// Evaluate an expression.  `loc` is the parcel address `*` stands for.
/// A leading `#` complements the whole expression.
pub(crate) fn eval(
    text: &str,
    loc: u64,
    lookup: &mut dyn FnMut(&str) -> Result<Val, String>,
) -> Result<Val, String> {
    let mut p = Parser {
        text,
        b: text.as_bytes(),
        pos: 0,
        loc,
        lookup,
    };
    if p.b.is_empty() {
        return Err("an expression is missing".to_string());
    }
    let complement = p.eat(b'#');
    let v = p.sum()?;
    if p.pos != p.b.len() {
        return Err(p.unexpected());
    }
    Ok(if complement {
        Val {
            value: !v.value,
            attr: Attr::Value,
            ..v
        }
    } else {
        v
    })
}

/// Read the body of a character constant whose opening quote is at `start`
/// (`''` stands for one quote).  Returns the characters and the index just
/// past the closing quote.
pub(crate) fn string_body(text: &str, start: usize) -> Result<(Vec<u8>, usize), String> {
    let b = text.as_bytes();
    debug_assert_eq!(b[start], b'\'');
    let mut out = Vec::new();
    let mut i = start + 1;
    loop {
        match b.get(i) {
            None => {
                return Err(format!(
                    "character constant `{}` has no closing quote",
                    &text[start..]
                ))
            }
            Some(b'\'') if b.get(i + 1) == Some(&b'\'') => {
                out.push(b'\'');
                i += 2;
            }
            Some(b'\'') => return Ok((out, i + 1)),
            Some(c) => {
                out.push(*c);
                i += 1;
            }
        }
    }
}

/// Pack characters eight to a word.  `L`: left justified, zero filled.
/// `H`: left justified, blank filled.  `R`: right justified, zero filled.
/// An empty string gives one word.
pub(crate) fn pack(bytes: &[u8], justify: u8) -> Vec<u64> {
    let words = bytes.len().div_ceil(8).max(1);
    let total = words * 8;
    let fill = if justify == b'H' { b' ' } else { 0 };
    let mut buf = vec![fill; total];
    let at = if justify == b'R' {
        total - bytes.len()
    } else {
        0
    };
    buf[at..at + bytes.len()].copy_from_slice(bytes);
    buf.chunks(8)
        .map(|c| u64::from_be_bytes(c.try_into().unwrap()))
        .collect()
}

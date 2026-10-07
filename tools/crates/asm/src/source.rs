//! Source lines: splitting into fields, INCLUDE, and macro expansion.

use crate::Severity;
use cray_xmp_isa::{is_symbol_char, quoted_mask};
use std::collections::HashMap;
use std::rc::Rc;

/// An operand field must begin before this column (1-based).  With an empty
/// operand field the comment starts here, as in CAL.
pub(crate) const COMMENT_COLUMN: usize = 35;

/// The fields of one source line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Fields<'a> {
    pub label: &'a str,
    pub result: &'a str,
    pub operand: &'a str,
    /// Everything after the result field, untrimmed.
    pub after_result: &'a str,
    /// Everything after the operand field, untrimmed.
    pub after_operand: &'a str,
    /// The operand field is empty because the text after the result field
    /// starts at or beyond the comment column.
    pub late: bool,
}

/// The 1-based display column of byte `index`, with tab stops every 8.
fn column(line: &str, index: usize) -> usize {
    let mut col = 1;
    for c in line.as_bytes()[..index].iter() {
        if *c == b'\t' {
            col = (col - 1) / 8 * 8 + 9;
        } else {
            col += 1;
        }
    }
    col
}

fn skip_blanks(line: &str, mut i: usize) -> usize {
    let b = line.as_bytes();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

/// The end of the field that starts at `start`: the first blank that is not
/// inside a character constant.
fn field_end(line: &str, start: usize) -> usize {
    let rest = &line[start..];
    let quoted = quoted_mask(rest);
    for (n, c) in rest.bytes().enumerate() {
        if (c == b' ' || c == b'\t') && !quoted[n] {
            return start + n;
        }
    }
    line.len()
}

/// Split a line into location, result and operand fields.
///
/// * A `*` or `;` in column 1 makes the whole line a comment.
/// * The location field starts in column 1.
/// * The result field is the next run of non-blanks, wherever it starts.
/// * The operand field is the run after that, if it starts before column 35.
///   Text that starts at or beyond column 35 is a comment (CAL's rule for an
///   empty operand field), as is anything from a field starting with `;` and
///   anything after the operand field.
pub(crate) fn split_fields(line: &str) -> Fields<'_> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut f = Fields::default();
    let b = line.as_bytes();
    if b.is_empty() || b[0] == b'*' || b[0] == b';' {
        return f;
    }
    let mut i = 0;
    if b[0] != b' ' && b[0] != b'\t' {
        i = field_end(line, 0);
        f.label = &line[..i];
    }
    i = skip_blanks(line, i);
    if i >= b.len() || b[i] == b';' {
        return f;
    }
    let end = field_end(line, i);
    f.result = &line[i..end];
    f.after_result = &line[end..];
    f.after_operand = &line[end..];
    i = skip_blanks(line, end);
    if i >= b.len() || b[i] == b';' {
        return f;
    }
    if column(line, i) >= COMMENT_COLUMN {
        f.late = true;
        return f;
    }
    let end = field_end(line, i);
    f.operand = &line[i..end];
    f.after_operand = &line[end..];
    f
}

/// Split an operand field at the commas that are not inside a character
/// constant or parentheses.
pub(crate) fn split_list(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    if text.is_empty() {
        return out;
    }
    let quoted = quoted_mask(text);
    let mut depth = 0;
    let mut start = 0;
    for (n, c) in text.bytes().enumerate() {
        if quoted[n] {
            continue;
        }
        match c {
            b'(' => depth += 1,
            b')' if depth > 0 => depth -= 1,
            b',' if depth == 0 => {
                out.push(&text[start..n]);
                start = n + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// One line handed to the assembler passes, after INCLUDE and macro
/// expansion.  Lines that only appear in the listing have empty fields.
#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub file: usize,
    pub line: u32,
    /// Macro nesting depth; 0 for text read from a file.
    pub depth: u8,
    pub text: String,
    pub label: String,
    pub result: String,
    pub operand: String,
    /// The operand is empty because the text after the result field starts
    /// at or beyond column 35.
    pub late: bool,
    /// The macro an expanded line came from.
    pub origin: Option<Rc<str>>,
}

pub(crate) struct Msg {
    pub line: usize,
    pub severity: Severity,
    pub text: String,
}

struct Macro {
    name: Rc<str>,
    /// Parameter the location field of a call replaces (CAL-style prototype).
    loc_param: String,
    params: Vec<String>,
    locals: Vec<String>,
    body: Vec<String>,
}

struct Defining {
    mac: Macro,
    need_prototype: bool,
}

pub(crate) struct Preprocessor<'a, 'b> {
    pub files: Vec<String>,
    pub lines: Vec<Line>,
    pub msgs: Vec<Msg>,
    macros: HashMap<String, Rc<Macro>>,
    defining: Option<Defining>,
    include: &'a mut crate::IncludeResolver<'b>,
    expansions: u32,
    ended: bool,
}

const MAX_DEPTH: u8 = 32;

/// Replace whole symbols of `text` that are keys of `map`.
fn substitute(text: &str, map: &[(&str, &str)]) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        if is_symbol_char(b[i]) {
            let start = i;
            while i < b.len() && is_symbol_char(b[i]) {
                i += 1;
            }
            let word = &text[start..i];
            match map.iter().find(|(name, _)| *name == word) {
                Some((_, value)) => out.push_str(value),
                None => out.push_str(word),
            }
        } else {
            let start = i;
            i += 1;
            while i < b.len() && !is_symbol_char(b[i]) {
                i += 1;
            }
            out.push_str(&text[start..i]);
        }
    }
    out
}

/// Macro call arguments: split at top-level commas; an argument wrapped in
/// parentheses loses them, so it can contain commas.
fn split_args(operand: &str) -> Vec<&str> {
    split_list(operand)
        .into_iter()
        .map(|a| {
            if a.len() >= 2 && a.starts_with('(') && a.ends_with(')') {
                &a[1..a.len() - 1]
            } else {
                a
            }
        })
        .collect()
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_symbol_char) && !name.as_bytes()[0].is_ascii_digit()
}

impl<'a, 'b> Preprocessor<'a, 'b> {
    pub fn new(include: &'a mut crate::IncludeResolver<'b>) -> Self {
        Preprocessor {
            files: Vec::new(),
            lines: Vec::new(),
            msgs: Vec::new(),
            macros: HashMap::new(),
            defining: None,
            include,
            expansions: 0,
            ended: false,
        }
    }

    fn error(&mut self, text: String) {
        let line = self.lines.len() - 1;
        self.msgs.push(Msg {
            line,
            severity: Severity::Error,
            text,
        });
    }

    /// Add a line that only appears in the listing.
    fn push_text(
        &mut self,
        file: usize,
        line: u32,
        depth: u8,
        text: &str,
        origin: &Option<Rc<str>>,
    ) {
        self.push(file, line, depth, text, "", "", "", origin);
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        file: usize,
        line: u32,
        depth: u8,
        text: &str,
        label: &str,
        result: &str,
        operand: &str,
        origin: &Option<Rc<str>>,
    ) {
        self.lines.push(Line {
            file,
            line,
            depth,
            text: text.trim_end_matches(['\r', '\n']).to_string(),
            label: label.to_string(),
            result: result.to_string(),
            operand: operand.to_string(),
            late: false,
            origin: origin.clone(),
        });
    }

    /// Read a whole source file.
    pub fn file(&mut self, name: &str, text: &str, depth: u8) {
        let file = self.files.len();
        self.files.push(name.to_string());
        for (n, line) in text.lines().enumerate() {
            if self.ended {
                break;
            }
            self.handle(file, n as u32 + 1, depth, line, &None);
        }
        if depth == 0 {
            if let Some(d) = self.defining.take() {
                let last = self.lines.len().saturating_sub(1);
                self.msgs.push(Msg {
                    line: last,
                    severity: Severity::Error,
                    text: format!("macro {} has no ENDM", d.mac.name),
                });
            }
        }
    }

    fn handle(&mut self, file: usize, line: u32, depth: u8, text: &str, origin: &Option<Rc<str>>) {
        let f = split_fields(text);
        if self.defining.is_some() {
            self.push_text(file, line, depth, text, origin);
            self.define(&f, text);
            return;
        }
        match f.result {
            "MACRO" => {
                self.push_text(file, line, depth, text, origin);
                let mut mac = Macro {
                    name: "".into(),
                    loc_param: String::new(),
                    params: Vec::new(),
                    locals: Vec::new(),
                    body: Vec::new(),
                };
                let need_prototype = f.operand.is_empty();
                if !need_prototype {
                    mac.name = f.operand.into();
                    // the parameter list is the field after the name
                    let rest = f.after_operand;
                    let lead = rest.len() - rest.trim_start().len();
                    let start = text.trim_end_matches(['\r', '\n']).len() - rest.len() + lead;
                    let params = split_fields_at(text, start);
                    mac.params = split_list(params).into_iter().map(str::to_string).collect();
                    self.check_prototype(&mac);
                }
                self.defining = Some(Defining {
                    mac,
                    need_prototype,
                });
            }
            "ENDM" => {
                self.push_text(file, line, depth, text, origin);
                self.error("ENDM without MACRO".to_string());
            }
            "LOCAL" => {
                self.push_text(file, line, depth, text, origin);
                self.error("LOCAL is only allowed inside a macro definition".to_string());
            }
            "INCLUDE" => {
                self.push_text(file, line, depth, text, origin);
                let arg = f.after_result.trim();
                let name = match arg.as_bytes().first() {
                    Some(q @ (b'"' | b'\'')) => arg[1..].split(*q as char).next().unwrap_or(""),
                    _ => arg.split_whitespace().next().unwrap_or(""),
                };
                if name.is_empty() {
                    self.error("INCLUDE needs a file name".to_string());
                } else if depth >= MAX_DEPTH {
                    self.error(format!("INCLUDE \"{}\" is nested too deeply", name));
                } else {
                    let from = self.files[file].clone();
                    match (self.include)(name, &from) {
                        Ok((path, contents)) => self.file(&path, &contents, depth + 1),
                        Err(e) => self.error(format!("cannot include \"{}\": {}", name, e)),
                    }
                }
            }
            "END" => {
                self.push(
                    file, line, depth, text, f.label, f.result, f.operand, origin,
                );
                self.ended = true;
            }
            name if self.macros.contains_key(name) => {
                let mac = self.macros[name].clone();
                self.expand(&mac, &f, file, line, depth, text, origin);
            }
            _ => {
                self.push(
                    file, line, depth, text, f.label, f.result, f.operand, origin,
                );
                self.lines.last_mut().unwrap().late = f.late;
            }
        }
    }

    fn check_prototype(&mut self, mac: &Macro) {
        if !valid_name(&mac.name) {
            self.error(format!("`{}` is not a valid macro name", mac.name));
        }
        for p in mac
            .params
            .iter()
            .chain((!mac.loc_param.is_empty()).then_some(&mac.loc_param))
        {
            if !valid_name(p) {
                self.error(format!("`{}` is not a valid macro parameter name", p));
            }
        }
    }

    /// Handle a line inside a macro definition (already pushed to the listing).
    fn define(&mut self, f: &Fields, text: &str) {
        let def = self.defining.as_mut().unwrap();
        if def.need_prototype {
            if f.result.is_empty() {
                return;
            }
            def.need_prototype = false;
            def.mac.name = f.result.into();
            def.mac.loc_param = f.label.to_string();
            def.mac.params = split_list(f.operand)
                .into_iter()
                .map(str::to_string)
                .collect();
            let def = self.defining.take().unwrap();
            self.check_prototype(&def.mac);
            self.defining = Some(def);
            return;
        }
        match f.result {
            "ENDM" => {
                let def = self.defining.take().unwrap();
                self.macros
                    .insert(def.mac.name.to_string(), Rc::new(def.mac));
            }
            "MACRO" => self.error("a macro definition cannot contain another MACRO".to_string()),
            "LOCAL" => def
                .mac
                .locals
                .extend(split_list(f.operand).into_iter().map(str::to_string)),
            _ => def
                .mac
                .body
                .push(text.trim_end_matches(['\r', '\n']).to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn expand(
        &mut self,
        mac: &Rc<Macro>,
        f: &Fields,
        file: usize,
        line: u32,
        depth: u8,
        text: &str,
        origin: &Option<Rc<str>>,
    ) {
        // The call itself: listed, and its label (unless the macro takes it as
        // a parameter) is attached to whatever the macro generates first.
        let label = if mac.loc_param.is_empty() {
            f.label
        } else {
            ""
        };
        self.push(file, line, depth, text, label, "", "", origin);
        if depth >= MAX_DEPTH {
            self.error(format!(
                "macro {} is nested too deeply (does it call itself?)",
                mac.name
            ));
            return;
        }
        let args = split_args(f.operand);
        if args.len() > mac.params.len() {
            self.error(format!(
                "macro {} takes {} arguments, {} given",
                mac.name,
                mac.params.len(),
                args.len()
            ));
            return;
        }
        self.expansions += 1;
        let locals: Vec<String> = mac
            .locals
            .iter()
            .map(|l| format!("{}%{}", l, self.expansions))
            .collect();
        let mut map: Vec<(&str, &str)> = Vec::new();
        for (n, p) in mac.params.iter().enumerate() {
            map.push((p, args.get(n).copied().unwrap_or("")));
        }
        if !mac.loc_param.is_empty() {
            map.push((&mac.loc_param, f.label));
        }
        for (l, unique) in mac.locals.iter().zip(&locals) {
            map.push((l, unique));
        }
        let inner: Option<Rc<str>> = Some(mac.name.clone());
        for body in &mac.body {
            if self.ended {
                break;
            }
            let expanded = substitute(body, &map);
            self.handle(file, line, depth + 1, &expanded, &inner);
        }
    }
}

/// The field of `text` that starts at byte `start`, if it starts before the
/// comment column and is not a comment.
fn split_fields_at(text: &str, start: usize) -> &str {
    let text = text.trim_end_matches(['\r', '\n']);
    let b = text.as_bytes();
    if start >= b.len()
        || b[start] == b';'
        || b[start] == b'*'
        || column(text, start) >= COMMENT_COLUMN
    {
        return "";
    }
    &text[start..field_end(text, start)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(line: &str) -> (&str, &str, &str) {
        let f = split_fields(line);
        (f.label, f.result, f.operand)
    }

    #[test]
    fn fixed_column_layout() {
        assert_eq!(
            fields("         ERR                      000000"),
            ("", "ERR", "")
        );
        assert_eq!(
            fields("STRTVAL  CON       D'64           000100"),
            ("STRTVAL", "CON", "D'64")
        );
        assert_eq!(fields("CON1     =         O'4520"), ("CON1", "=", "O'4520"));
        assert_eq!(
            fields("         B7,A4     ,              034407"),
            ("", "B7,A4", ",")
        );
        assert_eq!(
            fields("         ,         B22,A5         035522"),
            ("", ",", "B22,A5")
        );
        assert_eq!(
            fields("         CON1+1,A1 A3             111300 004521"),
            ("", "CON1+1,A1", "A3")
        );
        assert_eq!(fields("*       000"), ("", "", ""));
        assert_eq!(fields("*\t10x"), ("", "", ""));
        assert_eq!(
            fields("         CON       10000000000    *Start executing code at parcel 64 - A0=0\r"),
            ("", "CON", "10000000000")
        );
        assert_eq!(
            fields("RX_SPIN  S0        RX_STAT,A0     *A0=0"),
            ("RX_SPIN", "S0", "RX_STAT,A0")
        );
        // the result field may start anywhere
        assert_eq!(
            fields("                                        EX"),
            ("", "EX", "")
        );
        assert!(split_fields("         ERR                      000000").late);
        assert!(!split_fields("         ERR").late);
    }

    #[test]
    fn free_form_layout() {
        assert_eq!(fields("LOOP A1 A1+1 count up"), ("LOOP", "A1", "A1+1"));
        assert_eq!(fields(" A1 A1+1"), ("", "A1", "A1+1"));
        assert_eq!(fields("\tJ\tLOOP\tback"), ("", "J", "LOOP"));
        assert_eq!(fields("\tEX ; done"), ("", "EX", ""));
        assert_eq!(fields("LOOP ; just a label"), ("LOOP", "", ""));
        assert_eq!(
            fields("A_VERY_LONG_LABEL_THAT_RUNS_PAST_COLUMN_35 EX 27"),
            ("A_VERY_LONG_LABEL_THAT_RUNS_PAST_COLUMN_35", "EX", "")
        );
        assert_eq!(fields("; comment"), ("", "", ""));
        assert_eq!(
            fields(" DATA 'two words',5 text"),
            ("", "DATA", "'two words',5")
        );
        assert_eq!(
            fields(" DATA 'it''s ok'H text"),
            ("", "DATA", "'it''s ok'H")
        );
        assert_eq!(fields(" A1 D'10 don't care"), ("", "A1", "D'10"));
        assert_eq!(fields(""), ("", "", ""));
        // tabs count to the next multiple of 8: the fifth tab stop is column 41
        assert_eq!(fields("\tEX\t\t\t\t27"), ("", "EX", ""));
        assert_eq!(fields("\tEX\t\t\t27"), ("", "EX", "27"));
    }

    #[test]
    fn lists_and_substitution() {
        assert_eq!(
            split_list("1,2,'a,b',(3,4),5"),
            ["1", "2", "'a,b'", "(3,4)", "5"]
        );
        assert_eq!(split_list(""), Vec::<&str>::new());
        assert_eq!(split_list(",A0"), ["", "A0"]);
        assert_eq!(split_args("A1,(B7,A4),"), ["A1", "B7,A4", ""]);
        assert_eq!(
            substitute("L R,R+RR  R", &[("R", "S1"), ("L", "X")]),
            "X S1,S1+RR  S1"
        );
    }
}

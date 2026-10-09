//! Write random characters and keys for a terminal, and the states the model's
//! Ampex terminal is in behind them, for `sim/build/ampex/Vterm_ampex_tb` to
//! check the hardware description of the terminal against.
//!
//! ```text
//! screen_random SEED CASES FILE [--events MIN MAX] [--focus AREA] [--every N]
//!               [--part I N] [--only CASE] [--script FILE]
//! ```
//!
//! A case is a terminal that has just been switched on and a few hundred to a
//! few thousand events for it: characters from the computer, keys, and now
//! and then both in the same clock.  They are drawn as `tools/py/d80test.py`
//! draws them for the terminal's firmware: every control character and escape
//! sequence, right and wrong, in every mode, and each case with more of one
//! area than of the others (`--focus` names one for all; the names are in
//! AREAS).  Left out is what the hardware has not: the printer port (`ESC P`
//! and `ESC J`) and the keys its keyboard cannot send.  One case in eight is
//! for the terminal with a single page, the printer's screen, and has no
//! `ESC N`, `ESC F` and `ESC v` and no PAGE key either.
//!
//! The state is written behind every hundredth event (`--every`) and at the
//! end.  A case is the same whatever is written of it and with it: `--part`
//! writes the cases whose number leaves I when divided by N, for N runs side
//! by side, and `--only` one case; with `--script` the events of that one are
//! also written as a script for the `screen_dump` example.
//!
//! The file, a case behind the other:
//!
//! ```text
//! C5 pages number      a case starts: 1 or 2 pages; its number (4 bytes)
//! 68 character         a character from the computer
//! 6B key               a key, by its code
//! 62 character key     both in the same clock; the character is first
//! 53 state             the state the terminal is in now
//! 45                   the case ends
//! ```
//!
//! A state is the page shown, the last row of each of two pages, the cursor's
//! row of memory and column, whether it is in the status line, program mode,
//! whether the terminal has hung (a byte each); the number of bells so far (4
//! bytes); the number of characters sent since the state
//! before (2 bytes) and the characters; then the status line and the 24 rows
//! of each page, a row as 0 (as in the state before), 1 and a cell (every
//! cell of the row is that), or 2 and 80 cells.  A cell is two bytes: the
//! four attributes, the protect bit, the character.  Numbers of more than a
//! byte have their low byte first.

use cray_xmp_ios::Screen;
use std::io::Write;

const LINES: usize = 24;
const COLUMNS: usize = 80;
const ESC: u8 = 0x1B;

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n
    }

    /// From `low` to `high`, both of them too.
    fn range(&mut self, low: u64, high: u64) -> u64 {
        low + self.below(high - low + 1)
    }

    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64) < p * (1u64 << 53) as f64
    }

    fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[self.below(of.len() as u64) as usize]
    }

    fn letter(&mut self, of: &str) -> u8 {
        self.pick(of.as_bytes())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Event {
    Host(u8),
    Key(u8),
}

const AREAS: [&str; 13] = [
    "mixed",
    "text",
    "cursor",
    "erase",
    "insert",
    "tabs",
    "protect",
    "attributes",
    "pages",
    "send",
    "program",
    "keyboard",
    "noise",
];

/// What a stream is made of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Text,
    Word,
    Move,
    Bell,
    Control,
    Sub,
    Del,
    Nul,
    Address,
    Corner,
    EraseLine,
    ErasePage,
    Clear,
    ClearAll,
    Char,
    Line,
    TabSet,
    Tab,
    Attribute,
    WriteAttribute,
    WriteProtect,
    Protect,
    Block,
    Program,
    Flip,
    Lock,
    Drawing,
    Field,
    Page,
    Send,
    Ask,
    Escape,
    Byte,
    Key,
    Function,
    Reset,
    Typing,
    ModeError,
    Form,
    Wall,
    OwnColumn,
    Scroll,
    Stripes,
    LastTabs,
    Hang,
}

use Group::*;

/// How much of each a mixed stream has.
const WEIGHTS: [(Group, f64); 45] = [
    (Text, 30.0),
    (Word, 20.0),
    (Move, 16.0),
    (Bell, 1.0),
    (Control, 3.0),
    (Sub, 1.0),
    (Del, 1.0),
    (Nul, 1.0),
    (Address, 10.0),
    (Corner, 4.0),
    (EraseLine, 4.0),
    (ErasePage, 3.0),
    (Clear, 2.0),
    (ClearAll, 1.0),
    (Char, 5.0),
    (Line, 5.0),
    (TabSet, 4.0),
    (Tab, 5.0),
    (Attribute, 5.0),
    (WriteAttribute, 2.0),
    (WriteProtect, 3.0),
    (Protect, 2.0),
    (Block, 2.0),
    (Program, 1.0),
    (Flip, 2.0),
    (Lock, 1.0),
    (Drawing, 3.0),
    (Field, 3.0),
    (Page, 2.0),
    (Send, 3.0),
    (Ask, 2.0),
    (Escape, 4.0),
    (Byte, 4.0),
    (Key, 8.0),
    (Function, 4.0),
    (Reset, 1.0),
    (Typing, 1.0),
    (ModeError, 0.7),
    (Form, 2.0),
    (Wall, 0.5),
    (OwnColumn, 0.4),
    (Scroll, 3.0),
    (Stripes, 1.5),
    (LastTabs, 1.0),
    (Hang, 0.15),
];

/// What a stream that keeps to one area has more of.
fn focus(area: &str) -> &'static [(Group, f64)] {
    match area {
        "text" => &[(Text, 80.0), (Scroll, 30.0), (Move, 40.0), (Corner, 20.0)],
        "cursor" => &[
            (Address, 60.0),
            (Corner, 40.0),
            (Move, 60.0),
            (Ask, 15.0),
            (Flip, 6.0),
            (Page, 6.0),
        ],
        "erase" => &[
            (EraseLine, 30.0),
            (ErasePage, 25.0),
            (Clear, 12.0),
            (ClearAll, 5.0),
            (Sub, 6.0),
            (WriteProtect, 12.0),
            (Protect, 8.0),
            (Attribute, 10.0),
            (WriteAttribute, 6.0),
        ],
        "insert" => &[
            (Char, 40.0),
            (Line, 40.0),
            (Flip, 8.0),
            (Page, 6.0),
            (Attribute, 8.0),
            (WriteAttribute, 6.0),
            (WriteProtect, 8.0),
            (Protect, 4.0),
            (Stripes, 10.0),
        ],
        "tabs" => &[
            (TabSet, 40.0),
            (Tab, 50.0),
            (EraseLine, 10.0),
            (Protect, 8.0),
            (WriteProtect, 8.0),
            (Corner, 15.0),
            (LastTabs, 10.0),
        ],
        "protect" => &[
            (Protect, 14.0),
            (WriteProtect, 25.0),
            (Form, 25.0),
            (Field, 20.0),
            (Tab, 25.0),
            (TabSet, 14.0),
            (Move, 40.0),
            (Send, 10.0),
            (ErasePage, 8.0),
            (Char, 10.0),
            (Flip, 5.0),
            (Wall, 3.0),
            (OwnColumn, 3.0),
            (Hang, 0.5),
        ],
        "attributes" => &[
            (Attribute, 40.0),
            (WriteAttribute, 16.0),
            (Field, 25.0),
            (WriteProtect, 8.0),
            (Drawing, 10.0),
            (Send, 8.0),
            (Line, 8.0),
            (Scroll, 8.0),
            (Stripes, 15.0),
        ],
        "pages" => &[
            (Page, 30.0),
            (Flip, 20.0),
            (Scroll, 20.0),
            (Line, 20.0),
            (Move, 30.0),
            (ClearAll, 4.0),
            (Protect, 5.0),
            (Form, 5.0),
            (Hang, 1.0),
        ],
        "send" => &[
            (Send, 40.0),
            (Ask, 10.0),
            (WriteAttribute, 10.0),
            (Attribute, 20.0),
            (WriteProtect, 14.0),
            (Protect, 10.0),
            (Form, 10.0),
            (Program, 4.0),
            (Drawing, 10.0),
            (Move, 30.0),
            (Key, 20.0),
        ],
        "program" => &[
            (Program, 12.0),
            (Control, 40.0),
            (Escape, 30.0),
            (Byte, 30.0),
            (Nul, 10.0),
            (Send, 8.0),
            (Function, 30.0),
            (Key, 20.0),
            (ModeError, 6.0),
        ],
        "keyboard" => &[
            (Key, 120.0),
            (Function, 50.0),
            (Block, 8.0),
            (Lock, 5.0),
            (Program, 2.0),
            (Reset, 4.0),
            (Typing, 20.0),
            (ModeError, 3.0),
        ],
        "noise" => &[(Byte, 200.0), (Escape, 60.0), (Control, 40.0)],
        _ => &[],
    }
}

/// The codes of the terminal's own keys that the keyboard of the core sends
/// (rtl/terminal/term_keyboard.v).
const OWN_KEYS: [u8; 26] = [
    0x82, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95,
    0x96, 0x97, 0xB4, 0xB5, 0xB6, 0xB7, 0xD4, 0xD9, 0xF4, 0xF9,
];

fn row_byte(rng: &mut Random) -> u8 {
    match rng.below(100) {
        0..=54 => 0x20 + rng.below(24) as u8,
        55..=74 => 0x20 + rng.pick(&[0, 1, 11, 12, 22, 23]),
        75..=86 => 0x20 + rng.range(24, 47) as u8,
        87..=93 => rng.below(0x20) as u8,
        _ => rng.range(0x38, 0x7F) as u8,
    }
}

fn column_byte(rng: &mut Random) -> u8 {
    match rng.below(100) {
        0..=54 => 0x20 + rng.below(80) as u8,
        55..=79 => 0x20 + rng.pick(&[0, 1, 71, 72, 75, 78, 79]),
        80..=91 => 0x20 + rng.range(80, 95) as u8,
        _ => rng.below(0x80) as u8,
    }
}

fn text(rng: &mut Random, n: u64) -> Vec<Event> {
    (0..n)
        .map(|_| Event::Host(0x20 + rng.below(0x5F) as u8))
        .collect()
}

fn host(bytes: &[u8]) -> Vec<Event> {
    bytes.iter().map(|&b| Event::Host(b)).collect()
}

/// The events of a case, drawn a group at a time.
struct Stream {
    groups: Vec<Group>,
    weights: Vec<f64>,
    total: f64,
    /// Some streams hold a mode on for most of their length.
    on_write_attribute: f64,
    on_write_protect: f64,
    on_protect: f64,
    on_block: f64,
    on_flip: f64,
    on_program: f64,
    on_lock: f64,
}

impl Stream {
    fn new(rng: &mut Random, area: &str) -> Stream {
        let mut weights: Vec<(Group, f64)> = WEIGHTS.to_vec();
        for &(group, more) in focus(area) {
            weights
                .iter_mut()
                .find(|(g, _)| *g == group)
                .expect("a group")
                .1 += more;
        }
        let often = [0.3, 0.5, 0.5, 0.8];
        Stream {
            groups: weights.iter().map(|w| w.0).collect(),
            total: weights.iter().map(|w| w.1).sum(),
            weights: weights.iter().map(|w| w.1).collect(),
            on_write_attribute: rng.pick(&often),
            on_write_protect: rng.pick(&often),
            on_protect: rng.pick(&often),
            on_block: rng.pick(&often),
            on_flip: rng.pick(&often),
            on_program: rng.pick(&[0.1, 0.25, 0.25, 0.6]),
            on_lock: rng.pick(&[0.2, 0.4]),
        }
    }

    fn draw(&self, rng: &mut Random) -> Group {
        let mut at = (rng.next() >> 11) as f64 / (1u64 << 53) as f64 * self.total;
        for (group, weight) in self.groups.iter().zip(&self.weights) {
            if at < *weight {
                return *group;
            }
            at -= weight;
        }
        Text
    }

    fn address(rng: &mut Random) -> Vec<Event> {
        host(&[ESC, b'=', row_byte(rng), column_byte(rng)])
    }

    fn pair(rng: &mut Random, on: f64, set: u8, clear: u8) -> Vec<Event> {
        host(&[ESC, if rng.chance(on) { set } else { clear }])
    }

    /// A key the keyboard sends: mostly a character.
    fn key(rng: &mut Random) -> u8 {
        match rng.below(100) {
            0..=64 => 0x20 + rng.below(0x5F) as u8,
            65..=79 => rng.range(1, 0x1F) as u8,
            80..=82 => 0x7F,
            _ => rng.pick(&OWN_KEYS),
        }
    }

    fn group(&self, rng: &mut Random, screen: &Screen) -> Vec<Event> {
        let group = self.draw(rng);
        // With protect mode on and nothing but protected cells the terminal
        // walks through every page for each character it gets: a stream does
        // not stay there for long.
        if screen.status_line().contains("PROT ERR") && rng.chance(0.4) {
            return match rng.below(4) {
                0 => host(&[ESC, b'\'']),
                1 => host(&[0x1A]),
                2 => host(&[ESC, b'*']),
                _ => host(&[ESC, b'+']),
            };
        }
        match group {
            Text => {
                let n = rng.range(1, 60);
                text(rng, n)
            }
            Word => {
                let n = rng.range(1, 6);
                text(rng, n)
            }
            Move => host(&[rng.pick(&[
                0x08, 0x08, 0x09, 0x0A, 0x0A, 0x0A, 0x0B, 0x0C, 0x0C, 0x0D, 0x0D, 0x1E, 0x1F,
            ])]),
            Bell => host(&[0x07]),
            Control => host(&[rng.below(0x20) as u8]),
            Sub => host(&[0x1A]),
            Del => host(&[0x7F]),
            Nul => host(&[0x00]),
            Address => Stream::address(rng),
            Corner => {
                let row = rng.pick(&[0, 0, 11, 12, 23, 23, 23]);
                let column = rng.pick(&[0, 0, 1, 77, 78, 79, 79, 79]);
                let mut out = host(&[ESC, b'=', 0x20 + row, 0x20 + column]);
                if rng.chance(0.6) {
                    let n = rng.range(1, 4);
                    out.extend(text(rng, n));
                }
                out
            }
            EraseLine => host(&[ESC, rng.letter("Tt")]),
            ErasePage => host(&[ESC, rng.letter("Yy;:")]),
            Clear => host(&[ESC, rng.letter("*+Z")]),
            ClearAll => host(&[ESC, b'X']),
            Char => host(&[ESC, rng.letter("QW")]),
            Line => host(&[ESC, rng.letter("ER")]),
            TabSet => host(&[ESC, rng.letter("1111223")]),
            Tab => match rng.below(4) {
                0 | 1 => host(&[0x09]),
                2 => host(&[ESC, b'i']),
                _ => host(&[ESC, b'I']),
            },
            Attribute => host(&[ESC, rng.letter("jklmnopq")]),
            WriteAttribute => Stream::pair(rng, self.on_write_attribute, b'A', b'a'),
            WriteProtect => Stream::pair(rng, self.on_write_protect, b')', b'('),
            Protect => Stream::pair(rng, self.on_protect, b'&', b'\''),
            Block => Stream::pair(rng, self.on_block, b'B', b'C'),
            Program => Stream::pair(rng, self.on_program, b'c', b'd'),
            Flip => Stream::pair(rng, self.on_flip, b'v', b'w'),
            Lock => Stream::pair(rng, self.on_lock, b'#', b'"'),
            Drawing => {
                let x = if rng.chance(0.85) {
                    b'A' + rng.below(11) as u8
                } else {
                    rng.below(0x80) as u8
                };
                host(&[ESC, b'G', x])
            }
            Field => {
                let p = rng.below(100);
                let mut r1 = rng.below(24) as u8;
                let step = [0, 0, 0, 1, 2, rng.below(24) as u8];
                let mut r2 = (r1 + rng.pick(&step)).min(23);
                let mut c1 = rng.below(80) as u8;
                let mut c2 = if r1 == r2 {
                    rng.range(u64::from(c1), 79) as u8
                } else {
                    rng.below(80) as u8
                };
                match p {
                    0..=59 => {}
                    // the end before the start
                    60..=67 => {
                        r2 = r1;
                        (c1, c2) = (c1.max(c2), c1.min(c2));
                    }
                    68..=73 => (r1, r2) = (r1.max(r2), r1.min(r2)),
                    // one of the four off the screen
                    74..=81 => match rng.below(4) {
                        0 => r1 = 24 + rng.below(8) as u8,
                        1 => c1 = 80 + rng.below(8) as u8,
                        2 => r2 = 24 + rng.below(8) as u8,
                        _ => c2 = 80 + rng.below(8) as u8,
                    },
                    // the whole page
                    82..=87 => (r1, c1, r2, c2) = (0, 0, 23, 79),
                    _ => {
                        return host(&[
                            ESC,
                            b'f',
                            row_byte(rng),
                            column_byte(rng),
                            row_byte(rng),
                            column_byte(rng),
                        ])
                    }
                }
                host(&[ESC, b'f', 0x20 + r1, 0x20 + c1, 0x20 + r2, 0x20 + c2])
            }
            Page => host(&[ESC, rng.letter("NNNF")]),
            Send => host(&[ESC, rng.letter("4567")]),
            Ask => host(&[ESC, b'?']),
            Escape => {
                let mut out = vec![Event::Host(ESC), Event::Host(rng.below(0x80) as u8)];
                // sometimes with bytes behind it that a sequence with parameters eats
                for _ in 0..rng.pick(&[0, 0, 0, 1, 2, 4]) {
                    out.push(Event::Host(rng.below(0x80) as u8));
                }
                out
            }
            Byte => host(&[rng.below(0x80) as u8]),
            Key => vec![Event::Key(Stream::key(rng))],
            // the keys that the terminal carries out itself
            Function => vec![Event::Key(rng.pick(&OWN_KEYS))],
            // CTRL+CLEAR, sometimes on a locked keyboard
            Reset => {
                let mut out = if rng.chance(0.3) {
                    host(&[ESC, b'#'])
                } else {
                    Vec::new()
                };
                out.push(Event::Key(0x82));
                out
            }
            Typing => (0..rng.range(3, 16))
                .map(|_| Event::Key(0x20 + rng.below(0x5F) as u8))
                .collect(),
            // the keys PROT MODE and WRITE PROT in program mode, and ESC c in
            // the modes it is refused in
            ModeError => {
                let mut out;
                if rng.chance(0.5) {
                    out = host(&[ESC, b'\'', ESC, b'(', ESC, b'a', ESC, b'c']);
                    out.push(Event::Key(rng.pick(&[0x86, 0x88])));
                } else {
                    out = host(&[ESC, rng.letter("&)A"), ESC, b'c']);
                }
                if rng.chance(0.6) {
                    out.extend(host(&[ESC, b'd']));
                }
                out
            }
            // a page with no cell that is not protected, protect mode, and
            // then something that looks for one
            Wall => {
                let mut out = host(&[ESC, b')', ESC, b'f', 0x20, 0x20, 0x20 + 23, 0x20 + 79]);
                if rng.chance(0.7) {
                    out.extend(host(&[ESC, b'&']));
                }
                let then: [&[u8]; 12] = [
                    &[ESC, b'5'],
                    &[ESC, b'4'],
                    &[0x09],
                    &[ESC, b'I'],
                    &[0x0A],
                    &[ESC, b'Q'],
                    &[ESC, b'1'],
                    &[ESC, b'7'],
                    &[ESC, b'N'],
                    &[ESC, b'X'],
                    // two errors at once: a cursor address off the screen
                    &[ESC, b'=', 0x7E, 0x7E],
                    // CTRL+CLEAR, which looks for a cell too
                    &[],
                ];
                let then = rng.pick(&then);
                out.extend(host(then));
                if then.is_empty() {
                    out.push(Event::Key(0x82));
                }
                if rng.chance(0.5) {
                    out.extend(host(&[ESC, b'(']));
                }
                out
            }
            // fields: protected text with gaps, sometimes whole protected lines
            Form => {
                let mut out = if rng.chance(0.5) {
                    Stream::address(rng)
                } else {
                    Vec::new()
                };
                for _ in 0..rng.range(1, 4) {
                    out.extend(host(&[ESC, b')']));
                    let n = rng.pick(&[1, 3, 8, 30, 80, 100]);
                    out.extend(text(rng, n));
                    out.extend(host(&[ESC, b'(']));
                    let n = rng.pick(&[0, 1, 2, 5, 12]);
                    out.extend(text(rng, n));
                    if rng.chance(0.3) {
                        out.extend(host(&[0x0D, 0x0A]));
                    }
                }
                if rng.chance(0.5) {
                    out.extend(host(&[ESC, b'&']));
                }
                out
            }
            // ESC 1 in protect mode with the cursor on the last cell of the
            // page, where the search before it ended: the cursor's own cell
            // becomes protected, and then something is done there
            OwnColumn => {
                let mut out = host(&[ESC, b'&', ESC, b'=', 0x20 + 23, 0x20 + 79, ESC, b'1']);
                let then: [&[u8]; 8] = [
                    &[ESC, b'Q'],
                    &[ESC, b'W'],
                    &[ESC, b'4'],
                    &[ESC, b'5'],
                    &[0x41],
                    &[0x08],
                    &[ESC, b'T'],
                    &[ESC, b'2'],
                ];
                out.extend(host(rng.pick(&then)));
                out
            }
            Scroll => {
                let row = rng.pick(&[21, 22, 23, 23]);
                let mut out = host(&[ESC, b'=', 0x20 + row, 0x20 + rng.below(80) as u8]);
                for _ in 0..rng.range(1, 30) {
                    let n = rng.pick(&[0, 3, 20, 85]);
                    out.extend(text(rng, n));
                    let end: [&[u8]; 3] = [&[0x0A], &[0x0D, 0x0A], &[0x1F]];
                    out.extend(host(rng.pick(&end)));
                }
                out
            }
            // lines with attributes, and then something that moves lines or
            // cells: a line or a cell that moves takes its attributes along,
            // one that is blanked keeps them
            Stripes => {
                let mut out = host(&[ESC, b'A']);
                for _ in 0..rng.range(1, 4) {
                    let any = rng.below(24) as u8;
                    let row = rng.pick(&[0, 0, 1, 11, 12, 22, 23, 23, any]);
                    out.extend(host(&[ESC, b'=', 0x20 + row, 0x20 + rng.below(80) as u8]));
                    out.extend(host(&[ESC, rng.letter("jlnp")]));
                    let n = rng.range(1, 30);
                    out.extend(text(rng, n));
                    if rng.chance(0.7) {
                        out.extend(host(&[ESC, rng.letter("kmoq")]));
                    }
                }
                if rng.chance(0.7) {
                    out.extend(host(&[ESC, b'a']));
                }
                let any = rng.below(24) as u8;
                let row = rng.pick(&[0, 1, 5, 11, 12, 20, 23, any]);
                out.extend(host(&[ESC, b'=', 0x20 + row, 0x20 + rng.below(80) as u8]));
                let then: [&[u8]; 8] = [
                    &[ESC, b'R'],
                    &[ESC, b'R'],
                    &[ESC, b'E'],
                    &[ESC, b'E'],
                    &[ESC, b'Q'],
                    &[ESC, b'W'],
                    &[ESC, b'T'],
                    &[ESC, b'=', 0x20 + 23, 0x20, 0x0A, 0x0A],
                ];
                out.extend(host(rng.pick(&then)));
                out
            }
            // a tab stop in one of the last eight columns, which an erase
            // sets or clears, and a tab to it
            LastTabs => {
                let row = 0x20 + rng.below(24) as u8;
                let mut out = host(&[
                    ESC,
                    b'=',
                    row,
                    0x20 + rng.range(72, 79) as u8,
                    ESC,
                    rng.letter("1112"),
                ]);
                let then: [&[u8]; 8] = [
                    &[],
                    &[ESC, b'3'],
                    &[ESC, b'T'],
                    &[ESC, b't'],
                    &[ESC, b'Y'],
                    &[ESC, b';'],
                    &[ESC, b'*'],
                    &[ESC, b'&', ESC, b'T', ESC, b'\''],
                ];
                out.extend(host(rng.pick(&then)));
                out.extend(host(&[
                    ESC,
                    b'=',
                    row,
                    0x20 + rng.range(60, 78) as u8,
                    0x09,
                ]));
                if rng.chance(0.4) {
                    out.extend(host(&[0x09]));
                }
                out
            }
            // What hangs the terminal: the last page with no cell that is
            // not protected, protect mode, auto flip and ESC X.  Not every
            // time with everything it takes.
            Hang => {
                let mut out = Vec::new();
                if rng.chance(0.8) {
                    out.extend(host(&[ESC, b'F', ESC, b'N']));
                }
                out.extend(host(&[
                    ESC,
                    b')',
                    ESC,
                    b'f',
                    0x20,
                    0x20,
                    0x20 + 23,
                    0x20 + 79,
                    ESC,
                    b'(',
                ]));
                if rng.chance(0.3) {
                    out.extend(host(&[ESC, b'N']));
                }
                if rng.chance(0.8) {
                    out.extend(host(&[ESC, b'v']));
                }
                if rng.chance(0.8) {
                    out.extend(host(&[ESC, b'&']));
                }
                out.extend(host(&[ESC, b'X']));
                out
            }
        }
    }
}

/// The terminal's collector of escape sequences, kept here as well: what a
/// character will be to the terminal is known before it is sent.
#[derive(Default)]
struct Collector {
    /// ESC has come and the letter is next.
    seen: bool,
    /// Parameters still to come.
    left: u8,
}

impl Collector {
    /// Whether `event` reaches the collector.
    fn takes(&self, screen: &Screen, event: Event) -> bool {
        if screen.hung() {
            return false;
        }
        let waits = self.seen || self.left != 0;
        match event {
            Event::Host(b) => !(b == 0 && screen.mode() & Screen::PGM == 0) && (b == ESC || waits),
            Event::Key(k) => {
                !screen.keyboard_locked()
                    && k < 0x80
                    && screen.mode() & Screen::BLK != 0
                    && (k == ESC || waits)
            }
        }
    }

    /// The letter of an escape sequence that `event` is, if it is one.
    fn letter(&self, screen: &Screen, event: Event) -> Option<u8> {
        let (Event::Host(b) | Event::Key(b)) = event;
        (self.seen && self.takes(screen, event)).then_some(b)
    }

    fn feed(&mut self, screen: &Screen, event: Event) {
        if !self.takes(screen, event) {
            return;
        }
        let (Event::Host(b) | Event::Key(b)) = event;
        if self.seen {
            self.seen = false;
            self.left = match b {
                b'G' => 1,
                b'=' | b'e' => 2,
                b'f' => 4,
                _ => 0,
            };
        } else if self.left != 0 {
            self.left -= 1;
        } else {
            self.seen = true;
        }
    }

    /// As the model shows its collector in its state.
    fn count(&self) -> u8 {
        if self.seen {
            0xFF
        } else {
            self.left
        }
    }
}

/// What the hardware has not, and the terminal of one page neither.
fn wanted(collector: &Collector, screen: &Screen, event: Event, one_page: bool) -> bool {
    if one_page && event == Event::Key(0x8A) {
        return false;
    }
    match collector.letter(screen, event) {
        Some(b'P' | b'J') => false,
        Some(b'N' | b'F' | b'v') => !one_page,
        _ => true,
    }
}

type Row = [u16; COLUMNS];

/// What the hardware is compared with.
struct State {
    head: [u8; 8],
    bells: u32,
    sent: Vec<u8>,
    /// The status line, then the rows of the pages.
    rows: Vec<Row>,
}

fn state(screen: &mut Screen, pages: usize, collector: &Collector) -> State {
    let sent = screen.take_sent();
    let dump = screen.dump();
    let word = |line: &str, name: &str| -> String {
        let words: Vec<&str> = line.split(' ').collect();
        let at = words
            .iter()
            .position(|w| *w == name)
            .expect("a name in the model's state");
        words[at + 1].to_string()
    };
    let shown: u8 = word(&dump[0], "shown").parse().expect("a page");
    let work = dump
        .iter()
        .find(|l| l.starts_with("work "))
        .expect("the working variables");
    let esc = u8::from_str_radix(&word(work, "esc"), 16).expect("the collector's count");
    assert_eq!(
        esc,
        collector.count(),
        "the collector kept here is not the model's"
    );
    let (row, column) = screen.cursor_memory();
    let mut rows = Vec::new();
    let mut status = [0u16; COLUMNS];
    for (c, cell) in status.iter_mut().enumerate() {
        let (code, flash) = screen.status_cell(c);
        *cell = u16::from(flash) << 10 | u16::from(code);
    }
    rows.push(status);
    for page in 0..pages {
        for memory_row in 0..LINES {
            let mut cells = [0u16; COLUMNS];
            for (c, cell) in cells.iter_mut().enumerate() {
                let m = screen.cell(page, memory_row, c);
                *cell =
                    u16::from(m.attributes) << 8 | u16::from(m.protect) << 7 | u16::from(m.code);
            }
            rows.push(cells);
        }
    }
    State {
        head: [
            shown,
            screen.bottom(0) as u8,
            screen.bottom(1) as u8,
            row as u8,
            column as u8,
            u8::from(screen.cursor_in_status()),
            u8::from(screen.mode() & Screen::PGM != 0),
            u8::from(screen.hung()),
        ],
        bells: screen.bells,
        sent,
        rows,
    }
}

fn write_state(out: &mut Vec<u8>, state: &State, before: &mut Vec<Row>) {
    out.push(0x53);
    out.extend(state.head);
    out.extend(state.bells.to_le_bytes());
    out.extend((state.sent.len() as u16).to_le_bytes());
    out.extend(&state.sent);
    for (n, row) in state.rows.iter().enumerate() {
        if before.get(n) == Some(row) {
            out.push(0);
        } else if row.iter().all(|c| *c == row[0]) {
            out.push(1);
            out.extend(row[0].to_le_bytes());
        } else {
            out.push(2);
            for cell in row {
                out.extend(cell.to_le_bytes());
            }
        }
    }
    *before = state.rows.clone();
}

struct Options {
    events: (u64, u64),
    focus: Option<String>,
    only: Option<u64>,
    part: (u64, u64),
    every: u64,
    script: Option<String>,
}

fn case(seed: u64, number: u64, options: &Options, script: &mut String) -> Vec<u8> {
    let mut rng = Random(
        (seed.wrapping_mul(1_000_003).wrapping_add(number)).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
    );
    for _ in 0..8 {
        rng.next();
    }
    let one_page = rng.below(8) == 0;
    let pages = if one_page { 1 } else { 2 };
    let area = match &options.focus {
        Some(area) => area.clone(),
        None => rng.pick(&AREAS).to_string(),
    };
    let stream = Stream::new(&mut rng, &area);
    let mut length = rng.range(options.events.0, options.events.1);
    let mut screen = Screen::default();
    let mut collector = Collector::default();
    let mut before: Vec<Row> = Vec::new();
    let mut out = vec![0xC5, pages as u8];
    out.extend((number as u32).to_le_bytes());
    script.push_str(&format!(
        "# seed {} case {}: {}, {} pages\n",
        seed, number, area, pages
    ));
    write_state(
        &mut out,
        &state(&mut screen, pages, &collector),
        &mut before,
    );
    script.push_str("d\n");
    let mut count = 0;
    let mut hung = false;
    while count < length {
        for event in stream.group(&mut rng, &screen) {
            if count >= length {
                break;
            }
            if !wanted(&collector, &screen, event, one_page) {
                continue;
            }
            collector.feed(&screen, event);
            match event {
                Event::Host(b) => {
                    screen.put(b);
                    // now and then a key in the same clock, which is worked
                    // off behind the character
                    let key = Event::Key(Stream::key(&mut rng));
                    if rng.chance(0.03) && wanted(&collector, &screen, key, one_page) {
                        let Event::Key(k) = key else { unreachable!() };
                        collector.feed(&screen, key);
                        screen.key(k);
                        out.extend([0x62, b, k]);
                        script.push_str(&format!("h {:02x}\nk {:02x}\n", b, k));
                    } else {
                        out.extend([0x68, b]);
                        script.push_str(&format!("h {:02x}\n", b));
                    }
                }
                Event::Key(k) => {
                    screen.key(k);
                    out.extend([0x6B, k]);
                    script.push_str(&format!("k {:02x}\n", k));
                }
            }
            count += 1;
            if count % options.every == 0 && count < length {
                write_state(
                    &mut out,
                    &state(&mut screen, pages, &collector),
                    &mut before,
                );
                script.push_str("d\n");
            }
            // a terminal that has hung gets a few more events, which it drops
            if screen.hung() && !hung {
                hung = true;
                length = length.min(count + 20);
            }
        }
    }
    write_state(
        &mut out,
        &state(&mut screen, pages, &collector),
        &mut before,
    );
    script.push_str("d\n");
    out.push(0x45);
    out
}

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let mut plain = Vec::new();
    let mut options = Options {
        events: (200, 3000),
        focus: None,
        only: None,
        part: (0, 1),
        every: 100,
        script: None,
    };
    let usage = || {
        eprintln!(
            "usage: screen_random SEED CASES FILE [--events MIN MAX] [--focus AREA] [--every N] \
             [--part I N] [--only CASE] [--script FILE]\nareas: {}",
            AREAS.join(" ")
        );
        std::process::ExitCode::from(64)
    };
    while let Some(a) = args.next() {
        let mut number = || args.next().and_then(|v| v.parse::<u64>().ok());
        match a.as_str() {
            "--events" => match (number(), number()) {
                (Some(low), Some(high)) if low <= high => options.events = (low, high),
                _ => return usage(),
            },
            "--part" => match (number(), number()) {
                (Some(i), Some(n)) if i < n => options.part = (i, n),
                _ => return usage(),
            },
            "--only" => match number() {
                Some(n) => options.only = Some(n),
                None => return usage(),
            },
            "--every" => match number() {
                Some(n) if n > 0 => options.every = n,
                _ => return usage(),
            },
            "--focus" => match args.next() {
                Some(area) if AREAS.contains(&area.as_str()) => options.focus = Some(area),
                _ => return usage(),
            },
            "--script" => match args.next() {
                Some(file) => options.script = Some(file),
                None => return usage(),
            },
            _ => plain.push(a),
        }
    }
    let parsed = (|| {
        let [seed, cases, file] = &plain[..] else {
            return None;
        };
        Some((seed.parse::<u64>().ok()?, cases.parse::<u64>().ok()?, file))
    })();
    let Some((seed, cases, file)) = parsed else {
        return usage();
    };
    let mut out = match std::fs::File::create(file) {
        Ok(out) => std::io::BufWriter::new(out),
        Err(e) => {
            eprintln!("screen_random: {}: {}", file, e);
            return 66.into();
        }
    };
    let mut script = String::new();
    for number in 0..cases {
        if options.only.is_some_and(|only| only != number)
            || number % options.part.1 != options.part.0
        {
            continue;
        }
        script.clear();
        let record = case(seed, number, &options, &mut script);
        if let Err(e) = out.write_all(&record) {
            eprintln!("screen_random: {}: {}", file, e);
            return 74.into();
        }
    }
    if let Some(path) = &options.script {
        if let Err(e) = std::fs::write(path, &script) {
            eprintln!("screen_random: {}: {}", path, e);
            return 74.into();
        }
    }
    0.into()
}

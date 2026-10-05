//! The screen of an Ampex Dialogue 80, the terminal the IOS software drives
//! its consoles as: 24 lines of 80 characters.
//!
//! Only what the software was seen to send is acted on (the devices spec,
//! part 9.3): the cursor address `ESC = row column`, form feed as cursor
//! right, line feed, carriage return, backspace, `ESC *` clear the screen,
//! `ESC T` erase to the end of the line, `ESC R` delete the line.  Every
//! other control character and escape sequence is dropped.

pub const LINES: usize = 24;
pub const COLUMNS: usize = 80;

/// What has been received of an escape sequence.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Escape {
    None,
    Start,
    /// `ESC =`, and then the row.
    Row,
    Column(u8),
    /// `ESC G`: one more character follows.
    Skip,
}

pub struct Screen {
    cells: [[u8; COLUMNS]; LINES],
    line: usize,
    column: usize,
    escape: Escape,
    /// Times the bell was rung.
    pub bells: u32,
}

impl Default for Screen {
    fn default() -> Screen {
        Screen {
            cells: [[b' '; COLUMNS]; LINES],
            line: 0,
            column: 0,
            escape: Escape::None,
            bells: 0,
        }
    }
}

impl Screen {
    /// The screen after `bytes` have been sent to it.
    pub fn of(bytes: &[u8]) -> Screen {
        let mut screen = Screen::default();
        for &b in bytes {
            screen.put(b);
        }
        screen
    }

    /// Line `n` as text, without the blanks at its end.
    pub fn line(&self, n: usize) -> String {
        String::from_utf8_lossy(&self.cells[n])
            .trim_end()
            .to_string()
    }

    /// The lines, without the empty ones at the bottom.
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = (0..LINES).map(|n| self.line(n)).collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Where the cursor is: line and column, from 0.
    pub fn cursor(&self) -> (usize, usize) {
        (self.line, self.column)
    }

    fn line_feed(&mut self) {
        if self.line + 1 < LINES {
            self.line += 1;
        } else {
            self.cells.copy_within(1.., 0);
            self.cells[LINES - 1] = [b' '; COLUMNS];
        }
    }

    /// One character from the line.
    pub fn put(&mut self, byte: u8) {
        let byte = byte & 0x7F;
        match self.escape {
            Escape::Start => {
                self.escape = Escape::None;
                match byte {
                    b'=' => self.escape = Escape::Row,
                    b'G' => self.escape = Escape::Skip,
                    b'*' => {
                        self.cells = [[b' '; COLUMNS]; LINES];
                        self.line = 0;
                        self.column = 0;
                    }
                    b'T' => self.cells[self.line][self.column..].fill(b' '),
                    b'R' => {
                        self.cells.copy_within(self.line + 1.., self.line);
                        self.cells[LINES - 1] = [b' '; COLUMNS];
                    }
                    _ => {}
                }
                return;
            }
            Escape::Row => {
                self.escape = Escape::Column(byte);
                return;
            }
            Escape::Column(row) => {
                self.escape = Escape::None;
                self.line = (row.saturating_sub(0x20) as usize).min(LINES - 1);
                self.column = (byte.saturating_sub(0x20) as usize).min(COLUMNS - 1);
                return;
            }
            Escape::Skip => {
                self.escape = Escape::None;
                return;
            }
            Escape::None => {}
        }
        match byte {
            0x1B => self.escape = Escape::Start,
            0x07 => self.bells += 1,
            0x08 => self.column = self.column.saturating_sub(1),
            0x0A => self.line_feed(),
            0x0C => self.column = (self.column + 1).min(COLUMNS - 1),
            0x0D => self.column = 0,
            0x20..=0x7E => {
                self.cells[self.line][self.column] = byte;
                if self.column + 1 < COLUMNS {
                    self.column += 1;
                } else {
                    self.column = 0;
                    self.line_feed();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sequences_the_station_sends() {
        let mut s = Screen::of(b"ONE\r\nTWO\r\nTHREE");
        assert_eq!(s.text(), "ONE\nTWO\nTHREE");
        assert_eq!(s.cursor(), (2, 5));
        // cursor to line 1, column 1; erase to the end of the line
        for b in *b"\x1b=!!\x1bT" {
            s.put(b);
        }
        assert_eq!(s.text(), "ONE\nT\nTHREE");
        // cursor right twice, then a character
        for b in *b"\x0c\x0cX" {
            s.put(b);
        }
        assert_eq!(s.line(1), "T  X");
        // delete line 1: the lines below move up
        for b in *b"\x1bR" {
            s.put(b);
        }
        assert_eq!(s.text(), "ONE\nTHREE");
        for b in *b"\x07\x1b*A" {
            s.put(b);
        }
        assert_eq!((s.text().as_str(), s.bells, s.cursor()), ("A", 1, (0, 1)));
        // an address off the screen stays on it; a line drawing sequence is dropped
        for b in *b"\x1b=\x7f\x7f\x1bGAZ" {
            s.put(b);
        }
        // the last column of the last line: the screen moves up
        assert_eq!(s.line(22).len(), 80);
        assert_eq!(s.cursor(), (23, 0));
    }

    #[test]
    fn a_scrolling_console() {
        let mut text = Vec::new();
        for n in 0..30 {
            text.extend(format!("\n\rLINE {}", n).bytes());
        }
        let s = Screen::of(&text);
        assert_eq!(s.line(0), "LINE 6");
        assert_eq!(s.line(23), "LINE 29");
    }
}

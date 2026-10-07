//! Write random character streams and the screens the model's Ampex terminal
//! makes of them, for `sim/build/ampex/Vterm_ampex_tb` to check the hardware
//! description of the terminal against.
//!
//! ```text
//! screen_random SEED CASES FILE
//! ```
//!
//! A case in the file is the length of the stream (4 bytes, low byte first),
//! the stream, the 24 lines of 80 characters, and the cursor's line and column.

use cray_xmp_ios::Screen;
use std::io::Write;

const LINES: usize = 24;
const COLUMNS: usize = 80;

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
}

fn stream(random: &mut Random) -> Vec<u8> {
    let length = 50 + random.below(4000) as usize;
    // some streams keep to the bottom of the screen, where it scrolls
    let low = random.below(4) == 0;
    let mut out = Vec::new();
    while out.len() < length {
        match random.below(100) {
            0..=54 => {
                for _ in 0..1 + random.below(40) {
                    out.push(0x20 + random.below(0x5F) as u8);
                }
            }
            55..=62 => out.push(0x0A),
            63..=67 => out.push(0x0D),
            68..=70 => out.push(0x08),
            71..=74 => out.push(0x0C),
            75..=82 => {
                let line = if low {
                    20 + random.below(5)
                } else {
                    random.below(26)
                };
                // the last columns more often than chance would have them
                let column = match random.below(4) {
                    0 => 76 + random.below(6),
                    _ => random.below(82),
                };
                out.extend([0x1B, b'=', 0x20 + line as u8, 0x20 + column as u8]);
            }
            83..=85 => out.extend([0x1B, b'T']),
            86..=89 => out.extend([0x1B, b'R']),
            90 => out.extend([0x1B, b'*']),
            91 => out.extend([0x1B, b'G', b'A' + random.below(11) as u8]),
            92 => out.push(0x07),
            // a cursor address below the first row and column
            93 => out.extend([0x1B, b'=', random.below(0x20) as u8, random.below(0x20) as u8]),
            94..=96 => out.extend([0x1B, random.below(128) as u8]),
            _ => out.push(random.below(128) as u8),
        }
    }
    out
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = (|| {
        let [seed, cases, file] = &args[..] else {
            return None;
        };
        Some((seed.parse::<u64>().ok()?, cases.parse::<u32>().ok()?, file))
    })();
    let Some((seed, cases, file)) = parsed else {
        eprintln!("usage: screen_random SEED CASES FILE");
        return 64.into();
    };
    let mut out = match std::fs::File::create(file) {
        Ok(out) => std::io::BufWriter::new(out),
        Err(e) => {
            eprintln!("screen_random: {}: {}", file, e);
            return 66.into();
        }
    };
    let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    for _ in 0..cases {
        let bytes = stream(&mut random);
        let screen = Screen::of(&bytes);
        let mut record = (bytes.len() as u32).to_le_bytes().to_vec();
        record.extend(&bytes);
        for n in 0..LINES {
            record.extend(format!("{:<1$}", screen.line(n), COLUMNS).bytes());
        }
        let (line, column) = screen.cursor();
        record.extend([line as u8, column as u8]);
        if let Err(e) = out.write_all(&record) {
            eprintln!("screen_random: {}: {}", file, e);
            return 74.into();
        }
    }
    0.into()
}

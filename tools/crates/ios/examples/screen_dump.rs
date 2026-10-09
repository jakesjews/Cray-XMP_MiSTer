//! Run a script of events on the model's Ampex terminal and print its state,
//! for `tools/py/d80test.py` to compare with what the terminal's firmware
//! makes of the same script.
//!
//! ```text
//! screen_dump FILE
//! ```
//!
//! A line of the file is one of
//!
//! ```text
//! config pages=2 duplex=full wrap=1 bell=0 hz=60 lock=0 fk=0 printer=0
//! h 1b 3d 21 21 41       bytes from the host (hexadecimal)
//! K 2 2 0 0              a key: matrix row, bit, SHIFT held, CTRL held
//! k 61                   a key by the code it sends (hexadecimal)
//! P a 0 1                PROG A (or b) held, and the key at row 0 bit 1
//! d                      print the state
//! tables                 print the four tables of key codes
//! ```
//!
//! `config` switches the terminal on with the switches as given, and is the
//! first line if it is there; `printer` must be 0, since the model has none.
//! What follows `#` is a comment.  The state is printed as lines of text that
//! end in a line `end`; `tools/py/d80emu.py` says what they are.

use cray_xmp_ios::Screen;

fn run(text: &str) -> Result<(), String> {
    let mut screen = Screen::default();
    for (number, line) in text.lines().enumerate() {
        let words: Vec<&str> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        let bad = || format!("line {}: {}", number + 1, line);
        let int = |word: &str| word.parse::<usize>().map_err(|_| bad());
        let hex = |word: &str| u8::from_str_radix(word, 16).map_err(|_| bad());
        match words[..] {
            [] => {}
            ["config", ref settings @ ..] => {
                let mut wrong = false;
                screen = Screen::with(|w| {
                    for setting in settings {
                        match setting.split_once('=') {
                            Some(("pages", "2")) => w.pages = 2,
                            Some(("pages", "4")) => w.pages = 4,
                            Some(("duplex", "full")) => w.half_duplex = false,
                            Some(("duplex", "half")) => w.half_duplex = true,
                            Some(("wrap", v @ ("0" | "1"))) => w.wrap = v == "1",
                            Some(("hz", v @ ("50" | "60"))) => w.fifty_hertz = v == "50",
                            Some(("lock", v @ ("0" | "1"))) => w.locked = v == "1",
                            Some(("fk", v @ ("0" | "1"))) => w.function_keys = v == "1",
                            Some(("printer", "0")) => {}
                            Some(("bell", v)) => match v.parse::<u8>() {
                                Ok(column) if column < 128 => w.bell_column = column,
                                _ => wrong = true,
                            },
                            _ => wrong = true,
                        }
                    }
                });
                if wrong {
                    return Err(bad());
                }
            }
            ["h", ref bytes @ ..] => {
                for byte in bytes {
                    screen.put(hex(byte)?);
                }
            }
            ["k", code] => screen.key(hex(code)?),
            ["K", row, bit, shift, ctrl] => {
                let (row, bit) = (int(row)?, int(bit)?);
                if row > 12 || bit > 7 {
                    return Err(bad());
                }
                let code =
                    Screen::key_code(row, bit, shift == "1", ctrl == "1", screen.caps_lock());
                screen.key(code);
            }
            ["P", prog @ ("a" | "b"), row, bit] => {
                let (row, bit) = (int(row)?, int(bit)?);
                if row > 12 || bit > 7 {
                    return Err(bad());
                }
                let code = Screen::key_code(row, bit, false, false, screen.caps_lock());
                screen.prog_key(prog == "b", code);
            }
            ["d"] => println!("{}", screen.dump().join("\n")),
            ["tables"] => {
                let tables = [
                    ("plain", false, false, false),
                    ("ctrl", false, true, false),
                    ("shift", true, false, false),
                    ("caps", false, false, true),
                ];
                for (name, shift, ctrl, caps) in tables {
                    let codes: String = Screen::keys(shift, ctrl, caps)
                        .iter()
                        .map(|c| format!("{:02x}", c))
                        .collect();
                    println!("{} {}", name, codes);
                }
            }
            _ => return Err(bad()),
        }
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file] = &args[..] else {
        eprintln!("usage: screen_dump FILE");
        return 64.into();
    };
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("screen_dump: {}: {}", file, e);
            return 66.into();
        }
    };
    match run(&text) {
        Ok(()) => 0.into(),
        Err(e) => {
            eprintln!("screen_dump: {}: {}", file, e);
            65.into()
        }
    }
}

//! `cray-xmp-run`: run a memory image on the CRAY-1 reference model.
//!
//! ```text
//! cray-xmp-run IMAGE [--machine CRAY1|XMP] [--max N] [--input TEXT] [--state OUT]
//!           [--trace OUT] [--quiet]
//! ```
//!
//! * `--machine XMP`: the machine with the X-MP features (see the crate
//!   documentation): four million words, the X-MP exchange package.
//! * `IMAGE`: raw big-endian 64-bit words, loaded at word 0.  The machine
//!   dead starts from the exchange package in words 0 to 15.
//! * `--max N`: stop after N steps (default 10000000).  A step is an
//!   instruction; the dead start counts as one, as does an exchange caused by
//!   a flag that was already set in a package when it was loaded.
//! * `--input TEXT`: console input, available from the start.  `\n`, `\r`,
//!   `\t`, `\0`, `\\` and `\xHH` are escapes.
//! * `--state OUT`: write the end state (format below).
//! * `--trace OUT`: write one line per event (format below).
//! * `--quiet`: do not copy console output to standard output and do not
//!   print the summary line.  Test errors are still reported.
//!
//! Console output goes to standard output; the summary and errors go to
//! standard error.
//!
//! # Exit status
//!
//! * The value written to `TEST_EXIT` (word 3777762 octal): 0 stays 0, 1 to
//!   255 as they are, anything larger becomes 255.
//! * 2: no write to `TEST_EXIT` within `--max` steps.
//! * 3: a test error.  The model stopped because the program used an
//!   undefined value where it steers the machine (`undefined value used`), or
//!   did something the manual does not define.  The message names P and the
//!   instruction.
//! * 64: bad command line; 66: a file could not be read or written.
//!
//! A test that itself exits with 2 or 3 cannot be told from these by the
//! status alone; the `exit` line of the state file can.
//!
//! # State file (`--state`)
//!
//! Text, one item per line, fields separated by one space, lower case hex
//! digits, no trailing spaces, lines ending in a newline.  In this order:
//!
//! ```text
//! exit <code>            the value written to TEST_EXIT, in decimal (the
//!                        full 64-bit value, not the saturated status), or
//! exit none              if the run did not end by a write to TEST_EXIT
//! console <hex>          every byte written to CON_DATA, two hex digits
//!                        each, no separators; just `console` if none
//! mem <addr> <value>     one line for every memory word the run stored
//!                        into, in ascending address order: <addr> is the
//!                        absolute word address as 7 octal digits, <value>
//!                        the final contents as 16 hex digits, or the word
//!                        `undef` if the model cannot predict it
//! A0 <6 hex> ... A7      or `undef`
//! S0 <16 hex> ... S7     or `undef`
//! VL <2 hex>             the 7-bit register, or `undef`
//! VM <16 hex>            or `undef`; the most significant bit is element 0
//! P <6 hex>              the 22-bit parcel address, relative to BA
//! BA <5 hex>             the 18-bit register (base = BA * 16)
//! LA <5 hex>             the 18-bit register (limit = LA * 16)
//! XA <2 hex>             the 8-bit register (package at XA * 16)
//! M <1 hex>              8 correctable memory error mode, 4 floating point
//!                        mode, 2 uncorrectable memory error mode, 1 monitor
//! F <3 hex>              100 (hex) console, 080 real-time clock, 040
//!                        floating point error, 020 operand range, 010
//!                        program range, 008 memory error, 004 I/O, 002 error
//!                        exit, 001 normal exit
//! B<jk> <6 hex>          only the B registers that are defined, jk as two
//!                        octal digits, ascending
//! T<jk> <16 hex>         only the T registers that are defined
//! V<i>[<e>] <16 hex>     only the V register elements that are defined; e
//!                        in decimal; V0 first, elements ascending
//! ```
//!
//! "Stored into" means a store by the run itself: 11h, 13h, 035, 037, 177
//! and the sixteen words of every exchange, the dead start included (it
//! leaves words 0 to 15 `undef`).  A store that was outside the field
//! (BA, LA) is not one.  The image load does not count, and the I/O page
//! (words 3777760 to 3777777 octal) never appears.
//!
//! A hardware simulation writes the same `exit`, `console` and `mem` lines
//! and compares them with these, skipping the `mem` lines whose value is
//! `undef`.  The register lines are extra information from the model.
//!
//! # Trace file (`--trace`)
//!
//! One line per event, in the order things happen.  Values are hex with a
//! fixed number of digits; an undefined value is the word `undef`.
//!
//! ```text
//! I <P> <parcel> [<parcel>]  <text>   an instruction issues: P as 8 octal
//!                        digits (relative to BA), each parcel as 6 octal
//!                        digits, the second column blank for a one-parcel
//!                        instruction, then the disassembly
//! A<i> <6 hex>           Ai written
//! S<i> <16 hex>          Si written
//! B<jk> <6 hex>          Bjk written, jk as two octal digits
//! T<jk> <16 hex>         Tjk written
//! V<i>[<e>] <16 hex>     element e (decimal) of Vi written; a vector
//!                        instruction lists its elements in element order
//! VL <2 hex>             VL written
//! VM <16 hex>            VM written
//! M <addr> <16 hex>      memory word stored, absolute address as 7 octal
//!                        digits
//! X start <2 hex>        an exchange sequence begins; the value is XA
//! X done                 the exchange sequence is complete
//! C <2 hex>              a byte written to the console
//! ```
//!
//! and, beyond the list above, for the registers that have no line there:
//!
//! ```text
//! XA <2 hex>             XA written (0013, exchange)
//! MODE <1 hex>           M written (0021, 0022, exchange)
//! F <3 hex>              F written (a flag set, exchange)
//! P <6 hex>, BA <5 hex>, LA <5 hex>   loaded by an exchange
//! RT <16 hex>            the real-time clock entered by 0014
//! E <decimal>            TEST_EXIT written
//! ```
//!
//! Between `X start` and `X done` come the sixteen `M` lines of the package
//! stored (all `undef` for the dead start) and then P, BA, LA, MODE, XA, VL,
//! F, A0 to A7 and S0 to S7 as loaded.  An instruction that raises a flag
//! shows the `F` line and then the exchange.

use cray_xmp_isa::Cpu;
use cray_xmp_model::{report, Event, Machine, Observer, RunResult};
use std::cell::Cell;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::rc::Rc;

const USAGE: &str =
    "usage: cray-xmp-run IMAGE [--machine CRAY1|XMP] [--max N] [--input TEXT] [--state OUT] [--trace OUT] [--quiet]";
const STATUS_USAGE: u8 = 64;
const STATUS_FILE: u8 = 66;

struct Options {
    image: String,
    max: u64,
    input: Vec<u8>,
    state: Option<String>,
    trace: Option<String>,
    quiet: bool,
    cpu: Cpu,
}

/// Decode the escapes of `--input`.
fn unescape(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut n = 0;
    while n < bytes.len() {
        if bytes[n] != b'\\' {
            out.push(bytes[n]);
            n += 1;
            continue;
        }
        let bad = || format!("bad escape in --input at `{}`", &text[n..]);
        match bytes.get(n + 1) {
            Some(b'n') => out.push(b'\n'),
            Some(b'r') => out.push(b'\r'),
            Some(b't') => out.push(b'\t'),
            Some(b'0') => out.push(0),
            Some(b'\\') => out.push(b'\\'),
            Some(b'x') => {
                let digits = text.get(n + 2..n + 4).ok_or_else(bad)?;
                out.push(u8::from_str_radix(digits, 16).map_err(|_| bad())?);
                n += 2;
            }
            _ => return Err(bad()),
        }
        n += 2;
    }
    Ok(out)
}

fn parse(argv: &[String]) -> Result<Options, String> {
    let mut o = Options {
        image: String::new(),
        max: 10_000_000,
        input: Vec::new(),
        state: None,
        trace: None,
        quiet: false,
        cpu: Cpu::Cray1,
    };
    let mut images = Vec::new();
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            match &inline {
                Some(v) => Ok(v.clone()),
                None => it
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("{} needs a value", name)),
            }
        };
        match name {
            "--max" => {
                let text = value(name)?;
                o.max = text
                    .parse()
                    .map_err(|_| format!("--max wants a decimal number, not `{}`", text))?;
            }
            "--input" => o.input = unescape(&value(name)?)?,
            "--state" => o.state = Some(value(name)?),
            "--trace" => o.trace = Some(value(name)?),
            "--quiet" => o.quiet = true,
            "--machine" => {
                let text = value(name)?;
                o.cpu = Cpu::from_name(&text)
                    .ok_or_else(|| format!("--machine `{}` is not known: CRAY1 or XMP", text))?;
            }
            _ if name.starts_with('-') && name != "-" => {
                return Err(format!("unknown option {}", name))
            }
            _ => images.push(arg.clone()),
        }
    }
    match images.as_slice() {
        [one] => o.image = one.clone(),
        [] => return Err("the image file is missing".to_string()),
        _ => return Err("more than one image file given".to_string()),
    }
    Ok(o)
}

/// Copies console output to standard output and writes the trace.
struct Sink {
    console: bool,
    trace: Option<BufWriter<File>>,
    failed: Rc<Cell<bool>>,
}

impl Observer for Sink {
    fn event(&mut self, event: &Event) {
        if let (true, Event::Console(byte)) = (self.console, event) {
            let mut out = std::io::stdout();
            let _ = out.write_all(&[*byte]);
            if *byte == b'\n' {
                let _ = out.flush();
            }
        }
        if let Some(trace) = self.trace.as_mut() {
            if writeln!(trace, "{}", report::trace_line(event)).is_err() {
                self.failed.set(true);
            }
        }
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        if let Some(trace) = self.trace.as_mut() {
            if trace.flush().is_err() {
                self.failed.set(true);
            }
        }
        let _ = std::io::stdout().flush();
    }
}

fn run(o: &Options) -> Result<u8, String> {
    let image = std::fs::read(&o.image).map_err(|e| format!("{}: {}", o.image, e))?;
    let mut machine = Machine::for_cpu(o.cpu);
    machine
        .load_image(&image)
        .map_err(|e| format!("{}: {}", o.image, e))?;
    machine.push_input(&o.input);
    // as in the simulation: each output channel cabled to its input channel
    machine.set_channel_loopback(o.cpu == Cpu::Xmp);
    let trace = match &o.trace {
        Some(path) => Some(BufWriter::new(
            File::create(path).map_err(|e| format!("{}: {}", path, e))?,
        )),
        None => None,
    };
    let failed = Rc::new(Cell::new(false));
    machine.set_observer(Some(Box::new(Sink {
        console: !o.quiet,
        trace,
        failed: failed.clone(),
    })));
    let result = machine.run(o.max);
    machine.set_observer(None);
    if failed.get() {
        return Err(format!(
            "{}: write failed",
            o.trace.as_deref().unwrap_or("trace")
        ));
    }
    if let Some(path) = &o.state {
        std::fs::write(path, report::state_text(&machine, &result))
            .map_err(|e| format!("{}: {}", path, e))?;
    }
    match &result {
        RunResult::Exit(code) if !o.quiet => {
            eprintln!(
                "cray-xmp-run: exit {} after {} instructions",
                code,
                machine.instructions()
            )
        }
        RunResult::Limit if !o.quiet => {
            eprintln!(
                "cray-xmp-run: no exit after {} steps, P={:08o}",
                machine.steps(),
                machine.p()
            )
        }
        RunResult::Error(e) => eprintln!("cray-xmp-run: {}", e),
        _ => {}
    }
    Ok(result.exit_status())
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&argv) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("cray-xmp-run: {}\n{}", e, USAGE);
            return ExitCode::from(STATUS_USAGE);
        }
    };
    match run(&options) {
        Ok(status) => ExitCode::from(status),
        Err(e) => {
            eprintln!("cray-xmp-run: {}", e);
            ExitCode::from(STATUS_FILE)
        }
    }
}

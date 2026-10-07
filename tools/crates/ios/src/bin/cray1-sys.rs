//! `cray1-sys`: run the I/O Subsystem software, and through it the
//! mainframe, on the reference models.
//!
//! ```text
//! cray1-sys SYSTEM [--script FILE] [--ms N] [--clock YYMMDD,HHMMSS] [--log FILE]
//!           [--poke PARCEL=VALUE]... [--two-iops] [--instant] [--timing NAME=N]...
//!           [--save DIR] [--quiet]
//! ```
//!
//! * `SYSTEM`: a directory with the software as the cray-sim project
//!   distributes it: `target/cos_117/iop_kern.bin` (the IOP kernel),
//!   `boot_tape.tap` (the overlays), `exp_disk.img` (the disk on the
//!   Peripheral Expander) and `biop_dkNN.img` (DD-29 drives, NN the BIOP
//!   channel in octal).  None of these files is ever written to.
//! * `--script FILE`: what the operator does, one step a line:
//!
//!   ```text
//!   wait CONSOLE TEXT    until CONSOLE shows TEXT (and, if it did already, has changed)
//!   wait printer TEXT    until the printer has printed TEXT, behind what the
//!                        wait printer before this one found
//!   type CONSOLE TEXT    type TEXT and RETURN
//!   run MS               let MS milliseconds pass
//!   screen CONSOLE       print CONSOLE as a screen
//!   printer              print what the printer has printed
//!   ```
//!
//!   `CONSOLE` is `kernel` (MIOP channels 46 and 47), `station` (MIOP
//!   channels 40 and 41) or `I.K`, console K (0 to 3) of IOP I.  A `wait`
//!   gives up after `--wait` seconds of machine time (default 60).
//! * `--ms N`: without a script, run N milliseconds (default 1000); with
//!   one, run N more when it ends.
//! * `--clock`: give the system a clock that reads this date and time.
//! * `--log FILE`: write every channel function and interrupt to FILE.
//! * `--poke`: change a parcel of the kernel (hexadecimal) before it runs.
//! * `--two-iops`: leave out the XIOP.
//! * `--without IOP.CHANNEL`: leave a channel (octal) with nothing on it.
//! * `--instant`: every channel operation takes no time, as in the
//!   cray-sim simulator.
//! * `--replay IOP,STEPS,FILE`: record the first STEPS steps (or more, to
//!   the end of a millisecond) of an I/O Processor in FILE, for checking
//!   the hardware description of the processor (`make -C sim iop`).
//! * `--timing NAME=N`: one of the times of `Timing` (`system.rs`), in
//!   clock periods of 12.5 ns: `disk_sector=400000` is a disk that takes
//!   5 ms for a sector.
//! * `--save DIR`: at the end write the disks that were written to into
//!   DIR, under their names in `SYSTEM`: each is its file with what the
//!   software wrote.  That is how a system is installed on empty drives and
//!   kept.  DIR must not be `SYSTEM`.
//! * `--quiet`: do not copy the kernel console to standard output.
//!
//! The exit status is 0 if the script ran to its end, 1 if a wait gave up
//! or the CPU model stopped, 64 for a bad command line, 66 for a file that
//! could not be read.

use cray1_ios::{
    Config, Image, Screen, System, Tape, Timing, DD29_SECTOR_BYTES, DISK_SECTOR_BYTES,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Clock periods of 12.5 ns in a millisecond.
const MILLISECOND: u64 = 80_000;

const USAGE: &str = "usage: cray1-sys SYSTEM [--script FILE] [--ms N] [--wait SECONDS] [--clock YYMMDD,HHMMSS] [--log FILE] [--poke PARCEL=VALUE]... [--two-iops] [--instant] [--timing NAME=N]... [--save DIR] [--quiet]";

struct Options {
    system: PathBuf,
    script: Option<String>,
    ms: Option<u64>,
    wait: u64,
    clock: Option<(String, String)>,
    log: Option<String>,
    pokes: Vec<(usize, u16)>,
    two_iops: bool,
    instant: bool,
    timing: Vec<(String, u32)>,
    replay: Option<(usize, String, u64)>,
    without: Vec<(usize, u8)>,
    save: Option<PathBuf>,
    quiet: bool,
}

fn options() -> Result<Options, String> {
    let mut o = Options {
        system: PathBuf::new(),
        script: None,
        ms: None,
        wait: 60,
        clock: None,
        log: None,
        pokes: Vec::new(),
        two_iops: false,
        instant: false,
        timing: Vec::new(),
        replay: None,
        without: Vec::new(),
        save: None,
        quiet: false,
    };
    let mut system = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{} needs a value", name));
        match arg.as_str() {
            "--script" => o.script = Some(value("--script")?),
            "--ms" => o.ms = Some(value("--ms")?.parse().map_err(|_| "--ms takes a number")?),
            "--wait" => {
                o.wait = value("--wait")?
                    .parse()
                    .map_err(|_| "--wait takes a number")?
            }
            "--clock" => {
                let text = value("--clock")?;
                let (date, time) = text.split_once(',').ok_or("--clock takes YYMMDD,HHMMSS")?;
                if date.len() != 6
                    || time.len() != 6
                    || !text.bytes().all(|b| b.is_ascii_digit() || b == b',')
                {
                    return Err("--clock takes YYMMDD,HHMMSS".into());
                }
                o.clock = Some((date.to_string(), time.to_string()));
            }
            "--log" => o.log = Some(value("--log")?),
            "--poke" => {
                let text = value("--poke")?;
                let parsed = text.split_once('=').and_then(|(parcel, v)| {
                    Some((
                        usize::from_str_radix(parcel, 16).ok()?,
                        u16::from_str_radix(v, 16).ok()?,
                    ))
                });
                o.pokes
                    .push(parsed.ok_or("--poke takes PARCEL=VALUE in hexadecimal")?);
            }
            "--two-iops" => o.two_iops = true,
            "--instant" => o.instant = true,
            "--without" => {
                let text = value("--without")?;
                let parsed = text.split_once('.').and_then(|(iop, channel)| {
                    let iop = iop.parse().ok().filter(|&n: &usize| n < 4)?;
                    Some((
                        iop,
                        u8::from_str_radix(channel, 8).ok().filter(|&c| c < 0o50)?,
                    ))
                });
                o.without
                    .push(parsed.ok_or("--without takes IOP.CHANNEL, the channel in octal")?);
            }
            "--replay" => {
                let text = value("--replay")?;
                let mut parts = text.splitn(3, ',');
                let parsed = (|| {
                    let iop = parts.next()?.parse().ok().filter(|&n: &usize| n < 4)?;
                    let steps = parts.next()?.parse().ok()?;
                    Some((iop, parts.next()?.to_string(), steps))
                })();
                o.replay = Some(parsed.ok_or("--replay takes IOP,STEPS,FILE")?);
            }
            "--timing" => {
                let text = value("--timing")?;
                let parsed = text
                    .split_once('=')
                    .and_then(|(name, v)| Some((name.to_string(), v.parse().ok()?)));
                o.timing
                    .push(parsed.ok_or("--timing takes NAME=CLOCK-PERIODS")?);
            }
            "--save" => o.save = Some(PathBuf::from(value("--save")?)),
            "--quiet" => o.quiet = true,
            _ if arg.starts_with("--") => return Err(format!("{} is not an option", arg)),
            _ if system.is_none() => system = Some(PathBuf::from(arg)),
            _ => return Err("one SYSTEM directory".into()),
        }
    }
    o.system = system.ok_or("no SYSTEM directory")?;
    Ok(o)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))
}

/// The BIOP channels that have a drive: those with an image in `system`.
fn drives(system: &Path) -> Vec<u8> {
    (0o20..0o40u8)
        .filter(|channel| system.join(format!("biop_dk{:o}.img", channel)).exists())
        .collect()
}

/// Write the disks that were written to into `dir`.
fn save(system: &System, o: &Options, dir: &Path) -> Result<(), String> {
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    if same(dir, &o.system) {
        return Err("--save needs a directory other than SYSTEM".into());
    }
    let (expander, disks) = system.written();
    if expander > 0 {
        let to = dir.join("exp_disk.img");
        system
            .save_expander_disk(&to)
            .map_err(|e| format!("{}: {}", to.display(), e))?;
        eprintln!("  saved {}", to.display());
    }
    for (index, channel) in drives(&o.system).into_iter().enumerate() {
        if disks[index] > 0 {
            let to = dir.join(format!("biop_dk{:o}.img", channel));
            system
                .save_disk(index, &to)
                .map_err(|e| format!("{}: {}", to.display(), e))?;
            eprintln!("  saved {}", to.display());
        }
    }
    Ok(())
}

fn build(o: &Options) -> Result<System, String> {
    let kernel = read(&o.system.join("target/cos_117/iop_kern.bin"))?;
    let tape = Tape::from_tap(&read(&o.system.join("boot_tape.tap"))?)?;
    let disk = o.system.join("exp_disk.img");
    let disk =
        Image::open(&disk, DISK_SECTOR_BYTES).map_err(|e| format!("{}: {}", disk.display(), e))?;
    let mut config = Config::new(kernel, tape, disk);
    for channel in drives(&o.system) {
        let path = o.system.join(format!("biop_dk{:o}.img", channel));
        let image = Image::open(&path, DD29_SECTOR_BYTES)
            .map_err(|e| format!("{}: {}", path.display(), e))?;
        config.disks.push((channel, image));
    }
    if let Some((date, time)) = &o.clock {
        if !config.with_clock(date, time) {
            return Err("the tape is too short to hold the clock overlay".into());
        }
    }
    for &(parcel, value) in &o.pokes {
        if 2 * parcel + 2 > config.kernel.len() {
            return Err(format!("the kernel has no parcel {:x}", parcel));
        }
        config.poke_kernel(parcel, value);
    }
    config.iops[3] = !o.two_iops;
    config.without = o.without.clone();
    if o.instant {
        config.timing = Timing::instant();
    }
    for (name, value) in &o.timing {
        if !config.timing.set(name, *value) {
            return Err(format!(
                "`{}` is not a time: see Timing in tools/crates/ios/src/system.rs",
                name
            ));
        }
    }
    let mut system = System::new(config);
    if let Some((iop, path, _)) = &o.replay {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {}", path, e))?;
        system.set_replay(
            *iop,
            Box::new(std::io::BufWriter::with_capacity(1 << 20, file)),
        );
    }
    if let Some(path) = &o.log {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {}", path, e))?;
        system.set_log(Some(Box::new(std::io::BufWriter::new(file))));
    }
    Ok(system)
}

/// A console by name: the IOP and its console number.
fn console(name: &str) -> Result<(usize, usize), String> {
    match name {
        "kernel" => Ok((0, 3)),
        "station" => Ok((0, 0)),
        _ => name
            .split_once('.')
            .and_then(|(i, k)| Some((i.parse().ok()?, k.parse().ok()?)))
            .filter(|&(i, k): &(usize, usize)| i < 4 && k < 4)
            .ok_or(format!(
                "`{}` is not a console: kernel, station or IOP.NUMBER",
                name
            )),
    }
}

/// Control characters made visible.
fn printable(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            b'\n' => "\n".to_string(),
            b'\r' | 0 => String::new(),
            0x20..=0x7E => (b as char).to_string(),
            _ => format!("<{:02x}>", b),
        })
        .collect()
}

struct Runner {
    system: System,
    /// The number of steps after which the record of an IOP ends.
    replay_steps: Option<u64>,
    /// How much of the kernel console has been copied out.
    shown: usize,
    /// How much of the printout, as `printable` gives it, the waits for the
    /// printer have found their texts in.
    heard: usize,
    quiet: bool,
}

impl Runner {
    /// Let a millisecond pass.
    fn tick(&mut self) {
        self.system.run(MILLISECOND);
        if self
            .replay_steps
            .is_some_and(|steps| self.system.replay_steps() >= steps)
        {
            self.replay_steps = None;
            eprintln!("cray1-sys: {} steps recorded", self.system.end_replay());
        }
        if !self.quiet {
            let output = self.system.console(0, 3);
            if output.len() > self.shown {
                print!("{}", printable(&output[self.shown..]));
                let _ = std::io::stdout().flush();
                self.shown = output.len();
            }
        }
    }

    fn report(&self) {
        let s = &self.system;
        eprintln!(
            "\ncray1-sys: {:.3} s of machine time",
            s.time() as f64 / 8e7
        );
        for n in 0..4 {
            let c = s.iop(n).counters();
            if c.instructions != 0 {
                eprintln!(
                    "  IOP {}: P {:04x}, {} instructions, {} interrupts{}",
                    n,
                    s.iop(n).p(),
                    c.instructions,
                    c.interrupts,
                    if s.iop(n).held() {
                        ", held by Master Clear"
                    } else {
                        ""
                    }
                );
            }
        }
        eprintln!(
            "  CPU: P {:o}, {} instructions, started {} times{}",
            s.cpu().p(),
            s.cpu().instructions(),
            s.cpu_starts(),
            if s.cpu_held() { ", held" } else { "" }
        );
        if let Some(why) = s.cpu_stopped() {
            eprintln!("  the CPU model stopped: {}", why);
        }
        let (expander, disks) = s.written();
        eprintln!(
            "  sectors written: expander disk {}, drives {:?}",
            expander, disks
        );
    }

    fn script(&mut self, text: &str, wait: u64) -> Result<(), String> {
        // what each console showed when a step last looked at it
        let mut before: [[String; 4]; 4] = Default::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fail = |why: String| format!("script line {}: {}", number + 1, why);
            let (step, rest) = line.split_once(' ').unwrap_or((line, ""));
            let (name, text) = rest.split_once(' ').unwrap_or((rest, ""));
            match step {
                "run" => {
                    for _ in 0..rest
                        .parse::<u64>()
                        .map_err(|_| fail("run takes milliseconds".into()))?
                    {
                        self.tick();
                    }
                }
                // a console is a screen: the text has to be on it, and if
                // it was there already the screen has to have changed
                "wait" if name == "printer" => {
                    let mut left = wait * 1000;
                    // the printout is looked at again only when it has grown
                    let mut length = usize::MAX;
                    loop {
                        if self.system.printed().len() != length {
                            length = self.system.printed().len();
                            let printed = printable(self.system.printed());
                            if let Some(at) = printed[self.heard..].find(text) {
                                self.heard += at + text.len();
                                break;
                            }
                        }
                        if left == 0 {
                            return Err(fail(format!(
                                "the printer did not print `{}` in {} s",
                                text, wait
                            )));
                        }
                        left -= 1;
                        self.tick();
                    }
                }
                "wait" => {
                    let (i, k) = console(name).map_err(fail)?;
                    let mut left = wait * 1000;
                    loop {
                        let screen = Screen::of(self.system.console(i, k)).text();
                        if screen.contains(text)
                            && (screen != before[i][k] || !before[i][k].contains(text))
                        {
                            before[i][k] = screen;
                            break;
                        }
                        if let Some(why) = self.system.cpu_stopped() {
                            return Err(fail(format!("the CPU model stopped: {}", why)));
                        }
                        if left == 0 {
                            return Err(fail(format!(
                                "`{}` did not appear on {} in {} s",
                                text, name, wait
                            )));
                        }
                        left -= 1;
                        self.tick();
                    }
                }
                "type" => {
                    let (i, k) = console(name).map_err(fail)?;
                    before[i][k] = Screen::of(self.system.console(i, k)).text();
                    let mut keys = text.as_bytes().to_vec();
                    keys.push(b'\r');
                    self.system.type_text(i, k, &keys);
                    let mut left = wait * 1000;
                    while !self.system.typed(i, k) {
                        if left == 0 {
                            return Err(fail(format!(
                                "{} did not take `{}` in {} s",
                                name, text, wait
                            )));
                        }
                        left -= 1;
                        self.tick();
                    }
                }
                "screen" => {
                    let (i, k) = console(name).map_err(fail)?;
                    println!(
                        "\n---- {} at {:.3} s",
                        name,
                        self.system.time() as f64 / 8e7
                    );
                    println!("{}", Screen::of(self.system.console(i, k)).text());
                    println!("----");
                }
                "printer" => {
                    println!("\n---- printer at {:.3} s", self.system.time() as f64 / 8e7);
                    println!("{}", printable(self.system.printed()));
                    println!("----");
                }
                _ => return Err(fail(format!("`{}` is not a step", step))),
            }
        }
        Ok(())
    }
}

fn main() -> ExitCode {
    let o = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("cray1-sys: {}\n{}", e, USAGE);
            return ExitCode::from(64);
        }
    };
    let script = match o
        .script
        .as_ref()
        .map(|path| read(Path::new(path)))
        .transpose()
    {
        Ok(script) => script.map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
        Err(e) => {
            eprintln!("cray1-sys: {}", e);
            return ExitCode::from(66);
        }
    };
    let system = match build(&o) {
        Ok(system) => system,
        Err(e) => {
            eprintln!("cray1-sys: {}", e);
            return ExitCode::from(66);
        }
    };
    let mut runner = Runner {
        system,
        replay_steps: o.replay.as_ref().map(|r| r.2),
        shown: 0,
        heard: 0,
        quiet: o.quiet,
    };
    let mut status = 0;
    if let Some(script) = &script {
        if let Err(e) = runner.script(script, o.wait) {
            eprintln!("\ncray1-sys: {}", e);
            status = 1;
        }
    }
    for _ in 0..o.ms.unwrap_or(if script.is_some() { 0 } else { 1000 }) {
        runner.tick();
    }
    runner.system.set_log(None);
    runner.system.end_replay();
    runner.report();
    if runner.system.cpu_stopped().is_some() {
        status = 1;
    }
    if let Some(dir) = &o.save {
        if let Err(e) = save(&runner.system, &o, dir) {
            eprintln!("cray1-sys: {}", e);
            return ExitCode::from(66);
        }
    }
    ExitCode::from(status)
}

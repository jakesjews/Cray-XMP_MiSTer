//! The real IOS kernel on the system model.
//!
//! These tests need the COS 1.17 system as the cray-sim project distributes
//! it, in the directory named by the environment variable `CRAY_XMP_SYSTEM`.
//! Without it they pass without doing anything.  One of them also needs a
//! boot of that system recorded on the cray-sim simulator, in the file named
//! by `CRAY_XMP_BOOT_LOG`.

use cray_xmp_ios::{Config, Image, System, Tape, Timing, DISK_SECTOR_BYTES};
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

/// Clock periods in a millisecond.
const MILLISECOND: u64 = 80_000;

fn config() -> Option<Config> {
    let dir = PathBuf::from(std::env::var_os("CRAY_XMP_SYSTEM")?);
    let kernel = std::fs::read(dir.join("target/cos_117/iop_kern.bin")).ok()?;
    let tape = Tape::from_tap(&std::fs::read(dir.join("boot_tape.tap")).ok()?).ok()?;
    let disk = Image::open(&dir.join("exp_disk.img"), DISK_SECTOR_BYTES).ok()?;
    Some(Config::new(kernel, tape, disk))
}

/// A log that the test can read while the system writes it.
#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<u8>>>);

impl Write for Log {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The channel functions of the MIOP in a function log: P, channel,
/// function, A and the value read.  The recorded log has a line `N` for a
/// function to a channel with nothing on it.
fn miop_functions(log: &str) -> Vec<String> {
    log.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(' ').collect();
            match f[..] {
                ["F", "0", p, channel, function, a, value, _] => {
                    Some(format!("{p} {channel} {function} {a} {value}"))
                }
                ["N", "0", p, channel, function, a] => {
                    Some(format!("{p} {channel} {function} {a} 0000"))
                }
                _ => None,
            }
        })
        .collect()
}

/// From dead start to the first interrupt of its real-time clock the MIOP
/// issues the same channel functions, with the same values, as in the boot
/// recorded on the cray-sim simulator: it clears its exit stack and its
/// channels, loads 152 records of overlays from tape into Buffer Memory and
/// dead starts the two other processors.  The configuration is that
/// simulator's: its patches to the kernel, a clock, and devices that take
/// no time.
#[test]
fn the_start_of_the_kernel_matches_the_recorded_boot() {
    let recorded = PathBuf::from(std::env::var_os("CRAY_XMP_BOOT_LOG").unwrap_or_default());
    let (Some(mut config), true) = (config(), recorded.is_file()) else {
        eprintln!("skipped: the COS 1.17 system or the recorded boot is not here");
        return;
    };
    let Ok(unpacked) = std::process::Command::new("gzip")
        .arg("-dc")
        .arg(&recorded)
        .output()
    else {
        eprintln!("skipped: no gzip to unpack the recorded boot with");
        return;
    };
    let recorded = miop_functions(&String::from_utf8_lossy(&unpacked.stdout));
    // the memory test returns at once, the Buffer Memory test is short and
    // memory is not cleared before the overlays are loaded
    for (parcel, value) in [(0x42B7, 0x0200), (0x43DA, 0), (0x4476, 0)] {
        config.poke_kernel(parcel, value);
    }
    assert!(config.with_clock("890606", "123538"));
    config.timing = Timing::instant();
    let mut system = System::new(config);
    let log = Log::default();
    system.set_log(Some(Box::new(log.clone())));
    // the first clock interrupt comes after 4,618 functions
    const COMPARED: usize = 4_600;
    let mut functions = Vec::new();
    for _ in 0..4_000 {
        system.run(MILLISECOND);
        functions = miop_functions(&String::from_utf8_lossy(&log.0.borrow()));
        if functions.len() >= COMPARED {
            break;
        }
    }
    assert!(
        functions.len() >= COMPARED,
        "only {} functions in 4 s",
        functions.len()
    );
    for n in 0..COMPARED {
        assert_eq!(
            functions[n],
            recorded[n],
            "function {} (P, channel, function, A, value)",
            n + 1
        );
    }
}

/// The kernel as it is, on devices that take their time: it tests its
/// memory and Buffer Memory, loads the overlays, starts the BIOP and the
/// XIOP and asks the operator for the date.
#[test]
fn the_kernel_boots_to_its_first_question() {
    let Some(mut config) = config() else {
        eprintln!("skipped: the COS 1.17 system is not here");
        return;
    };
    // an unoptimised build runs the tests of memory too slowly: leave out
    // the long ones there
    if cfg!(debug_assertions) {
        config.poke_kernel(0x42B7, 0x0200);
        config.poke_kernel(0x43DA, 0);
    }
    let mut system = System::new(config);
    let shows =
        |system: &System, text: &str| String::from_utf8_lossy(system.console(0, 3)).contains(text);
    for _ in 0..15_000 {
        if shows(&system, "ENTER DATE") {
            break;
        }
        system.run(MILLISECOND);
    }
    let console = String::from_utf8_lossy(system.console(0, 3)).into_owned();
    for text in [
        "MOS TEST COMPLETE",
        "IOP-0 KERNEL, VERSION 4.2.2",
        "IOP1 A->A",
        "IOP3 A->A",
        "MOS SIZE  100K",
        "ENTER DATE [MM/DD/YY]",
    ] {
        assert!(
            console.contains(text),
            "`{}` is not on the kernel console:\n{}",
            text,
            console
        );
    }
    for n in [0, 1, 3] {
        let (iop, counters) = (system.iop(n), system.iop(n).counters());
        assert!(!iop.held(), "IOP {} never started", n);
        assert!(
            iop.interrupt_enable() || iop.interrupt_enable_delayed() || counters.interrupts > 100,
            "IOP {}",
            n
        );
        // the kernel has no handler for the exit stack boundary and never
        // looks at the program fetch request channel
        assert_eq!(
            (counters.boundary_flags, counters.pfr_channel),
            (0, 0),
            "IOP {}",
            n
        );
    }
    assert!(system.iop(2).held());
    // the BIOP and the XIOP have a console of their own and say who they
    // are; the BIOP finds no drive on its disk channels
    let biop = String::from_utf8_lossy(system.console(1, 1)).into_owned();
    assert!(
        biop.contains("IOP-1 KERNEL, VERSION 4.2.2")
            && biop.contains("CHANNEL 20 DEVICE SELECT ERROR")
    );
    assert!(String::from_utf8_lossy(system.console(3, 1)).contains("IOP-3 KERNEL, VERSION 4.2.2"));
    assert!(system.cpu_held() && system.cpu_starts() == 0);
}

//! Write the record of a random program on the I/O Processor model, for
//! `sim/build/iop/Viop_cpu` to check the hardware description against.
//!
//! ```text
//! iop_random SEED STEPS noise|work FILE
//! iop_random directed FILE
//! ```
//!
//! The second form writes the record of a short program for what random
//! programs seldom reach.

use cray1_ios::replay::{directed, random, Mix};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [kind, file] = &args[..] {
        if kind == "directed" {
            return match std::fs::File::create(file) {
                Ok(out) => {
                    directed(Box::new(std::io::BufWriter::new(out)));
                    0.into()
                }
                Err(e) => {
                    eprintln!("iop_random: {}: {}", file, e);
                    66.into()
                }
            };
        }
    }
    let parsed = (|| {
        let [seed, steps, mix, file] = &args[..] else {
            return None;
        };
        let mix = match mix.as_str() {
            "noise" => Mix::Noise,
            "work" => Mix::Work,
            _ => return None,
        };
        Some((seed.parse().ok()?, steps.parse().ok()?, mix, file))
    })();
    let Some((seed, steps, mix, file)) = parsed else {
        eprintln!("usage: iop_random SEED STEPS noise|work FILE\n       iop_random directed FILE");
        return 64.into();
    };
    match std::fs::File::create(file) {
        Ok(out) => {
            random(seed, steps, mix, Box::new(std::io::BufWriter::new(out)));
            0.into()
        }
        Err(e) => {
            eprintln!("iop_random: {}: {}", file, e);
            66.into()
        }
    }
}

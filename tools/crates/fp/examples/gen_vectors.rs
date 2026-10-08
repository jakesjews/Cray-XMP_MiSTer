//! Write one vector file per operation.
//!
//! ```text
//! cargo run -p cray-xmp-fp --example gen_vectors -- OUT_DIR [COUNT] [SEED]
//! ```
//!
//! Writes `OUT_DIR/<op>.vec` for add, sub, mul, mulh, mulr, mul2m and recip, each with COUNT
//! vectors (default 2000; the corner cases come first) from SEED (default 1).

use std::path::PathBuf;
use std::process::ExitCode;

use cray_xmp_fp::{write_vector_file, Op};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first() else {
        eprintln!("usage: gen_vectors OUT_DIR [COUNT] [SEED]");
        return ExitCode::FAILURE;
    };
    let count = match args.get(1).map(|s| s.parse::<usize>()) {
        None => 2000,
        Some(Ok(n)) => n,
        Some(Err(e)) => {
            eprintln!("bad COUNT: {e}");
            return ExitCode::FAILURE;
        }
    };
    let seed = match args.get(2).map(|s| s.parse::<u64>()) {
        None => 1,
        Some(Ok(n)) => n,
        Some(Err(e)) => {
            eprintln!("bad SEED: {e}");
            return ExitCode::FAILURE;
        }
    };
    let dir = PathBuf::from(dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    for op in Op::ALL {
        let path = dir.join(format!("{}.vec", op.name()));
        match write_vector_file(&path, &[op], count, seed) {
            Ok(n) => println!("{}: {n} vectors", path.display()),
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

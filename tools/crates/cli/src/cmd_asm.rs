//! `cray-xmp asm`: assemble a CAL source file.

use crate::args::Args;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const DETAILS: &str = "
Assembles the CAL-subset source SRC into a raw memory image: big-endian
64-bit words from word 0 to the highest word used.

  -o OUT.img   image file (default: SRC with the extension .img)
  -l OUT.lst   listing: address, octal parcels, source line, symbols
  -s OUT.sym   symbol file: name, octal value, attribute (V value,
               W word address, P parcel address, X external)
  -I DIR       also look for INCLUDE files in DIR (may be repeated)

Errors and warnings go to standard error as FILE:LINE: message.
Exit status: 0 clean; 1 errors, no image or symbol file written;
2 warnings only, files written.
";

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {}", path.display(), e))
}

pub fn run(argv: &[String]) -> Result<ExitCode, String> {
    let args = Args::parse(argv, &["-o", "-l", "-s", "-I"])?;
    let source = PathBuf::from(args.one_positional("the source file")?);
    let include_dirs: Vec<PathBuf> = args.values("-I").into_iter().map(PathBuf::from).collect();
    let assembly = cray_xmp_asm::assemble_file(&source, &include_dirs)?;

    for d in &assembly.diagnostics {
        eprintln!("{}", d);
    }
    // the listing is the place to look at errors, so it is written either way
    if let Some(listing) = args.value("-l") {
        write(Path::new(listing), assembly.listing_text().as_bytes())?;
    }
    if assembly.has_errors() {
        let errors = assembly
            .diagnostics
            .iter()
            .filter(|d| d.severity == cray_xmp_asm::Severity::Error)
            .count();
        eprintln!("{}: {} errors, no image written", source.display(), errors);
        return Ok(ExitCode::from(1));
    }
    let image = args
        .value("-o")
        .map(PathBuf::from)
        .unwrap_or_else(|| source.with_extension("img"));
    write(&image, &assembly.image())?;
    if let Some(symbols) = args.value("-s") {
        write(Path::new(symbols), assembly.symbols_text().as_bytes())?;
    }
    println!("{}: {} words", image.display(), assembly.words.len());
    Ok(if assembly.has_warnings() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

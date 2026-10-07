//! The `cray-xmp` binary end to end.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cray_xmp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cray-xmp"))
        .args(args)
        .output()
        .expect("run cray-xmp")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A fresh directory under the target directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/asm")
        .join(name)
        .display()
        .to_string()
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).display().to_string()
}

#[test]
fn asm_writes_image_listing_and_symbols() {
    let dir = scratch("boot");
    let (img, lst, sym) = (
        path(&dir, "boot.img"),
        path(&dir, "boot.lst"),
        path(&dir, "boot.sym"),
    );
    let o = cray_xmp(&[
        "asm",
        &fixture("boot.cal"),
        "-o",
        &img,
        "-l",
        &lst,
        "-s",
        &sym,
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(stderr(&o), "");
    assert_eq!(stdout(&o), format!("{}: 33 words\n", img));
    let image = std::fs::read(&img).unwrap();
    assert_eq!(image.len(), 33 * 8);
    assert_eq!(&image[..8], [0, 0, 0, 0, 0x40, 0, 0, 0]);
    assert_eq!(
        &image[128..136],
        [0xa0, 0x08, 0x00, 0x01, 0x18, 0x00, 0x00, 0x40]
    );
    let listing = std::fs::read_to_string(&lst).unwrap();
    assert!(
        listing.contains("0000020a  120010 000001             30  RX_SPIN  S0        RX_STAT,A0"),
        "{}",
        listing
    );
    assert!(listing.contains("0 errors, 0 warnings"));
    let symbols = std::fs::read_to_string(&sym).unwrap();
    assert!(
        symbols.contains("RX_SPIN               100 P\n"),
        "{}",
        symbols
    );
    assert!(symbols.contains("PBASE                1000 V\n"));

    // disassemble a slice of it
    let o = cray_xmp(&["dis", &img, "--start", "0o20", "--words=2"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        stdout(&o),
        "\
0000020a  120010 000001  S0        2000001,0
0000020c  014000 000100  JSZ       100
0000021a  040100 000033  S1        33
0000021c  120210 000002  S2        2000002,0
"
    );
    let all = stdout(&cray_xmp(&["dis", &img]));
    assert!(all.contains("0000001   15 zero words\n"));
    assert!(all.contains("0000022d  040100 000034  S1        34               ; straddles words\n"));
    assert!(all.ends_with("0000040a  006000 000100  J         100\n0000040c  000000         ERR\n0000040d  000000         ERR\n"));
}

#[test]
fn asm_default_output_and_include_directory() {
    let dir = scratch("include");
    std::fs::create_dir_all(dir.join("inc")).unwrap();
    std::fs::write(dir.join("inc/defs.cal"), "SEVEN    =         7\n").unwrap();
    std::fs::write(
        dir.join("near.cal"),
        "         MACRO     HALT\n         EX\n         ENDM\n",
    )
    .unwrap();
    std::fs::write(dir.join("prog.cal"), "         INCLUDE   \"defs.cal\"\n         INCLUDE   near.cal\n         A1        SEVEN\n         HALT\n").unwrap();
    let o = cray_xmp(&["asm", &path(&dir, "prog.cal"), "-I", &path(&dir, "inc")]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    // the image lands next to the source
    assert_eq!(
        std::fs::read(dir.join("prog.img")).unwrap(),
        [0x24, 0x47, 0x08, 0x00, 0, 0, 0, 0]
    );
    // without -I the first include is not found
    std::fs::remove_file(dir.join("prog.img")).unwrap();
    let o = cray_xmp(&["asm", &path(&dir, "prog.cal")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("prog.cal:1: error: cannot include \"defs.cal\": file not found"),
        "{}",
        stderr(&o)
    );
    assert!(!dir.join("prog.img").exists());
}

#[test]
fn asm_errors_exit_1_without_an_image() {
    let dir = scratch("errors");
    let src = path(&dir, "bad.cal");
    std::fs::write(
        &src,
        "START    A1        NOWHERE\n         FROB      S1\n         EX\n",
    )
    .unwrap();
    let (img, lst, sym) = (
        path(&dir, "bad.img"),
        path(&dir, "bad.lst"),
        path(&dir, "bad.sym"),
    );
    let o = cray_xmp(&["asm", &src, "-o", &img, "-l", &lst, "-s", &sym]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(stdout(&o), "");
    let err = stderr(&o);
    assert!(
        err.contains(&format!("{}:1: error: undefined symbol NOWHERE\n", src)),
        "{}",
        err
    );
    assert!(
        err.contains(&format!(
            "{}:2: error: no instruction form matches `FROB S1`\n",
            src
        )),
        "{}",
        err
    );
    assert!(err.contains("2 errors, no image written"));
    assert!(!Path::new(&img).exists());
    assert!(!Path::new(&sym).exists());
    // the listing is still written, with the messages in place
    let listing = std::fs::read_to_string(&lst).unwrap();
    assert!(listing.contains("***** error: undefined symbol NOWHERE"));
}

#[test]
fn asm_warnings_exit_2_with_an_image() {
    let dir = scratch("warnings");
    let src = path(&dir, "warn.cal");
    std::fs::write(
        &src,
        "         PASS\nODD      PASS\n         S1        ODD,0\n",
    )
    .unwrap();
    let img = path(&dir, "warn.img");
    let o = cray_xmp(&["asm", &src, "-o", &img]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        stderr(&o).contains(&format!(
            "{}:3: warning: `ODD` is a parcel address that is not on a word boundary",
            src
        )),
        "{}",
        stderr(&o)
    );
    assert_eq!(std::fs::read(&img).unwrap().len(), 8);
}

#[test]
fn dis_notes_an_instruction_that_straddles_words() {
    let dir = scratch("straddle");
    let src = path(&dir, "straddle.cal");
    std::fs::write(
        &src,
        "         PASS\n         PASS\n         PASS\n         J         1234567\n",
    )
    .unwrap();
    let img = path(&dir, "straddle.img");
    let o = cray_xmp(&["asm", &src, "-o", &img]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let text = stdout(&cray_xmp(&["dis", &img]));
    assert_eq!(text.matches("; straddles words").count(), 1, "{}", text);
}

#[test]
fn dis_output_assembles_back_to_the_same_image() {
    let dir = scratch("roundtrip");
    let img = path(&dir, "all.img");
    let o = cray_xmp(&["asm", &fixture("all_forms.cal"), "-o", &img]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let text = stdout(&cray_xmp(&["dis", &img]));
    assert!(
        !text.contains("ignored bits"),
        "the fixture is canonical apart from ERR exp and EX exp"
    );
    // keep the CAL column: after the address and parcels, before any note
    let mut source = String::new();
    for line in text.lines() {
        let cal = line[25..].split(';').next().unwrap().trim_end();
        source.push_str(&format!("         {}\n", cal));
    }
    let again = path(&dir, "again.cal");
    std::fs::write(&again, source).unwrap();
    let o = cray_xmp(&["asm", &again]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(
        std::fs::read(dir.join("again.img")).unwrap(),
        std::fs::read(&img).unwrap()
    );
}

#[test]
fn isa_prints_the_table_and_bad_usage_is_reported() {
    let o = cray_xmp(&["isa"]);
    assert_eq!(o.status.code(), Some(0));
    let table = stdout(&o);
    assert!(table.contains("  030ijk   Ai        Aj+Ak      1   A Int Add  -     Aj,Ak        Ai     Integer sum of (Aj) and (Ak) to Ai\n"));
    assert!(table.contains("  177xjk   ,A0,Ak    Vj         1   Memory     WV"));

    let o = cray_xmp(&[]);
    assert_eq!(o.status.code(), Some(64));
    assert!(stderr(&o).contains("cray-xmp asm SRC [-o OUT.img] [-l OUT.lst] [-s OUT.sym] [-I DIR]"));
    assert_eq!(cray_xmp(&["frobnicate"]).status.code(), Some(64));
    assert_eq!(cray_xmp(&["asm"]).status.code(), Some(64));
    assert_eq!(
        cray_xmp(&["asm", "a.cal", "--bogus"]).status.code(),
        Some(64)
    );
    assert_eq!(
        cray_xmp(&["dis", "x.img", "--start", "abc"]).status.code(),
        Some(1)
    );
    assert_eq!(
        cray_xmp(&["asm", "/nonexistent/file.cal"]).status.code(),
        Some(1)
    );
    assert_eq!(cray_xmp(&["help"]).status.code(), Some(0));
    assert!(stdout(&cray_xmp(&["dis", "--help"])).contains("--start WORD"));
}

//! The `cray-xmp` command: host tools for the CRAY X-MP core.
//!
//! Each subcommand lives in its own module and is listed in `COMMANDS`.  To
//! add one (`run`, `gen`, `fpvec`...), write a module with
//! `pub fn run(args: &[String]) -> Result<ExitCode, String>` and add a row.

mod args;
mod cmd_asm;
mod cmd_dis;
mod cmd_isa;

use std::process::ExitCode;

/// Exit status for a command line that could not be understood.
const EXIT_USAGE: u8 = 64;

struct Command {
    name: &'static str,
    /// Arguments, as shown in the usage text.
    synopsis: &'static str,
    about: &'static str,
    /// More help, shown by `cray-xmp NAME --help`.
    details: &'static str,
    run: fn(&[String]) -> Result<ExitCode, String>,
}

const COMMANDS: &[Command] = &[
    Command {
        name: "asm",
        synopsis: "SRC [-o OUT.img] [-l OUT.lst] [-s OUT.sym] [-I DIR]...",
        about: "assemble CAL source into a memory image",
        details: cmd_asm::DETAILS,
        run: cmd_asm::run,
    },
    Command {
        name: "dis",
        synopsis: "IMG [--start WORD] [--words N]",
        about: "disassemble a memory image parcel by parcel",
        details: cmd_dis::DETAILS,
        run: cmd_dis::run,
    },
    Command {
        name: "isa",
        synopsis: "",
        about: "print the instruction table",
        details: cmd_isa::DETAILS,
        run: cmd_isa::run,
    },
];

fn usage() -> String {
    let mut text = String::from("Host tools for the CRAY X-MP core.\n\nUsage:\n");
    for c in COMMANDS {
        text.push_str(format!("  cray-xmp {} {}", c.name, c.synopsis).trim_end());
        text.push('\n');
    }
    text.push_str("\nCommands:\n");
    for c in COMMANDS {
        text.push_str(&format!("  {:<6}{}\n", c.name, c.about));
    }
    text.push_str("\n`cray-xmp COMMAND --help` says more about one command.\n");
    text
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = argv.first() else {
        eprint!("{}", usage());
        return ExitCode::from(EXIT_USAGE);
    };
    if matches!(name.as_str(), "help" | "--help" | "-h") {
        print!("{}", usage());
        return ExitCode::SUCCESS;
    }
    let Some(command) = COMMANDS.iter().find(|c| c.name == name) else {
        eprintln!("cray-xmp: unknown command `{}`\n", name);
        eprint!("{}", usage());
        return ExitCode::from(EXIT_USAGE);
    };
    let rest = &argv[1..];
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: {}\n\n{}",
            format!("cray-xmp {} {}", command.name, command.synopsis).trim_end(),
            command.details.trim()
        );
        return ExitCode::SUCCESS;
    }
    match (command.run)(rest) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("cray-xmp {}: {}", command.name, message);
            if message.starts_with(args::USAGE_PREFIX) {
                eprintln!("usage: cray-xmp {} {}", command.name, command.synopsis);
                return ExitCode::from(EXIT_USAGE);
            }
            ExitCode::FAILURE
        }
    }
}

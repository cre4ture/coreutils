// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//  *
//  * Synced with http://lingrok.org/xref/coreutils/src/tty.c

// spell-checker:ignore (ToDO) ttyname filedesc

use clap::{crate_version, Arg, ArgAction, Command};
use uucore::io::OwnedFileDescriptorOrHandle;
use std::io::{IsTerminal, Write};
use uucore::error::{set_exit_code, UResult, USimpleError};
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("tty.md");
const USAGE: &str = help_usage!("tty.md");

mod options {
    pub const SILENT: &str = "silent";
    pub const STDIO: &str = "stdio";
}

fn inspect_one(silent: bool, name: Option<&str>, fx: OwnedFileDescriptorOrHandle) -> std::io::Result<bool> {

    let is_terminal = fx.as_raw().is_terminal();

    // If silent, we don't need the name, only whether or not stdin is a tty.
    if silent {
        return Ok(is_terminal);
    };

    let mut stdout = std::io::stdout();
    if let Some(name) = name {
        write!(stdout, "{name}: ")?;
    }
    if is_terminal {
        #[cfg(unix)]
        let name = nix::unistd::ttyname(selected_stdio).display();
        #[cfg(windows)]
        let name: Result<&str, ()> = Ok("windows-terminal");

        match name {
            Ok(name) => writeln!(stdout, "{}", name)?,
            Err(_e) => {
                writeln!(stdout, "not a tty")?;
                return Ok(false);
            }
        };
    } else {
        writeln!(stdout, "not a tty")?;
    }

    Ok(is_terminal)
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().get_matches_from(args);

    let silent = matches.get_flag(options::SILENT);
    let stdio_str = matches.get_many::<String>(options::STDIO).unwrap();
    let with_name = stdio_str.len() > 1;

    let mut are_all_terminal = true;
    for d in stdio_str {

        let selected_stdio = match d.as_str() {
            "in" => OwnedFileDescriptorOrHandle::from(std::io::stdin()),
            "out" => OwnedFileDescriptorOrHandle::from(std::io::stdout()),
            "err" => OwnedFileDescriptorOrHandle::from(std::io::stderr()),
            s => return Err(USimpleError::new(2, format!("unknown stdio name provided: {s}"))),
        }?;

        let is_terminal = inspect_one(silent, with_name.then_some(d.as_str()), selected_stdio).map_err(|_| -> std::io::Error {
                // Don't return to prevent a panic later when another flush is attempted
                // because the `uucore_procs::main` macro inserts a flush after execution for every utility.
                std::process::exit(3);
            })?;
        are_all_terminal = are_all_terminal && is_terminal;
    }

    if !are_all_terminal {
        set_exit_code(1);
    }

    Ok(())
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new(options::SILENT)
                .long(options::SILENT)
                .visible_alias("quiet")
                .short('s')
                .help("print nothing, only return an exit status")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::STDIO)
                .long(options::STDIO)
                .short('d')
                .help("which stdio to query for. This is a uutils specific extension.")
                .value_delimiter(',')
                .default_values(["in"])
                .value_parser([
                    "in", "out", "err"
                ]),
        )
}

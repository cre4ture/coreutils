use std::{fs::File, io::{self, stdin, stdout, Stdout}};

use clap::{crate_version, Arg, ArgAction, ArgMatches, Command};
use uucore::{error::{UResult, USimpleError}, format_usage, help_about, help_usage, io::OwnedFileDescriptorOrHandle};


#[cfg(windows)]
mod windows;

#[cfg(unix)]
mod unix;


mod options {
    pub const ALL: &str = "all";
    pub const SAVE: &str = "save";
    pub const FILE: &str = "file";
    pub const SETTINGS: &str = "settings";
}

const USAGE: &str = help_usage!("stty.md");
const SUMMARY: &str = help_about!("stty.md");

struct Options<'a> {
    all: bool,
    save: bool,
    file: OwnedFileDescriptorOrHandle,
    settings: Option<Vec<&'a str>>,
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .override_usage(format_usage(USAGE))
        .about(SUMMARY)
        .infer_long_args(true)
        .arg(
            Arg::new(options::ALL)
                .short('a')
                .long(options::ALL)
                .help("print all current settings in human-readable form")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::SAVE)
                .short('g')
                .long(options::SAVE)
                .help("print all current settings in a stty-readable form")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::FILE)
                .short('F')
                .long(options::FILE)
                .value_hint(clap::ValueHint::FilePath)
                .value_name("DEVICE")
                .help("open and use the specified DEVICE instead of stdin"),
        )
        .arg(
            Arg::new(options::SETTINGS)
                .action(ArgAction::Append)
                .help("settings to change"),
        )
}


#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let opts = Options::from(&matches)?;

    if opts.save && opts.all {
        return Err(USimpleError::new(
            1,
            "the options for verbose and stty-readable output styles are mutually exclusive",
        ));
    }

    if opts.settings.is_some() && (opts.save || opts.all) {
        return Err(USimpleError::new(
            1,
            "when specifying an output style, modes may not be set",
        ));
    }

    #[cfg(unix)]
    let r = unix::stty::stty(&opts);
    #[cfg(windows)]
    let r = windows::stty(&opts);
    r
}


impl<'a> Options<'a> {
    fn from(matches: &'a ArgMatches) -> io::Result<Self> {
        Ok(Self {
            all: matches.get_flag(options::ALL),
            save: matches.get_flag(options::SAVE),
            file: match matches.get_one::<String>(options::FILE) {
                // Two notes here:
                // 1. O_NONBLOCK is needed because according to GNU docs, a
                //    POSIX tty can block waiting for carrier-detect if the
                //    "clocal" flag is not set. If your TTY is not connected
                //    to a modem, it is probably not relevant though.
                // 2. We never close the FD that we open here, but the OS
                //    will clean up the FD for us on exit, so it doesn't
                //    matter. The alternative would be to have an enum of
                //    BorrowedFd/OwnedFd to handle both cases.
                Some(f) => {
                    #[cfg(unix)]
                    let r = unix::open_file_of_options(f)?;
                    #[cfg(windows)]
                    let r = windows::open_file_of_options(f)?;
                    r
                }
                None => OwnedFileDescriptorOrHandle::from(stdout())?,
            },
            settings: matches
                .get_many::<String>(options::SETTINGS)
                .map(|v| v.map(|s| s.as_ref()).collect()),
        })
    }
}

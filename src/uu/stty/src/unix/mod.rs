// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore clocal

use std::os::unix::fs::OpenOptionsExt;

use nix::libc::O_NONBLOCK;
use uucore::io::OwnedFileDescriptorOrHandle;

mod flags;

pub(crate) mod stty;

pub(crate) fn open_file_of_options(f: &str) -> std::io::Result<OwnedFileDescriptorOrHandle> {
    // Two notes here:
    // 1. O_NONBLOCK is needed because according to GNU docs, a
    //    POSIX tty can block waiting for carrier-detect if the
    //    "clocal" flag is not set. If your TTY is not connected
    //    to a modem, it is probably not relevant though.
    // 2. We never close the FD that we open here, but the OS
    //    will clean up the FD for us on exit, so it doesn't
    //    matter. The alternative would be to have an enum of
    //    BorrowedFd/OwnedFd to handle both cases.
    OwnedFileDescriptorOrHandle::from(
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(f)?,
    )
}
mod flags;

pub(crate) mod stty;

pub(crate) fn open_file_of_options(f: &str) -> io::Result<OwnedFileDescriptorOrHandle> {
    // Two notes here:
    // 1. O_NONBLOCK is needed because according to GNU docs, a
    //    POSIX tty can block waiting for carrier-detect if the
    //    "clocal" flag is not set. If your TTY is not connected
    //    to a modem, it is probably not relevant though.
    // 2. We never close the FD that we open here, but the OS
    //    will clean up the FD for us on exit, so it doesn't
    //    matter. The alternative would be to have an enum of
    //    BorrowedFd/OwnedFd to handle both cases.
    Ok(OwnedFileDescriptorOrHandle::from(
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(f)?,
    )?)
}

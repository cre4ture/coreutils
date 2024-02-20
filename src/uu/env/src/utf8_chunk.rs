// copies from https://github.com/dylni/os_str_bytes/blob/master/src/iter.rs

use std::{ffi::{OsStr, OsString}, iter::FusedIterator, mem};

use os_str_bytes::OsStrBytes;

/// A container for platform strings containing no unicode characters.
///
/// Instances can only be constructed using [`Utf8Chunks`].
#[derive(Debug)]
#[cfg_attr(os_str_bytes_docs_rs, doc(cfg(feature = "raw_os_str")))]
#[repr(transparent)]
pub struct NonUnicodeOsStr(OsStr);

impl NonUnicodeOsStr {
    unsafe fn from_inner(string: &OsStr) -> &Self {
        // SAFETY: This struct has a layout that makes this operation safe.
        unsafe { mem::transmute(string) }
    }

    pub(super) unsafe fn new_unchecked(string: &[u8]) -> &Self {
        // SAFETY: This method has stricter safety requirements.
        unsafe { Self::from_inner(os_str(string)) }
    }

    /// Converts this representation back to a platform-native string, without
    /// copying or encoding conversion.
    #[inline]
    #[must_use]
    pub fn as_os_str(&self) -> &OsStr {
        &self.0
    }
}

impl AsRef<OsStr> for NonUnicodeOsStr {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        &self.0
    }
}

/// The iterator returned by [`OsStrBytesExt::utf8_chunks`].
///
/// [`OsStrBytesExt::utf8_chunks`]: super::OsStrBytesExt::utf8_chunks
#[derive(Clone, Debug)]
#[must_use]
pub struct Utf8Chunks<'a> {
    string: &'a OsStr,
    invalid_length: usize,
}

impl<'a> Utf8Chunks<'a> {
    pub(super) fn new(string: &'a OsStr) -> Self {
        Self {
            string,
            invalid_length: 0,
        }
    }
}

impl FusedIterator for Utf8Chunks<'_> {}

impl<'a> Iterator for Utf8Chunks<'a> {
    type Item = (&'a NonUnicodeOsStr, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let string = self.string.as_encoded_bytes();
        if string.is_empty() {
            debug_assert_eq!(0, self.invalid_length);
            return None;
        }

        loop {
            let (invalid, substring) = string.split_at(self.invalid_length);

            let valid = match std::str::from_utf8(substring) {
                Ok(valid) => {
                    self.string = OsStr::new("");
                    self.invalid_length = 0;
                    valid
                }
                Err(error) => {
                    let (valid, substring) =
                        substring.split_at(error.valid_up_to());

                    let invalid_length =
                        error.error_len().unwrap_or_else(|| substring.len());
                    if valid.is_empty() {
                        self.invalid_length += invalid_length;
                        continue;
                    }
                    // SAFETY: This substring was separated by a UTF-8 string.
                    self.string = unsafe { OsStr::new(&substring) };
                    self.invalid_length = invalid_length;

                    // SAFETY: This slice was validated to be UTF-8.
                    unsafe { str::from_utf8_unchecked(valid) }
                }
            };

            // SAFETY: This substring was separated by a UTF-8 string and
            // validated to not be UTF-8.
            let invalid = unsafe { NonUnicodeOsStr::new_unchecked(invalid) };
            return Some((invalid, valid));
        }
    }
}

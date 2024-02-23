// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.


use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::os::windows::prelude::*;
use std::{borrow::Cow, ffi::OsStr};

#[cfg(target_os = "windows")]
use u16 as NativeIntCharU;
#[cfg(not(target_os = "windows"))]
use u8 as NativeIntCharU;

pub type NativeCharIntT = NativeIntCharU;
pub type NativeIntStrT = [NativeCharIntT];
pub type NativeIntString = Vec<NativeCharIntT>;

pub fn to_native_int_representation(input: &OsStr) -> Cow<'_, NativeIntStrT> {
    #[cfg(target_os = "windows")]
    {
        Cow::Owned(input.encode_wide().collect())
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::ffi::OsStrExt;
        Cow::Borrowed(input.as_bytes())
    }
}

pub fn from_native_int_representation(input: Cow<'_, NativeIntStrT>) -> Cow<'_, OsStr> {
    #[cfg(target_os = "windows")]
    {
        Cow::Owned(OsString::from_wide(&input))
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::ffi::OsStringExt;
        match input {
            Cow::Borrowed(borrow) => Cow::Borrowed(OsStr::from_bytes(borrow)),
            Cow::Owned(own) => Cow::Owned(OsString::from_vec(own)),
        }
    }
}

pub fn from_native_int_representation_owned(input: NativeIntString) -> OsString {
    #[cfg(target_os = "windows")]
    {
        Cow::Owned(OsString::from_wide(&input))
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(input)
    }
}

pub fn get_single_native_int_value(c: char) -> Option<NativeCharIntT> {
    #[cfg(target_os = "windows")]
    {
        let mut buf = [0u16, 0];
        let s = c.encode_utf16(&mut buf);
        if s.len() == 1 {
            Some(buf[0])
        } else {
            None
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut buf = [0u8, 0, 0, 0];
        let s = c.encode_utf8(&mut buf);
        if s.len() == 1 {
            Some(buf[0])
        } else {
            None
        }
    }
}

pub fn get_char_from_native_int(ni: NativeCharIntT) -> Option<(char, NativeCharIntT)> {
    let c_opt;
    #[cfg(target_os = "windows")]
    {
        c_opt = char::decode_utf16([ni; 1]).next().unwrap().ok();
    };

    #[cfg(not(target_os = "windows"))]
    {
        c_opt = std::str::from_utf8(&[ni; 1])
            .ok()
            .map(|x| x.chars().next().unwrap());
    };

    if let Some(c) = c_opt {
        return Some((c, ni));
    }

    None
}

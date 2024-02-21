// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//
// spell-checker:ignore (words) splitted FFFD
#![forbid(unsafe_code)]

use std::{borrow::Cow, ffi::OsStr};
#[cfg(target_os = "windows")]
use std::os::windows::prelude::*;
#[cfg(target_os = "windows")]
use std::ffi::OsString;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub peek_position: usize,
    pub err_type: ErrorType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorType {
    EndOfInput,
    InternalError,
}

/// Provides a valid char or a invalid sequence of bytes.
///
/// Invalid byte sequences can't be splitted in any meaningful way.
/// Thus, they need to be consumed as one piece.
pub enum Chunk<'a> {
    InvalidEncoding(Cow<'a, OsStr>),
    ValidChar(char),
}

#[cfg(target_os = "windows")]
use u16 as NativeCharIntT;
#[cfg(not(target_os = "windows"))]
use u8 as NativeCharIntT;

fn to_native_int_representation<'a>(input: &'a OsStr) -> Cow<'a, [NativeCharIntT]> {
    #[cfg(target_os = "windows")]
    {
        Cow::Owned(input.encode_wide().collect())
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::ffi::OsStrExt;
        Cow::Borrowed::<'a>(input.as_bytes())
    }
}

fn from_native_int_representation<'a>(input: &'a [NativeCharIntT]) -> Cow<'a, OsStr> {
    #[cfg(target_os = "windows")]
    {
        Cow::Owned(OsString::from_wide(input))
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::ffi::OsStrExt;
        Cow::Borrowed(OsStr::from_bytes(input))
    }
}

fn get_single_native_int_value(c: char) -> Option<NativeCharIntT> {
    #[cfg(target_os = "windows")]
    {
        let mut buf = [0u16,0];
        let s = c.encode_utf16(&mut buf);
        if s.len() == 1 {
            Some(buf[0])
        } else {
            None
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut buf = [0u8,0,0,0];
        let s = c.encode_utf8(&mut buf);
        if s.len() == 1 {
            Some(buf[0])
        } else {
            None
        }
    }
}

fn get_char_from_single_native_int_value(ni: NativeCharIntT) -> Option<char> {
    #[cfg(target_os = "windows")]
    {
        // (ni <= 0xD7FF) || (0xE000 >= ni && ni <= 0xFFFF)
        char::decode_utf16([ni;1]).next().unwrap().ok()
    }

    #[cfg(not(target_os = "windows"))]
    {
        char::try_from(ni).ok()
    }
}

/// This class makes parsing a OsString char by char more convenient.
///
/// It also allows to capturing of intermediate positions for later splitting.
pub struct StringParser<'a, 'b>{
    input_cow: Cow<'a, [NativeCharIntT]>,
    input: &'b [NativeCharIntT],
    pointer: usize,
    remaining: &'b [NativeCharIntT],
    //remaining_str: &'b OsStr,
}

impl<'a, 'b> StringParser<'a, 'b> {
    pub fn new<S: AsRef<OsStr> + ?Sized>(input_str: &'a S) -> Self {
        let input_cow = to_native_int_representation(input_str.as_ref());
        let mut instance = Self {
            input_cow,
            input: &[0;0],
            pointer: 0,
            remaining: &[0;0],
            //remaining_str: &OsStr::new("")
        };
        instance.set_pointer(0);
        instance
    }

    pub fn new_at(input: &'a OsStr, pos: usize) -> Result<Self, Error> {
        let mut instance = Self::new(input);
        instance.set_pointer(pos);
        Ok(instance)
    }

    pub fn get_peek_position(&self) -> usize {
        self.pointer
    }

    pub fn peek(&self) -> Result<char, Error> {
        self.peek_char_at_pointer(self.pointer)
    }

    fn make_err(&self, err_type: ErrorType) -> Error {
        Error {
            peek_position: self.get_peek_position(),
            err_type,
        }
    }

    pub fn peek_char_at_pointer(&self, at_pointer: usize) -> Result<char, Error> {
        let split = self.input.split_at(at_pointer).1;
        if split.len() == 0 {
            return Err(self.make_err(ErrorType::EndOfInput));
        }
        let next = split[0];
        if let Some(c) = get_char_from_single_native_int_value(next) {
            Ok(c)
        } else {
            Ok('\u{FFFD}')
        }
    }

    fn get_chunk_with_length_at(&self, pointer: usize) -> Result<(Chunk<'a>, usize), Error> {
        let (_before, after) = self.input.split_at(pointer);
        if after.len() == 0 {
            return Err(self.make_err(ErrorType::EndOfInput))
        }

        let next_int = after[0];
        if let Some(c) = get_char_from_single_native_int_value(next_int) {
            Ok((Chunk::ValidChar(c), 1))
        } else {
            let mut i = 1;
            while i < after.len() {
                if let Some(c) = get_char_from_single_native_int_value(after[i]) {
                    break;
                }
                i += 1;
            }

            let chunk = &after[0..i];
            let str = from_native_int_representation(chunk);
            Ok((Chunk::InvalidEncoding(str), chunk.len()))
        }
    }

    pub fn peek_chunk(&self) -> Option<Chunk<'a>> {
        return self
            .get_chunk_with_length_at(self.pointer)
            .ok()
            .map(|(chunk, _)| chunk);
    }

    pub fn consume_chunk(&mut self) -> Result<Chunk<'a>, Error> {
        let (chunk, len) = self.get_chunk_with_length_at(self.pointer)?;
        self.set_pointer(self.pointer + len);
        Ok(chunk)
    }

    pub fn consume_one_ascii_or_all_non_ascii(&mut self) -> Result<Vec<Chunk<'a>>, Error> {
        let mut result = Vec::<Chunk<'a>>::new();
        loop {
            let data = self.consume_chunk()?;
            let was_ascii = if let Chunk::ValidChar(c) = &data {
                c.is_ascii()
            } else {
                false
            };
            result.push(data);
            if was_ascii {
                return Ok(result);
            }

            match self.peek_chunk() {
                Some(Chunk::ValidChar(c)) if c.is_ascii() => return Ok(result),
                None => return Ok(result),
                _ => {}
            }
        }
    }

    pub fn skip_multiple(&mut self, skip_byte_count: usize) {
        let end_ptr = self.pointer + skip_byte_count;
        self.set_pointer(end_ptr);
    }

    pub fn skip_until_char_or_end(&mut self, c: char) {
        let native_rep = get_single_native_int_value(c).unwrap();
        let pos = self.remaining.iter().position(|x| *x == native_rep);

        if let Some(pos) = pos {
            self.set_pointer(self.pointer + pos);
        } else {
            self.set_pointer(self.input.len());
        }
    }

    pub fn substring(&self, range: &std::ops::Range<usize>) -> Cow<'a, OsStr> {
        let (_before1, after1) = self.input.split_at(range.start);
        let (middle, _after2) = after1.split_at(range.end - range.start);
        from_native_int_representation(middle)
    }

    pub fn peek_remaining(&self) -> Cow<'a, OsStr> {
        from_native_int_representation(&*self.remaining)
    }

    pub fn set_pointer(&mut self, new_pointer: usize) {
        self.pointer = new_pointer;
        let (_before, after) = self.input.split_at(self.pointer);
        self.remaining = after.into();
        //self.remaining_str = self.peek_remaining();
    }
}

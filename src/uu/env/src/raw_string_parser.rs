// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//
//! SAFETY: This module does "unsafe" byte by byte operations on a UTF8 encoded string.
//! UTF8 encodes all non-ASCII characters as multi-byte characters. Meaning that the UTF8
//! string contains short sequences of bytes which should not be splitted or individually modified.
//! All bytes that belong to a multi-byte character sequence are defined to have a different value
//! than any ASCII single byte char. This can be used to easily detect where multi-byte character sequences
//! start and end.
//! To guarantee that after processing the output is again valid UTF8, the following rules must apply:
//! 1. Move multi-byte characters as a whole.
//! 2. Insert characters only on ASCII boundaries.
//! We also want to support even strings that contain partially invalid utf8. Thats why we can't rely
//! on std library functionality when dealing with multi-byte characters.
//!
//! The general idea of this module is to encapsulate the unsafe parts in a small and easily testable unit.
// spell-checker:ignore (words) splitted
#![forbid(unsafe_code)]

use std::{
    ffi::{OsStr, OsString},
    mem,
};


use os_str_bytes::{NonUnicodeOsStr, OsStrBytesExt};


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub look_at_pos: usize,
    pub err_type: ErrorType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorType {
    NoAsciiBoundary,
    NoAsciiChar,
    NoAsciiCharInput,
    EndOfInput,
    InternalError,
}

pub struct RawStringParser<'a> {
    pub input: &'a OsStr,
    split: os_str_bytes::iter::Utf8Chunks<'a>,
    chunk_nu: Option<&'a NonUnicodeOsStr>,
    chunk_str: Option<&'a str>,
    pointer: usize,
    pointer_str: &'a OsStr,
}

pub struct RawStringExpander<'a> {
    parser: RawStringParser<'a>,
    output: OsString,
}

impl<'a> RawStringExpander<'a> {
    pub fn new<S: AsRef<OsStr> + ?Sized>(input: &'a S) -> Self {
        Self {
            parser: RawStringParser::new(input),
            output: OsString::default(),
        }
    }

    pub fn new_at(input: &'a OsStr, pos: usize) -> Result<Self, Error> {
        Ok(Self {
            parser: RawStringParser::new_at(input, pos)?,
            output: OsString::default(),
        })
    }

    pub fn get_parser(&self) -> &RawStringParser<'a> {
        &self.parser
    }

    pub fn get_parser_mut(&mut self) -> &mut RawStringParser<'a> {
        &mut self.parser
    }

    pub fn skip_one(&mut self) -> Result<(), Error> {
        self.get_parser_mut().consume_till_next_ascii_or_end()?;
        Ok(())
    }

    pub fn get_look_at_pos(&self) -> usize {
        self.get_parser().get_look_at_pos()
    }

    pub fn take_one(&mut self) -> Result<(), Error> {

        let chunks = self.parser.consume_till_next_ascii_or_end()?;
        for chunk in chunks {
            match chunk {
                Chunk::InvalidEncoding(invalid) => self.output.push(invalid),
                Chunk::ValidChar(char) => self.output.push(char.to_string()),
            }
        }
        Ok(())
    }

    pub fn put_one_ascii(&mut self, c: char) -> Result<(), Error> {
        self.output.push(c.to_string());
        Ok(())
    }

    pub fn put_string(&mut self, str: &OsStr) -> Result<(), Error> {
        self.output.push(str);
        Ok(())
    }

    pub fn put_string_utf8(&mut self, str: &str) -> Result<(), Error> {
        self.put_string(&OsString::from(str))
    }

    pub fn take_collected_output(&mut self) -> Result<OsString, Error> {
        Ok(mem::take(&mut self.output))
    }
}

pub enum Chunk<'a> {
    InvalidEncoding(&'a OsStr),
    ValidChar(char),
}

impl<'a> RawStringParser<'a> {
    pub fn new<S: AsRef<OsStr> + ?Sized>(input: &'a S) -> Self {
        let input = input.as_ref();
        Self {
            input,
            split: input.utf8_chunks(),
            chunk_nu: None,
            chunk_str: None,
            pointer: 0,
            pointer_str: input,
        }
    }

    pub fn new_at(input: &'a OsStr, pos: usize) -> Result<Self, Error> {
        let instance = Self {
            input,
            split: input.split_at(pos).1.utf8_chunks(),
            chunk_nu: None,
            chunk_str: None,
            pointer: pos,
            pointer_str: input,
        };

        if !instance.detect_boundary()? {
            return Err(Error {
                look_at_pos: instance.get_look_at_pos(),
                err_type: ErrorType::NoAsciiBoundary,
            });
        }

        Ok(instance)
    }

    pub fn get_look_at_pos(&self) -> usize {
        self.pointer
    }

    pub fn look_at(&self) -> Result<char, Error> {
        self.look_at_pointer(self.pointer)
    }

    fn make_err(&self, err_type: ErrorType) -> Error {
        Error {
            look_at_pos: self.get_look_at_pos(),
            err_type,
        }
    }

    pub fn look_at_pointer(&self, at_pointer: usize) -> Result<char, Error> {
        let mut split = self.input.split_at(at_pointer).1.utf8_chunks();
        let next = split.next();
        if let Some((a,b)) = next {
            if a.as_os_str().is_empty() {
                return Ok(b.chars().next().unwrap());
            } else {
                return Ok('\u{FFFD}');
            }
        }
        Err(self.make_err(ErrorType::EndOfInput))
    }

    fn check_chunk(&mut self) {
        if self.chunk_nu.is_none() && (self.chunk_str.is_none() || (self.chunk_str.unwrap().len() == 0)) {
            let next = self.split.next();
            if let Some(next) = next {
                self.chunk_nu = if next.0.as_os_str().is_empty() { None } else { Some(next.0) };
                self.chunk_str = if next.1.is_empty() { None } else { Some(next.1) };
            }
        }
    }

    pub fn peek_one(&mut self) -> Option<Chunk> {
        self.check_chunk();

        if let Some(_nu) = self.chunk_nu {
            let data = _nu.as_os_str();
            return Some(Chunk::InvalidEncoding(data));
        }

        if let Some(str) = &mut self.chunk_str {
            let mut iter = str.char_indices();
            if let Some((_pos, char)) = iter.next() {
                return Some(Chunk::ValidChar(char));
            }
        }

        return None;
    }

    pub fn consume_one(&mut self) -> Result<Chunk<'a>, Error> {

        self.check_chunk();

        if let Some(_nu) = self.chunk_nu {
            let data = _nu.as_os_str();
            self.chunk_nu = None;
            return Ok(Chunk::InvalidEncoding(data));
        }

        if let Some(str) = &mut self.chunk_str {
            let mut iter = str.char_indices();
            if let Some((_pos, char)) = iter.next() {
                if let Some((pos2, _char2)) = iter.next() {
                    *str = &str[pos2..];
                } else {
                    self.chunk_str = None;
                }
                return Ok(Chunk::ValidChar(char));
            }
        }

        return Err(self.make_err(ErrorType::EndOfInput));
    }

    pub fn consume_till_next_ascii_or_end(&mut self) -> Result<Vec<Chunk<'a>>, Error> {
        let mut result = Vec::<Chunk<'a>>::new();
        loop {
            let data = self.consume_one()?;
            result.push(data);
            match self.peek_one() {
                Some(Chunk::ValidChar(c)) if c.is_ascii() => return Ok(result),
                None => return Ok(result),
                _ => {}
            }
        }
    }

    pub fn skip_till_next_ascii(&mut self) -> Result<(), Error> {
        self.consume_till_next_ascii_or_end()?;
        Ok(())
    }

    pub fn skip_multiple_ascii_bounded(&mut self, skip_byte_count: usize) -> Result<(), Error> {
        let start_bounds = self.detect_boundary_at(self.pointer)?;
        let end_ptr = self.pointer + skip_byte_count;
        let end_bounds = self.detect_boundary_at(end_ptr)?;
        if start_bounds && end_bounds {
            self.set_pointer(end_ptr)?;
            return Ok(());
        }

        Err(self.make_err(ErrorType::NoAsciiBoundary))
    }

    pub fn skip_until_ascii_char_or_end(&mut self, c: char) -> Result<(), Error> {

        let pos = self.pointer_str.find(c);

        if let Some(pos) = pos {
            self.set_pointer(self.pointer + pos)?;
        } else {
            self.set_pointer(self.input.len())?;
        }
        return Ok(());
    }

    pub fn detect_boundary_at(&self, at_pointer: usize) -> Result<bool, Error> {
        let boundary_detected = (at_pointer == 0)
            || (at_pointer == self.input.len())
            || (self.look_at_pointer(at_pointer)?).is_ascii()
            || (self.look_at_pointer(at_pointer - 1)?).is_ascii();
        Ok(boundary_detected)
    }

    pub fn detect_boundary(&self) -> Result<bool, Error> {
        self.detect_boundary_at(self.pointer)
    }

    pub fn get_substring(&self, range: &std::ops::Range<usize>) -> Result<&'a OsStr, Error> {
        let start_boundary = self.detect_boundary_at(range.start)?;
        let end_boundary = self.detect_boundary_at(range.end)?;
        if start_boundary && end_boundary {
            let (_before1, after1) = self.input.split_at(range.start);
            let (middle, _after2) = after1.split_at(range.end - range.start);
            Ok(middle)
        } else {
            Err(self.make_err(ErrorType::NoAsciiBoundary))
        }
    }

    pub fn look_at_remaining(&self) -> Result<&'a OsStr, Error> {
        let boundary_detected = self.detect_boundary()?;
        if boundary_detected {
            let (_before, after) = self.input.split_at(self.pointer);
            Ok(after)
        } else {
            Err(self.make_err(ErrorType::NoAsciiBoundary))
        }
    }

    fn set_pointer(&mut self, new_pointer: usize) -> Result<(), Error> {
        self.pointer = new_pointer;
        self.pointer_str = self.look_at_remaining()?;
        Ok(())
    }
}

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
#![allow(unsafe_code)]

use std::mem;

pub fn is_ascii(c: u8) -> bool {
    (c & 0x80) == 0
}

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
    pub input: &'a str,
    pointer: usize,
    pointer_str: &'a str, // just for debugging sessions. In release build it will be removed by the compiler.
}

pub struct RawStringExpander<'a> {
    parser: RawStringParser<'a>,
    output: String,
}

impl<'a> RawStringExpander<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            parser: RawStringParser::new(input),
            output: String::default(),
        }
    }

    pub fn new_at(input: &'a str, pos: usize) -> Result<Self, Error> {
        Ok(Self {
            parser: RawStringParser::new_at(input, pos)?,
            output: String::default(),
        })
    }

    pub fn get_parser(&self) -> &RawStringParser<'a> {
        &self.parser
    }

    pub fn get_parser_mut(&mut self) -> &mut RawStringParser<'a> {
        &mut self.parser
    }

    pub fn skip_one(&mut self) -> Result<(), Error> {
        self.get_parser_mut().skip_one()
    }

    pub fn get_look_at_pos(&self) -> usize {
        self.get_parser().get_look_at_pos()
    }

    pub fn take_one(&mut self) -> Result<(), Error> {
        let parser = &mut self.parser;
        let mut c = parser.look_at()?;
        loop {
            // SAFETY: Just moving any non-ASCII sequence as a whole is keeping multibyte chars intact.
            // SAFETY: Additionally, the function 'take_collected_output' ensures that
            // we only take the result when its end is at a ASCII boundary
            unsafe {
                self.output.as_mut_vec().push(c);
            }
            parser.set_pointer(parser.pointer + 1);

            if is_ascii(c) {
                break; // stop at ASCII boundary
            }

            if parser.pointer == parser.input.as_bytes().len() {
                break;
            }

            c = parser.look_at()?;
            if is_ascii(c) {
                break; // stop at ASCII boundary
            }
        }

        Ok(())
    }

    pub fn put_one_ascii(&mut self, c: u8) -> Result<(), Error> {
        let parser = &self.parser;
        if !is_ascii(c) {
            return Err(parser.make_err(ErrorType::NoAsciiCharInput)); // SAFETY: only ASCII character are allowed to be pushed this way.
        }
        let boundary_detected = parser.detect_boundary()?;
        if boundary_detected {
            // SAFETY: when current look_at is ascii or the one before or we are at one of the two ends of the input,
            // then we can't destroy a multi-byte-non-ascii char of input.
            unsafe {
                self.output.as_mut_vec().push(c);
            }
            Ok(())
        } else {
            Err(parser.make_err(ErrorType::NoAsciiBoundary))
        }
    }

    pub fn put_string_utf8(&mut self, str: &str) -> Result<(), Error> {
        let parser = &self.parser;
        let boundary_detected = parser.detect_boundary()?;
        if boundary_detected {
            // SAFETY: when current look_at is ascii or the one before or we are at one of the two ends of the input,
            // then we can't destroy a multi-byte-non-ascii char of input.
            self.output.push_str(str);
            Ok(())
        } else {
            Err(parser.make_err(ErrorType::NoAsciiBoundary))
        }
    }

    pub fn take_collected_output(&mut self) -> Result<String, Error> {
        let parser = &self.parser;
        let boundary_detected = parser.detect_boundary()?;
        if boundary_detected {
            // SAFETY: when current look_at is ascii or the one before or we are at one of the two ends of the input,
            // then we can't destroy a multi-byte-non-ascii char of input.
            Ok(mem::take(&mut self.output))
        } else {
            Err(parser.make_err(ErrorType::NoAsciiBoundary))
        }
    }
}

impl<'a> RawStringParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pointer: 0,
            pointer_str: input,
        }
    }

    pub fn new_at(input: &'a str, pos: usize) -> Result<Self, Error> {
        let instance = Self {
            input,
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

    pub fn look_at(&self) -> Result<u8, Error> {
        self.look_at_pointer(self.pointer)
    }

    fn make_err(&self, err_type: ErrorType) -> Error {
        Error {
            look_at_pos: self.get_look_at_pos(),
            err_type,
        }
    }

    pub fn look_at_pointer(&self, at_pointer: usize) -> Result<u8, Error> {
        let c = self.input.as_bytes().get(at_pointer);
        if let Some(c) = c {
            Ok(*c)
        } else {
            Err(self.make_err(ErrorType::EndOfInput))
        }
    }

    pub fn skip_one(&mut self) -> Result<(), Error> {
        let mut c = self.look_at()?;
        loop {
            // SAFETY: Just skipping any non-ASCII sequence as a whole is keeping multibyte chars intact.
            // SAFETY: Additionally, the function 'take_collected_output' ensures that
            // we only take the result when its end is at a ASCII boundary
            self.set_pointer(self.pointer + 1);

            if is_ascii(c) {
                break; // stop at ASCII boundary
            }

            if self.pointer == self.input.as_bytes().len() {
                break;
            }

            c = self.look_at()?;
            if is_ascii(c) {
                break; // stop at ASCII boundary
            }
        }

        Ok(())
    }

    pub fn skip_multiple_ascii_bounded(&mut self, skip_byte_count: usize) -> Result<(), Error> {
        let start_bounds = self.detect_boundary_at(self.pointer)?;
        let end_ptr = self.pointer + skip_byte_count;
        let end_bounds = self.detect_boundary_at(end_ptr)?;
        if start_bounds && end_bounds {
            self.set_pointer(end_ptr);
            return Ok(());
        }

        Err(self.make_err(ErrorType::NoAsciiBoundary))
    }

    pub fn skip_until_ascii_char_or_end(&mut self, c: u8) -> Result<(), Error> {
        if !is_ascii(c) {
            return Err(self.make_err(ErrorType::NoAsciiCharInput));
        }
        let boundary = self.detect_boundary()?;
        if !boundary {
            // SAFETY: moving away from within a multi-byte char is not allowed
            return Err(self.make_err(ErrorType::NoAsciiBoundary));
        }
        let remaining = self.input.as_bytes().get(self.pointer..);
        if let Some(remaining_str) = remaining {
            let pos = memchr::memchr(c, remaining_str);
            if let Some(pos) = pos {
                // SAFETY: new pointer position is on ASCII char
                self.set_pointer(self.pointer + pos);
            } else {
                // SAFETY: setting pointer to the end should be valid as input is valid
                self.set_pointer(self.input.len());
            }
            return Ok(());
        }
        Err(self.make_err(ErrorType::InternalError))
    }

    pub fn detect_boundary_at(&self, at_pointer: usize) -> Result<bool, Error> {
        let boundary_detected = (at_pointer == 0)
            || (at_pointer == self.input.bytes().len())
            || is_ascii(self.look_at_pointer(at_pointer)?)
            || is_ascii(self.look_at_pointer(at_pointer - 1)?);
        Ok(boundary_detected)
    }

    pub fn detect_boundary(&self) -> Result<bool, Error> {
        self.detect_boundary_at(self.pointer)
    }

    pub fn get_substring(&self, range: &std::ops::Range<usize>) -> Result<&'a str, Error> {
        let start_boundary = self.detect_boundary_at(range.start)?;
        let end_boundary = self.detect_boundary_at(range.end)?;
        if start_boundary && end_boundary {
            Ok(self.input.get(range.start..range.end).unwrap())
        } else {
            Err(self.make_err(ErrorType::NoAsciiBoundary))
        }
    }

    pub fn look_at_remaining(&self) -> Result<&'a str, Error> {
        let boundary_detected = self.detect_boundary()?;
        if boundary_detected {
            // SAFETY: when current look_at is ascii or the one before or we are at one of the two ends of the input,
            // then we can't destroy a multi-byte-non-ascii char of input.
            Ok(self.pointer_str)
        } else {
            Err(self.make_err(ErrorType::NoAsciiBoundary))
        }
    }

    // UNSAFE -> private
    fn set_pointer(&mut self, new_pointer: usize) {
        self.pointer = new_pointer;
        self.pointer_str = self.input.get(self.pointer..).unwrap_or("\u{FFFD}");
    }
}

// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//
// spell-checker:ignore (words) splitted
#![forbid(unsafe_code)]

use std::{
    ffi::{OsStr, OsString},
    mem,
};

use os_str_bytes::OsStrBytesExt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub look_at_pos: usize,
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
    InvalidEncoding(&'a OsStr),
    ValidChar(char),
}

/// This class makes parsing a OsString char by char more convenient.
///
/// It also allows to capturing of intermediate positions for later splitting.
pub struct RawStringParser<'a> {
    pub input: &'a OsStr,
    pointer: usize,
    pointer_str: &'a OsStr,
}

/// This class makes parsing and word collection more convenient.
///
/// It manages an "output" buffer that is automatically filled.
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
        self.get_parser_mut()
            .consumer_one_ascii_or_all_non_ascii()?;
        Ok(())
    }

    pub fn get_look_at_pos(&self) -> usize {
        self.get_parser().get_look_at_pos()
    }

    pub fn take_one(&mut self) -> Result<(), Error> {
        let chunks = self.parser.consumer_one_ascii_or_all_non_ascii()?;
        for chunk in chunks {
            match chunk {
                Chunk::InvalidEncoding(invalid) => self.output.push(invalid),
                Chunk::ValidChar(char) => self.output.push(char.to_string()),
            }
        }
        Ok(())
    }

    pub fn put_one_char(&mut self, c: char) {
        self.output.push(c.to_string());
    }

    pub fn put_string<S: AsRef<OsStr>>(&mut self, str: S) {
        self.output.push(str);
    }

    pub fn take_collected_output(&mut self) -> OsString {
        mem::take(&mut self.output)
    }
}

impl<'a> RawStringParser<'a> {
    pub fn new<S: AsRef<OsStr> + ?Sized>(input: &'a S) -> Self {
        let input = input.as_ref();
        Self {
            input,
            pointer: 0,
            pointer_str: input,
        }
    }

    pub fn new_at(input: &'a OsStr, pos: usize) -> Result<Self, Error> {
        let (_, remaining) = input.split_at(pos);
        let instance = Self {
            input,
            pointer: pos,
            pointer_str: remaining,
        };
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
        if let Some((a, b)) = next {
            if a.as_os_str().is_empty() {
                return Ok(b.chars().next().unwrap());
            } else {
                return Ok('\u{FFFD}');
            }
        }
        Err(self.make_err(ErrorType::EndOfInput))
    }

    fn get_chunk_with_length_at(&self, pointer: usize) -> Result<(Chunk<'a>, usize), Error> {
        let (_before, after) = self.input.split_at(pointer);
        let next_chunk = after.utf8_chunks().next();
        if let Some((nuo, s)) = next_chunk {
            let nuo_os = nuo.as_os_str();
            if !nuo_os.is_empty() {
                return Ok((Chunk::InvalidEncoding(nuo_os), nuo_os.len()));
            } else if !s.is_empty() {
                let (_, c) = s.char_indices().nth(0).unwrap();
                let (c_len, _) = s.char_indices().nth(1).unwrap_or((s.len(), '\0'));
                return Ok((Chunk::ValidChar(c), c_len));
            }
        }

        Err(self.make_err(ErrorType::EndOfInput))
    }

    pub fn look_at_chunk(&self) -> Option<Chunk<'a>> {
        return self.get_chunk_with_length_at(self.pointer).ok().map(|(chunk, _)| chunk);
    }

    pub fn consume_one(&mut self) -> Result<Chunk<'a>, Error> {
        let (chunk, len) = self.get_chunk_with_length_at(self.pointer)?;
        self.set_pointer(self.pointer + len);
        Ok(chunk)
    }

    pub fn consumer_one_ascii_or_all_non_ascii(&mut self) -> Result<Vec<Chunk<'a>>, Error> {
        let mut result = Vec::<Chunk<'a>>::new();
        loop {
            let data = self.consume_one()?;
            let was_ascii = if let Chunk::ValidChar(c) = &data {
                c.is_ascii()
            } else {
                false
            };
            result.push(data);
            if was_ascii {
                return Ok(result);
            }

            match self.look_at_chunk() {
                Some(Chunk::ValidChar(c)) if c.is_ascii() => return Ok(result),
                None => return Ok(result),
                _ => {}
            }
        }
    }

    pub fn skip_till_next_ascii(&mut self) -> Result<(), Error> {
        self.consumer_one_ascii_or_all_non_ascii()?;
        Ok(())
    }

    pub fn skip_multiple_ascii_bounded(&mut self, skip_byte_count: usize) {
        let end_ptr = self.pointer + skip_byte_count;
        self.set_pointer(end_ptr);
    }

    pub fn skip_until_ascii_char_or_end(&mut self, c: char) {
        let pos = self.pointer_str.find(c);

        if let Some(pos) = pos {
            self.set_pointer(self.pointer + pos);
        } else {
            self.set_pointer(self.input.len());
        }
    }

    pub fn get_substring(&self, range: &std::ops::Range<usize>) -> &'a OsStr {
        let (_before1, after1) = self.input.split_at(range.start);
        let (middle, _after2) = after1.split_at(range.end - range.start);
        middle
    }

    pub fn look_at_remaining(&self) -> &'a OsStr {
        let (_before, after) = self.input.split_at(self.pointer);
        after
    }

    fn set_pointer(&mut self, new_pointer: usize) {
        self.pointer = new_pointer;
        self.pointer_str = self.look_at_remaining();
    }
}

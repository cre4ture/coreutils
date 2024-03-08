// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//
// spell-checker:ignore (words) splitted FFFD
#![forbid(unsafe_code)]

use std::ffi::OsStr;

use os_str_bytes::OsStrBytesExt;

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
    InvalidEncoding(&'a OsStr),
    ValidChar(char),
}

/// This class makes parsing a OsString char by char more convenient.
///
/// It also allows to capturing of intermediate positions for later splitting.
pub struct StringParser<'a> {
    pub input: &'a OsStr,
    pointer: usize,
    pointer_str: &'a OsStr,
}

impl<'a> StringParser<'a> {
    pub fn new<S: AsRef<OsStr> + ?Sized>(input: &'a S) -> Self {
        let input = input.as_ref();
        Self {
            input,
            pointer: 0,
            pointer_str: input,
        }
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
        let pos = self.pointer_str.find(c);

        if let Some(pos) = pos {
            self.set_pointer(self.pointer + pos);
        } else {
            self.set_pointer(self.input.len());
        }
    }

    pub fn substring(&self, range: &std::ops::Range<usize>) -> &'a OsStr {
        let (_before1, after1) = self.input.split_at(range.start);
        let (middle, _after2) = after1.split_at(range.end - range.start);
        middle
    }

    pub fn peek_remaining(&self) -> &'a OsStr {
        self.pointer_str
    }

    pub fn set_pointer(&mut self, new_pointer: usize) {
        self.pointer = new_pointer;
        let (_before, after) = self.input.split_at(self.pointer);
        self.pointer_str = after;
    }
}

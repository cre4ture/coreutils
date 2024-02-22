// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::{
    borrow::Cow, ffi::{OsStr, OsString}, mem, ops::Deref
};

use crate::{native_int_str::{from_native_int_representation, to_native_int_representation, NativeCharIntT, NativeIntStrT}, string_parser::{Chunk, Error, StringParser}};

/// This class makes parsing and word collection more convenient.
///
/// It manages an "output" buffer that is automatically filled.
/// It provides "skip_one" and "take_one" that focus on
/// working with ASCII separators. Thus they will skip or take
/// all consecutive non-ascii char sequences at once.
pub struct StringExpander<'a> {
    parser: StringParser<'a>,
    output: Vec<NativeCharIntT>,
}

impl<'a> StringExpander<'a> {
    pub fn new(input: &'a NativeIntStrT) -> Self {
        Self {
            parser: StringParser::new(input),
            output: Vec::default(),
        }
    }

    pub fn new_at(input: &'a NativeIntStrT, pos: usize) -> Self {
        Self {
            parser: StringParser::new_at(input, pos),
            output: Vec::default(),
        }
    }

    pub fn get_parser(&self) -> &StringParser<'a> {
        &self.parser
    }

    pub fn get_parser_mut(&mut self) -> &mut StringParser<'a> {
        &mut self.parser
    }

    pub fn peek(&self) -> Result<char, Error> {
        self.parser.peek()
    }

    pub fn skip_one(&mut self) -> Result<(), Error> {
        self.get_parser_mut().consume_one_ascii_or_all_non_ascii()?;
        Ok(())
    }

    pub fn get_peek_position(&self) -> usize {
        self.get_parser().get_peek_position()
    }

    pub fn take_one(&mut self) -> Result<(), Error> {
        let chunks = self.parser.consume_one_ascii_or_all_non_ascii()?;
        for chunk in chunks {
            match chunk {
                Chunk::InvalidEncoding(invalid) => self.output.extend(invalid),
                Chunk::ValidSingleIntChar((_c, ni)) => self.output.push(ni),
            }
        }
        Ok(())
    }

    pub fn put_one_char(&mut self, c: char) {
        let os_str = OsString::from(c.to_string());
        self.put_string(os_str);
    }

    pub fn put_string<S: AsRef<OsStr>>(&mut self, os_str: S) {
        let native = to_native_int_representation(os_str.as_ref());
        self.output.extend(native.deref());
    }

    pub fn take_collected_output(&mut self) -> OsString {
        let out = mem::take(&mut self.output);
        let cow = Cow::Owned(out);
        if let Cow::Owned(vec) = from_native_int_representation(cow) {
            return vec;
        }

        panic!("assert failed: owned in, owned out");
    }
}

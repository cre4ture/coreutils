// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::{ffi::{OsStr, OsString}, mem};

use crate::string_parser::{Chunk, Error, StringParser};




/// This class makes parsing and word collection more convenient.
///
/// It manages an "output" buffer that is automatically filled.
/// It provides "skip_one" and "take_one" that focus on
/// working with ASCII separators. Thus they will skip or take
/// all consecutive non-ascii char sequences at once.
pub struct StringExpander<'a> {
    parser: StringParser<'a>,
    output: OsString,
}



impl<'a> StringExpander<'a> {
    pub fn new<S: AsRef<OsStr> + ?Sized>(input: &'a S) -> Self {
        Self {
            parser: StringParser::new(input),
            output: OsString::default(),
        }
    }

    pub fn new_at(input: &'a OsStr, pos: usize) -> Result<Self, Error> {
        Ok(Self {
            parser: StringParser::new_at(input, pos)?,
            output: OsString::default(),
        })
    }

    pub fn get_parser(&self) -> &StringParser<'a> {
        &self.parser
    }

    pub fn get_parser_mut(&mut self) -> &mut StringParser<'a> {
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

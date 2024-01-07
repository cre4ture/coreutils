// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//
// This file is based on work from Tomasz Miąsko who published it as "shell_words" crate,
// licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
//
//! Process command line according to parsing rules of original GNU env.
//! Even though it looks quite like a POSIX syntax, the original
//! "shell_words" implementation had to be adapted significantly.
//!
//! Apart from the grammar differences, there is a new feature integrated: $VARIABLE expansion by subst crate.
//!
//! [GNU env] https://www.gnu.org/software/coreutils/manual/html_node/env-invocation.html#g_t_002dS_002f_002d_002dsplit_002dstring-syntax

#![forbid(unsafe_code)]

use std::mem;

use crate::parse_error::ParseError;
use crate::raw_string_parser;
use crate::raw_string_parser::is_ascii;
use crate::raw_string_parser::RawStringParser;

/// A map that gives strings from the environment.
#[derive(Debug)]
pub struct EnvWithoutLookupError;

impl<'a> subst::VariableMap<'a> for EnvWithoutLookupError {
    type Value = String;

    fn get(&'a self, key: &str) -> Option<Self::Value> {
        let var = std::env::var(key);
        let result = var.unwrap_or_default();
        Some(result)
    }
}

#[derive(Clone, Copy)]
pub enum State {
    /// Within a delimiter.
    Delimiter,
    /// After backslash, but before starting word.
    DelimiterBackslash,
    /// Within an unquoted word.
    Unquoted,
    /// After backslash in an unquoted word.
    UnquotedBackslash,
    /// Within a single quoted word.
    SingleQuoted,
    /// After backslash inside a double quoted word.
    SingleQuotedBackslash,
    /// Within a double quoted word.
    DoubleQuoted,
    /// After backslash inside a double quoted word.
    DoubleQuotedBackslash,
    /// Inside a comment.
    Comment,
}

const BACKSLASH: u8 = b'\\';
const DOUBLE_QUOTES: u8 = b'\"';
const SINGLE_QUOTES: u8 = b'\'';
const INVALID_UTF8_MARKER: char = '\u{FFFD}';

const REPLACEMENTS: (&[u8], &[u8]) = ("rntfv_#$\"".as_bytes(), "\r\n\t\x0C\x0B #$\"".as_bytes());
static_assertions::const_assert_eq!(REPLACEMENTS.0.len(), REPLACEMENTS.1.len());
const ASCII_WHITESPACE_CHARS: &[u8] = " \t\r\n\x0B\x0C".as_bytes();

pub struct SplitIterator<'a> {
    pub raw_parser: RawStringParser<'a>,
    pub words: Vec<String>,
    pub state: State,
}

impl<'a> SplitIterator<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            raw_parser: RawStringParser::new(s),
            words: Vec::<String>::new(),
            state: State::Delimiter,
        }
    }

    fn skip_one(&mut self) -> Result<(), ParseError> {
        let result = self.raw_parser.skip_one();
        self.map_internal_error(result, "skip_one 1")
    }

    fn take_one(&mut self) -> Result<(), ParseError> {
        let result = self.raw_parser.take_one();
        self.map_internal_error(result, "take_one 1")
    }

    fn get_current_char(&self) -> Option<u8> {
        let result = self.raw_parser.look_at();
        if let Ok(c) = result {
            Some(c)
        } else {
            None
        }
    }

    fn push_ascii_char_to_word(&mut self, c: u8) -> Result<(), ParseError> {
        let result = self.raw_parser.put_one_ascii(c);
        self.map_internal_error(result, "push_ascii_char_to_word 1")
    }

    fn push_word_to_words(&mut self) -> Result<(), ParseError> {
        let result = self.raw_parser.take_collected_output();
        let byte_word = self.map_internal_error(result, "push_word_to_words 1")?;
        self.words.push(byte_word);
        Ok(())
    }

    fn map_internal_error<T, S>(
        &self,
        result: Result<T, raw_string_parser::Error>,
        msg: S,
    ) -> Result<T, ParseError>
    where
        std::string::String: From<S>,
    {
        result.map_err(|e| ParseError::InternalError {
            pos: self.raw_parser.get_look_at_pos(),
            message: String::from(msg),
            sub_err: e,
        })
    }

    fn substitute_variable(&mut self) -> Result<(), ParseError> {
        let remaining_str =
            self.map_internal_error(self.raw_parser.look_at_remaining(), "substitute_variable 1")?;

        let result = subst::substitute_one_step(remaining_str, &EnvWithoutLookupError {});

        match result {
            Ok(Some((value, pos))) => {
                let result = self.raw_parser.put_string_utf8(value.as_str());
                self.map_internal_error(result, "substitute_variable 2")?;
                let result = self.raw_parser.skip_multiple_ascii_bounded(pos);
                self.map_internal_error(result, "substitute_variable 3")?;
                Ok(())
            }
            Ok(None) => self.take_one(), // no variable name, take the $ char
            Err(subst::Error::MissingVariableName(_)) => self.take_one(), // no variable name, take the $ char
            Err(e) => Err(ParseError::ParsingOfVariableNameFailed {
                pos: self.raw_parser.get_look_at_pos(),
                sub_err: e,
            }),
        }
    }

    fn check_and_replace_ascii_escape_code(&mut self, c: u8) -> Result<bool, ParseError> {
        let (from, to) = REPLACEMENTS;
        if let Some(pos) = memchr::memchr(c, from) {
            self.skip_one()?;
            self.push_ascii_char_to_word(*to.get(pos).unwrap())?;
            return Ok(true);
        }

        Ok(false)
    }

    fn make_invalid_sequence_backslash_xin_minus_s(&self, c: u8) -> ParseError {
        let valid_char: char = if is_ascii(c) {
            c.into()
        } else {
            INVALID_UTF8_MARKER
        };
        ParseError::InvalidSequenceBackslashXInMinusS {
            pos: self.raw_parser.get_look_at_pos(),
            c: valid_char,
        }
    }

    pub fn split(&mut self) -> Result<Vec<String>, ParseError> {
        use State::*;

        loop {
            let c = self.get_current_char();
            let _c_char =
                c.map(|c| -> char { char::from_u32(c.into()).unwrap_or(INVALID_UTF8_MARKER) }); // just for debugging session. In release, compiler will remove
            self.state = match self.state {
                Delimiter => match c {
                    None => break,
                    Some(SINGLE_QUOTES) => {
                        self.skip_one()?;
                        SingleQuoted
                    }
                    Some(DOUBLE_QUOTES) => {
                        self.skip_one()?;
                        DoubleQuoted
                    }
                    Some(BACKSLASH) => {
                        self.skip_one()?;
                        DelimiterBackslash
                    }
                    Some(c) if ASCII_WHITESPACE_CHARS.contains(&c) => {
                        self.skip_one()?;
                        Delimiter
                    }
                    Some(b'#') => {
                        self.skip_one()?;
                        Comment
                    }
                    Some(b'$') => {
                        self.substitute_variable()?;
                        self.state
                    }
                    Some(_) => {
                        self.take_one()?;
                        Unquoted
                    }
                },
                DelimiterBackslash => match c {
                    None => {
                        return Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                            pos: self.raw_parser.get_look_at_pos(),
                            quoting: "Delimiter".into(),
                        })
                    }
                    Some(b'_') => {
                        self.skip_one()?;
                        Delimiter
                    }
                    Some(b'\n') => {
                        self.skip_one()?;
                        Delimiter
                    }
                    Some(b'$') | Some(BACKSLASH) | Some(b'#') | Some(SINGLE_QUOTES)
                    | Some(DOUBLE_QUOTES) => {
                        self.take_one()?;
                        Unquoted
                    }
                    Some(b'c') => break,
                    Some(c) if self.check_and_replace_ascii_escape_code(c)? => Unquoted,
                    Some(c) => return Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
                },
                Unquoted => match c {
                    None => {
                        self.push_word_to_words()?;
                        break;
                    }
                    Some(b'$') => {
                        self.substitute_variable()?;
                        self.state
                    }
                    Some(SINGLE_QUOTES) => {
                        self.skip_one()?;
                        SingleQuoted
                    }
                    Some(DOUBLE_QUOTES) => {
                        self.skip_one()?;
                        DoubleQuoted
                    }
                    Some(BACKSLASH) => {
                        self.skip_one()?;
                        UnquotedBackslash
                    }
                    Some(c) if ASCII_WHITESPACE_CHARS.contains(&c) => {
                        self.push_word_to_words()?;
                        self.skip_one()?;
                        Delimiter
                    }
                    Some(_) => {
                        self.take_one()?;
                        Unquoted
                    }
                },
                UnquotedBackslash => match c {
                    None => {
                        return Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                            pos: self.raw_parser.get_look_at_pos(),
                            quoting: "Unquoted".into(),
                        })
                    }
                    Some(b'\n') => {
                        self.skip_one()?;
                        Unquoted
                    }
                    Some(b'_') => {
                        self.skip_one()?;
                        self.push_word_to_words()?;
                        Delimiter
                    }
                    Some(b'c') => {
                        self.push_word_to_words()?;
                        break;
                    }
                    Some(b'$') | Some(BACKSLASH) | Some(SINGLE_QUOTES) | Some(DOUBLE_QUOTES) => {
                        self.take_one()?;
                        Unquoted
                    }
                    Some(c) if self.check_and_replace_ascii_escape_code(c)? => Unquoted,
                    Some(c) => return Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
                },
                SingleQuoted => match c {
                    None => {
                        return Err(ParseError::MissingClosingQuote {
                            pos: self.raw_parser.get_look_at_pos(),
                            c: '\'',
                        })
                    }
                    Some(SINGLE_QUOTES) => {
                        self.skip_one()?;
                        Unquoted
                    }
                    Some(BACKSLASH) => {
                        self.skip_one()?;
                        SingleQuotedBackslash
                    }
                    Some(_) => {
                        self.take_one()?;
                        SingleQuoted
                    }
                },
                SingleQuotedBackslash => match c {
                    None => {
                        return Err(ParseError::MissingClosingQuote {
                            pos: self.raw_parser.get_look_at_pos(),
                            c: '\'',
                        })
                    }
                    Some(b'\n') => {
                        self.skip_one()?;
                        SingleQuoted
                    }
                    Some(SINGLE_QUOTES) | Some(BACKSLASH) => {
                        self.take_one()?;
                        SingleQuoted
                    }
                    Some(c) if REPLACEMENTS.0.contains(&c) => {
                        // See GNU test-suite e11: In single quotes, \t remains as it is.
                        // Comparing with GNU behavior: \a is not accepted and issues an error.
                        // So apparently only known sequences are allowed, even though they are not expanded.... bug of GNU?
                        self.push_ascii_char_to_word(BACKSLASH)?;
                        self.take_one()?;
                        SingleQuoted
                    }
                    Some(c) => return Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
                },
                DoubleQuoted => match c {
                    None => {
                        return Err(ParseError::MissingClosingQuote {
                            pos: self.raw_parser.get_look_at_pos(),
                            c: '"',
                        })
                    }
                    Some(b'$') => {
                        self.substitute_variable()?;
                        self.state
                    }
                    Some(DOUBLE_QUOTES) => {
                        self.skip_one()?;
                        Unquoted
                    }
                    Some(BACKSLASH) => {
                        self.skip_one()?;
                        DoubleQuotedBackslash
                    }
                    Some(_) => {
                        self.take_one()?;
                        DoubleQuoted
                    }
                },
                DoubleQuotedBackslash => match c {
                    None => {
                        return Err(ParseError::MissingClosingQuote {
                            pos: self.raw_parser.get_look_at_pos(),
                            c: '"',
                        })
                    }
                    Some(b'\n') => {
                        self.skip_one()?;
                        DoubleQuoted
                    }
                    Some(DOUBLE_QUOTES) | Some(b'$') | Some(BACKSLASH) => {
                        self.take_one()?;
                        DoubleQuoted
                    }
                    Some(b'c') => {
                        return Err(ParseError::BackslashCNotAllowedInDoubleQuotes {
                            pos: self.raw_parser.get_look_at_pos(),
                        })
                    }
                    Some(c) if self.check_and_replace_ascii_escape_code(c)? => DoubleQuoted,
                    Some(c) => return Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
                },
                Comment => match c {
                    None => break,
                    Some(b'\n') => {
                        self.skip_one()?;
                        Delimiter
                    }
                    Some(_) => {
                        let result = self.raw_parser.skip_until_ascii_char_or_end(b'\n');
                        self.map_internal_error(result, "skip_until_ascii_char_or_end 1")?;
                        Comment
                    }
                },
            };

            if c.is_none() {
                break;
            }
        }

        Ok(mem::take(&mut self.words))
    }
}

pub fn split(s: &str) -> Result<Vec<String>, ParseError> {
    SplitIterator::new(s).split()
}

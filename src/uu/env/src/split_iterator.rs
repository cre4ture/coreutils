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
//! Apart from the grammar differences, there is a new feature integrated: $VARIABLE expansion.
//!
//! [GNU env] <https://www.gnu.org/software/coreutils/manual/html_node/env-invocation.html#g_t_002dS_002f_002d_002dsplit_002dstring-syntax>
// spell-checker:ignore (words) Tomasz Miąsko rntfv FFFD varname

#![forbid(unsafe_code)]

use std::mem;
use std::ops::Range;

use crate::parse_error::ParseError;
use crate::raw_string_parser::is_ascii;
use crate::raw_string_parser::RawStringExpander;
use crate::raw_string_parser::RawStringParser;

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
    pub raw_parser: RawStringExpander<'a>,
    pub words: Vec<String>,
    pub state: State,
}

impl<'a> SplitIterator<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            raw_parser: RawStringExpander::new(s),
            words: Vec::<String>::new(),
            state: State::Delimiter,
        }
    }

    fn skip_one(&mut self) -> Result<(), ParseError> {
        Ok(self.raw_parser.get_parser_mut().skip_one()?)
    }

    fn take_one(&mut self) -> Result<(), ParseError> {
        Ok(self.raw_parser.take_one()?)
    }

    fn get_current_char(&self) -> Option<u8> {
        self.raw_parser.get_parser().look_at().ok()
    }

    fn push_ascii_char_to_word(&mut self, c: u8) -> Result<(), ParseError> {
        Ok(self.raw_parser.put_one_ascii(c)?)
    }

    fn push_word_to_words(&mut self) -> Result<(), ParseError> {
        let word = self.raw_parser.take_collected_output()?;
        self.words.push(word);
        Ok(())
    }

    fn check_variable_name_start(&self) -> Result<(), ParseError> {
        if let Some(c) = self.get_current_char() {
            if c.is_ascii_digit() {
                return Err(ParseError::ParsingOfVariableNameFailed {
                    pos: self.raw_parser.get_parser().get_look_at_pos(),
                    msg: format!("Unexpected character: '{}', expected variable name must not start with 0..9", c as char) });
            }
        }
        Ok(())
    }

    fn get_parser(&self) -> &RawStringParser<'a> {
        self.raw_parser.get_parser()
    }

    fn get_parser_mut(&mut self) -> &mut RawStringParser<'a> {
        self.raw_parser.get_parser_mut()
    }

    fn parse_braced_variable_name(&mut self) -> Result<(&'a str, Option<&'a str>), ParseError> {
        let pos_start = self.get_parser().get_look_at_pos();

        self.check_variable_name_start()?;

        let (varname_end, default_end);
        loop {
            match self.get_current_char() {
                None => {
                    return Err(ParseError::ParsingOfVariableNameFailed {
                        pos: self.get_parser().get_look_at_pos(), msg: "Missing closing brace".into() })
                },
                Some(c) if !c.is_ascii() || c.is_ascii_alphanumeric() || c == b'_' => {
                    self.skip_one()?;
                }
                Some(b':') => {
                    varname_end = self.get_parser().get_look_at_pos();
                    loop {
                        match self.get_current_char() {
                            None => {
                                return Err(ParseError::ParsingOfVariableNameFailed {
                                    pos: self.get_parser().get_look_at_pos(),
                                    msg: "Missing closing brace after default value".into() })
                            },
                            Some(b'}') => {
                                default_end = Some(self.get_parser().get_look_at_pos());
                                self.skip_one()?;
                                break
                            },
                            Some(_) => {
                                self.skip_one()?;
                            },
                        }
                    }
                    break;
                },
                Some(b'}') => {
                    varname_end = self.get_parser().get_look_at_pos();
                    default_end = None;
                    self.skip_one()?;
                    break;
                },
                Some(c) => {
                    return Err(ParseError::ParsingOfVariableNameFailed {
                        pos: self.get_parser().get_look_at_pos(),
                        msg: format!("Unexpected character: '{}', expected a closing brace ('}}') or colon (':')", c as char)
                    })
                },
            };
        }

        let default = if let Some(default_end) = default_end {
            Some(self.get_parser().get_substring(&Range {
                start: varname_end + 1,
                end: default_end,
            })?)
        } else {
            None
        };

        let varname = self.get_parser().get_substring(&Range {
            start: pos_start,
            end: varname_end,
        })?;

        Ok((varname, default))
    }

    fn parse_unbraced_variable_name(&mut self) -> Result<&str, ParseError> {
        let pos_start = self.get_parser().get_look_at_pos();

        self.check_variable_name_start()?;

        loop {
            match self.get_current_char() {
                None => break,
                Some(c) if c.is_ascii_alphanumeric() || c == b'_' => {
                    self.get_parser_mut().skip_one()?;
                }
                Some(_) => break,
            };
        }

        let pos_end = self.get_parser().get_look_at_pos();

        if pos_end == pos_start {
            return Err(ParseError::ParsingOfVariableNameFailed {
                pos: pos_start,
                msg: "Missing variable name".into(),
            });
        }

        Ok(self.get_parser().get_substring(&Range {
            start: pos_start,
            end: pos_end,
        })?)
    }

    fn substitute_variable(&mut self) -> Result<(), ParseError> {
        self.get_parser_mut().skip_one()?;

        let (name, default) = match self.get_current_char() {
            None => {
                return Err(ParseError::ParsingOfVariableNameFailed {
                    pos: self.get_parser().get_look_at_pos(),
                    msg: "missing variable name".into(),
                })
            }
            Some(b'{') => {
                self.skip_one()?;
                self.parse_braced_variable_name()?
            }
            Some(_) => (self.parse_unbraced_variable_name()?, None),
        };

        let value = std::env::var(name).ok();
        match (&value, default) {
            (None, None) => {} // do nothing, just replace it with ""
            (Some(value), _) => {
                self.raw_parser.put_string_utf8(value)?;
            }
            (None, Some(default)) => {
                self.raw_parser.put_string_utf8(default)?;
            }
        };

        Ok(())
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
            pos: self.raw_parser.get_parser().get_look_at_pos(),
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
                        Unquoted
                    }
                    Some(_) => {
                        self.take_one()?;
                        Unquoted
                    }
                },
                DelimiterBackslash => match c {
                    None => {
                        return Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                            pos: self.get_parser().get_look_at_pos(),
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
                            pos: self.get_parser().get_look_at_pos(),
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
                            pos: self.get_parser().get_look_at_pos(),
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
                            pos: self.get_parser().get_look_at_pos(),
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
                            pos: self.get_parser().get_look_at_pos(),
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
                            pos: self.get_parser().get_look_at_pos(),
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
                            pos: self.get_parser().get_look_at_pos(),
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
                        self.get_parser_mut().skip_until_ascii_char_or_end(b'\n')?;
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

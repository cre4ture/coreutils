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

use std::ffi::OsStr;
use std::ffi::OsString;
use std::mem;
use std::ops::Range;

use crate::parse_error::ParseError;
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

const BACKSLASH: char = '\\';
const DOUBLE_QUOTES: char = '\"';
const SINGLE_QUOTES: char = '\'';

const REPLACEMENTS: [(char, char); 9] = [
    ('r', '\r'),
    ('n', '\n'),
    ('t', '\t'),
    ('f', '\x0C'),
    ('v', '\x0B'),
    ('_', ' '),
    ('#', '#'),
    ('$', '$'),
    ('"', '"'),
];

const ASCII_WHITESPACE_CHARS: [char; 6] = [' ', '\t', '\r', '\n', '\x0B', '\x0C'];

pub struct SplitIterator<'a> {
    pub raw_parser: RawStringExpander<'a>,
    pub words: Vec<OsString>,
}

impl<'a> SplitIterator<'a> {

    pub fn new<S: AsRef<OsStr> + ?Sized>(s: &'a S) -> Self {
        Self {
            raw_parser: RawStringExpander::new(s.as_ref()),
            words: Vec::<OsString>::new(),
        }
    }

    fn skip_one(&mut self) -> Result<(), ParseError> {
        self.raw_parser.get_parser_mut().skip_till_next_ascii()?;
        Ok(())
    }

    fn take_one(&mut self) -> Result<(), ParseError> {
        Ok(self.raw_parser.take_one()?)
    }

    fn get_current_char(&self) -> Option<char> {
        self.raw_parser.get_parser().look_at().ok()
    }

    fn push_ascii_char_to_word(&mut self, c: char) -> Result<(), ParseError> {
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

    fn parse_braced_variable_name(&mut self) -> Result<(&'a OsStr, Option<&'a OsStr>), ParseError> {
        let pos_start = self.get_parser().get_look_at_pos();

        self.check_variable_name_start()?;

        let (varname_end, default_end);
        loop {
            match self.get_current_char() {
                None => {
                    return Err(ParseError::ParsingOfVariableNameFailed {
                        pos: self.get_parser().get_look_at_pos(), msg: "Missing closing brace".into() })
                },
                Some(c) if !c.is_ascii() || c.is_ascii_alphanumeric() || c == '_' => {
                    self.skip_one()?;
                }
                Some(':') => {
                    varname_end = self.get_parser().get_look_at_pos();
                    loop {
                        match self.get_current_char() {
                            None => {
                                return Err(ParseError::ParsingOfVariableNameFailed {
                                    pos: self.get_parser().get_look_at_pos(),
                                    msg: "Missing closing brace after default value".into() })
                            },
                            Some('}') => {
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
                Some('}') => {
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

    fn parse_unbraced_variable_name(&mut self) -> Result<&OsStr, ParseError> {
        let pos_start = self.get_parser().get_look_at_pos();

        self.check_variable_name_start()?;

        loop {
            match self.get_current_char() {
                None => break,
                Some(c) if c.is_ascii_alphanumeric() || c == '_' => {
                    self.skip_one()?;
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
        self.skip_one()?;

        let (name, default) = match self.get_current_char() {
            None => {
                return Err(ParseError::ParsingOfVariableNameFailed {
                    pos: self.get_parser().get_look_at_pos(),
                    msg: "missing variable name".into(),
                })
            }
            Some('{') => {
                self.skip_one()?;
                self.parse_braced_variable_name()?
            }
            Some(_) => (self.parse_unbraced_variable_name()?, None),
        };

        let value = std::env::var_os(name);
        match (&value, default) {
            (None, None) => {} // do nothing, just replace it with ""
            (Some(value), _) => {
                self.raw_parser.put_string(value)?;
            }
            (None, Some(default)) => {
                self.raw_parser.put_string(default)?;
            }
        };

        Ok(())
    }

    fn check_and_replace_ascii_escape_code(&mut self, c: char) -> Result<bool, ParseError> {
        if let Some(replace) = REPLACEMENTS.iter().find(|&x| x.0 == c) {
            self.skip_one()?;
            self.push_ascii_char_to_word(replace.1)?;
            return Ok(true);
        }

        Ok(false)
    }

    fn make_invalid_sequence_backslash_xin_minus_s(&self, c: char) -> ParseError {
        ParseError::InvalidSequenceBackslashXInMinusS {
            pos: self.raw_parser.get_parser().get_look_at_pos(),
            c,
        }
    }

    fn split_root(&mut self) -> Result<(), ParseError> {
        loop {
            match self.split_delimiter() {
                Err(ParseError::ContinueWithDelimiter) => {}
                Err(ParseError::ReachedEnd) => return Ok(()),
                result => return result,
            }
        }
    }

    fn split_delimiter(&mut self) -> Result<(), ParseError> {
        loop {
            match self.get_current_char() {
                None => return Ok(()),
                Some(SINGLE_QUOTES) => {
                    // don't consume char!
                    self.split_unquoted()?;
                }
                Some(DOUBLE_QUOTES) => {
                    // don't consume char!
                    self.split_unquoted()?;
                }
                Some(BACKSLASH) => {
                    self.skip_one()?;
                    self.split_delimiter_backslash()?;
                }
                Some(c) if ASCII_WHITESPACE_CHARS.contains(&c) => {
                    self.skip_one()?;
                }
                Some('#') => {
                    self.skip_one()?;
                    self.split_comment()?;
                }
                Some('$') => {
                    // don't consume char!
                    self.split_unquoted()?;
                }
                Some(_) => {
                    // don't consume char!
                    self.split_unquoted()?;
                }
            }
        }
    }

    fn split_delimiter_backslash(&mut self) -> Result<(), ParseError> {
        match self.get_current_char() {
            None => Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                pos: self.get_parser().get_look_at_pos(),
                quoting: "Delimiter".into(),
            }),
            Some('_') | Some('\n') => {
                self.skip_one()?;
                Ok(())
            }
            Some('$') | Some(BACKSLASH) | Some('#') | Some(SINGLE_QUOTES)
            | Some(DOUBLE_QUOTES) => {
                self.take_one()?;
                self.split_unquoted()
            }
            Some('c') => Err(ParseError::ReachedEnd),
            Some(c) if self.check_and_replace_ascii_escape_code(c)? => self.split_unquoted(),
            Some(c) => Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
        }
    }

    fn split_unquoted(&mut self) -> Result<(), ParseError> {
        loop {
            match self.get_current_char() {
                None => {
                    self.push_word_to_words()?;
                    return Err(ParseError::ReachedEnd);
                }
                Some('$') => {
                    self.substitute_variable()?;
                }
                Some(SINGLE_QUOTES) => {
                    self.skip_one()?;
                    self.split_single_quoted()?;
                }
                Some(DOUBLE_QUOTES) => {
                    self.skip_one()?;
                    self.split_double_quoted()?;
                }
                Some(BACKSLASH) => {
                    self.skip_one()?;
                    self.split_unquoted_backslash()?;
                }
                Some(c) if ASCII_WHITESPACE_CHARS.contains(&c) => {
                    self.push_word_to_words()?;
                    self.skip_one()?;
                    return Ok(());
                }
                Some(_) => {
                    self.take_one()?;
                }
            }
        }
    }

    fn split_unquoted_backslash(&mut self) -> Result<(), ParseError> {
        match self.get_current_char() {
            None => Err(ParseError::InvalidBackslashAtEndOfStringInMinusS {
                pos: self.get_parser().get_look_at_pos(),
                quoting: "Unquoted".into(),
            }),
            Some('\n') => {
                self.skip_one()?;
                Ok(())
            }
            Some('_') => {
                self.skip_one()?;
                self.push_word_to_words()?;
                Err(ParseError::ContinueWithDelimiter)
            }
            Some('c') => {
                self.push_word_to_words()?;
                Err(ParseError::ReachedEnd)
            }
            Some('$') | Some(BACKSLASH) | Some(SINGLE_QUOTES) | Some(DOUBLE_QUOTES) => {
                self.take_one()?;
                Ok(())
            }
            Some(c) if self.check_and_replace_ascii_escape_code(c)? => Ok(()),
            Some(c) => Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
        }
    }

    fn split_single_quoted(&mut self) -> Result<(), ParseError> {
        loop {
            match self.get_current_char() {
                None => {
                    return Err(ParseError::MissingClosingQuote {
                        pos: self.get_parser().get_look_at_pos(),
                        c: '\'',
                    })
                }
                Some(SINGLE_QUOTES) => {
                    self.skip_one()?;
                    return Ok(());
                }
                Some(BACKSLASH) => {
                    self.skip_one()?;
                    self.split_single_quoted_backslash()?;
                }
                Some(_) => {
                    self.take_one()?;
                }
            }
        }
    }

    fn split_single_quoted_backslash(&mut self) -> Result<(), ParseError> {
        match self.get_current_char() {
            None => Err(ParseError::MissingClosingQuote {
                pos: self.get_parser().get_look_at_pos(),
                c: '\'',
            }),
            Some('\n') => {
                self.skip_one()?;
                Ok(())
            }
            Some(SINGLE_QUOTES) | Some(BACKSLASH) => {
                self.take_one()?;
                Ok(())
            }
            Some(c) if REPLACEMENTS.iter().any(|&x| x.0 == c) => {
                // See GNU test-suite e11: In single quotes, \t remains as it is.
                // Comparing with GNU behavior: \a is not accepted and issues an error.
                // So apparently only known sequences are allowed, even though they are not expanded.... bug of GNU?
                self.push_ascii_char_to_word(BACKSLASH)?;
                self.take_one()?;
                Ok(())
            }
            Some(c) => Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
        }
    }

    fn split_double_quoted(&mut self) -> Result<(), ParseError> {
        loop {
            match self.get_current_char() {
                None => {
                    return Err(ParseError::MissingClosingQuote {
                        pos: self.get_parser().get_look_at_pos(),
                        c: '"',
                    })
                }
                Some('$') => {
                    self.substitute_variable()?;
                }
                Some(DOUBLE_QUOTES) => {
                    self.skip_one()?;
                    return Ok(());
                }
                Some(BACKSLASH) => {
                    self.skip_one()?;
                    self.split_double_quoted_backslash()?;
                }
                Some(_) => {
                    self.take_one()?;
                }
            }
        }
    }

    fn split_double_quoted_backslash(&mut self) -> Result<(), ParseError> {
        match self.get_current_char() {
            None => Err(ParseError::MissingClosingQuote {
                pos: self.get_parser().get_look_at_pos(),
                c: '"',
            }),
            Some('\n') => {
                self.skip_one()?;
                Ok(())
            }
            Some(DOUBLE_QUOTES) | Some('$') | Some(BACKSLASH) => {
                self.take_one()?;
                Ok(())
            }
            Some('c') => Err(ParseError::BackslashCNotAllowedInDoubleQuotes {
                pos: self.get_parser().get_look_at_pos(),
            }),
            Some(c) if self.check_and_replace_ascii_escape_code(c)? => Ok(()),
            Some(c) => Err(self.make_invalid_sequence_backslash_xin_minus_s(c)),
        }
    }

    fn split_comment(&mut self) -> Result<(), ParseError> {
        loop {
            match self.get_current_char() {
                None => return Err(ParseError::ReachedEnd),
                Some('\n') => {
                    self.skip_one()?;
                    return Ok(());
                }
                Some(_) => {
                    self.get_parser_mut().skip_until_ascii_char_or_end('\n')?;
                }
            }
        }
    }

    pub fn split(&mut self) -> Result<Vec<OsString>, ParseError> {
        self.split_root()?;
        Ok(mem::take(&mut self.words))
    }
}

pub fn split(s: &OsStr) -> Result<Vec<OsString>, ParseError> {
    SplitIterator::new(s).split()
}

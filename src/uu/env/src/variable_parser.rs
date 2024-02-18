// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//
// This file is based on work from Tomasz Miąsko who published it as "shell_words" crate,
// licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.

use std::{ffi::OsStr, ops::Range};

use crate::{parse_error::ParseError, raw_string_parser::RawStringParser};

pub struct VariableParser<'a, 'b>
    where 'a : 'b
{
    pub parser: &'b mut RawStringParser<'a>
}

impl<'a, 'b> VariableParser<'a, 'b> {

    fn get_current_char(&self) -> Option<char> {
        self.parser.look_at().ok()
    }

    fn check_variable_name_start(&self) -> Result<(), ParseError> {
        if let Some(c) = self.get_current_char() {
            if c.is_ascii_digit() {
                return Err(ParseError::ParsingOfVariableNameFailed {
                    pos: self.parser.get_look_at_pos(),
                    msg: format!("Unexpected character: '{}', expected variable name must not start with 0..9", c) });
            }
        }
        Ok(())
    }

    fn skip_one(&mut self) -> Result<(), ParseError> {
        self.parser.consume_one()?;
        Ok(())
    }

    fn parse_braced_variable_name(&mut self) -> Result<(&'a OsStr, Option<&'a OsStr>), ParseError> {
        let pos_start = self.parser.get_look_at_pos();

        self.check_variable_name_start()?;

        let (varname_end, default_end);
        loop {
            match self.get_current_char() {
                None => {
                    return Err(ParseError::ParsingOfVariableNameFailed {
                        pos: self.parser.get_look_at_pos(), msg: "Missing closing brace".into() })
                },
                Some(c) if !c.is_ascii() || c.is_ascii_alphanumeric() || c == '_' => {
                    self.skip_one()?;
                }
                Some(':') => {
                    varname_end = self.parser.get_look_at_pos();
                    loop {
                        match self.get_current_char() {
                            None => {
                                return Err(ParseError::ParsingOfVariableNameFailed {
                                    pos: self.parser.get_look_at_pos(),
                                    msg: "Missing closing brace after default value".into() })
                            },
                            Some('}') => {
                                default_end = Some(self.parser.get_look_at_pos());
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
                    varname_end = self.parser.get_look_at_pos();
                    default_end = None;
                    self.skip_one()?;
                    break;
                },
                Some(c) => {
                    return Err(ParseError::ParsingOfVariableNameFailed {
                        pos: self.parser.get_look_at_pos(),
                        msg: format!("Unexpected character: '{}', expected a closing brace ('}}') or colon (':')", c)
                    })
                },
            };
        }

        let default = if let Some(default_end) = default_end {
            Some(self.parser.get_substring(&Range {
                start: varname_end + 1,
                end: default_end,
            }))
        } else {
            None
        };

        let varname = self.parser.get_substring(&Range {
            start: pos_start,
            end: varname_end,
        });

        Ok((varname, default))
    }

    fn parse_unbraced_variable_name(&mut self) -> Result<&'a OsStr, ParseError> {
        let pos_start = self.parser.get_look_at_pos();

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

        let pos_end = self.parser.get_look_at_pos();

        if pos_end == pos_start {
            return Err(ParseError::ParsingOfVariableNameFailed {
                pos: pos_start,
                msg: "Missing variable name".into(),
            });
        }

        Ok(self.parser.get_substring(&Range {
            start: pos_start,
            end: pos_end,
        }))
    }

    pub fn parse_variable(&mut self) -> Result<(&'a OsStr, Option<&'a OsStr>), ParseError> {
        self.skip_one()?;

        let (name, default) = match self.get_current_char() {
            None => {
                return Err(ParseError::ParsingOfVariableNameFailed {
                    pos: self.parser.get_look_at_pos(),
                    msg: "missing variable name".into(),
                })
            }
            Some('{') => {
                self.skip_one()?;
                self.parse_braced_variable_name()?
            }
            Some(_) => (self.parse_unbraced_variable_name()?, None),
        };

        Ok((name, default))
    }
}

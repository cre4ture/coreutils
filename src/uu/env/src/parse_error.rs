// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::fmt;

use crate::raw_string_parser;

/// An error returned when string arg splitting fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    MissingClosingQuote {
        pos: usize,
        c: char,
    },
    InvalidBackslashAtEndOfStringInMinusS {
        pos: usize,
        quoting: String,
    },
    BackslashCNotAllowedInDoubleQuotes {
        pos: usize,
    },
    InvalidSequenceBackslashXInMinusS {
        pos: usize,
        c: char,
    },
    ParsingOfVariableNameFailed {
        pos: usize,
        sub_err: subst::Error,
    },
    InternalError {
        pos: usize,
        message: String,
        sub_err: raw_string_parser::Error,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(format!("{:?}", self).as_str())
    }
}

impl std::error::Error for ParseError {}

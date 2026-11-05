// Copyright 2026 AsterSQL.
use std::{error::Error, fmt};
/// Explicit radices used by PD's decimal and legacy hexadecimal persistence formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoUintBase {
    Decimal,
    Hexadecimal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoUintErrorKind {
    Syntax,
    Range,
}
/// Retains Go NumError input bytes so public error strings do not lose protocol context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoParseUintError {
    pub input: Vec<u8>,
    pub kind: GoUintErrorKind,
}
impl fmt::Display for GoParseUintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "strconv.ParseUint: parsing {}: {}",
            go_quote_bytes(&self.input, false),
            match self.kind {
                GoUintErrorKind::Syntax => "invalid syntax",
                GoUintErrorKind::Range => "value out of range",
            }
        )
    }
}
impl Error for GoParseUintError {}
/// Matches explicit-base Go ParseUint(...,64), including saturated range values and error order.
pub fn parse_uint64_with_base(input: &[u8], base: GoUintBase) -> (u64, Option<GoParseUintError>) {
    let error = |kind| GoParseUintError {
        input: input.to_vec(),
        kind,
    };
    if input.is_empty() {
        return (0, Some(error(GoUintErrorKind::Syntax)));
    }
    let radix = match base {
        GoUintBase::Decimal => 10,
        GoUintBase::Hexadecimal => 16,
    };
    let mut value = 0_u64;
    for &byte in input {
        let digit = match byte {
            b'0'..=b'9' => u64::from(byte - b'0'),
            b'a'..=b'f' => u64::from(byte - b'a' + 10),
            b'A'..=b'F' => u64::from(byte - b'A' + 10),
            _ => return (0, Some(error(GoUintErrorKind::Syntax))),
        };
        if digit >= radix {
            return (0, Some(error(GoUintErrorKind::Syntax)));
        }
        match value.checked_mul(radix).and_then(|v| v.checked_add(digit)) {
            Some(next) => value = next,
            None => return (u64::MAX, Some(error(GoUintErrorKind::Range))),
        }
    }
    (value, None)
}
/// Go-style quoted byte strings; ascii selects fmt's %+q rather than %q rendering.
pub fn go_quote_bytes(bytes: &[u8], ascii: bool) -> String {
    let mut out = String::from("\"");
    let mut cursor = 0;
    while cursor < bytes.len() {
        let tail = &bytes[cursor..];
        let (valid, length) = match std::str::from_utf8(tail) {
            Ok(s) => (s, s.len()),
            Err(e) => (
                std::str::from_utf8(&tail[..e.valid_up_to()]).unwrap(),
                e.valid_up_to(),
            ),
        };
        for c in valid.chars() {
            use std::fmt::Write;
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{7}' => out.push_str("\\a"),
                '\u{8}' => out.push_str("\\b"),
                '\u{b}' => out.push_str("\\v"),
                '\u{c}' => out.push_str("\\f"),
                c if (' '..='~').contains(&c)
                    || (!ascii
                        && !c.is_control()
                        && !c.escape_debug().to_string().starts_with("\\u{")) =>
                {
                    out.push(c)
                }
                c => {
                    let n = c as u32;
                    if n < 128 {
                        write!(out, "\\x{n:02x}").unwrap()
                    } else if n <= 0xffff {
                        write!(out, "\\u{n:04x}").unwrap()
                    } else {
                        write!(out, "\\U{n:08x}").unwrap()
                    }
                }
            }
        }
        cursor += length;
        if cursor < bytes.len() {
            use std::fmt::Write;
            write!(out, "\\x{:02x}", bytes[cursor]).unwrap();
            cursor += 1;
        }
    }
    out.push('"');
    out
}

// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

use std::{error::Error, fmt};

/// A reusable definition for one stable PD domain error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorTemplate {
    code: &'static str,
    message: &'static str,
}

impl ErrorTemplate {
    /// Creates a template with its stable RFC code and message template.
    pub const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }

    /// Returns the stable RFC error code.
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the unformatted Go-compatible message template.
    pub const fn template(&self) -> &'static str {
        self.message
    }

    /// Instantiates the template with a stable snapshot of its arguments.
    pub fn instantiate<I, S>(self, args: I) -> TypedError
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        TypedError {
            definition: self,
            args: args.into_iter().map(Into::into).collect(),
            source: None,
        }
    }

    /// Instantiates the template and attaches the error that caused it.
    pub fn with_source<I, S, E>(self, args: I, source: E) -> TypedError
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        E: Error + Send + Sync + 'static,
    {
        let mut error = self.instantiate(args);
        error.source = Some(Box::new(source));
        error
    }
}

/// An instantiated domain error with stable comparison semantics.
#[derive(Debug)]
pub struct TypedError {
    definition: ErrorTemplate,
    args: Vec<String>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl TypedError {
    /// Returns the stable RFC error code.
    pub const fn code(&self) -> &'static str {
        self.definition.code()
    }

    /// Returns the unformatted message template.
    pub const fn template(&self) -> &'static str {
        self.definition.template()
    }

    /// Returns the captured formatting arguments.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    fn message(&self) -> String {
        let mut message = String::with_capacity(self.template().len());
        let mut characters = self.template().chars();
        let mut arguments = self.args.iter();

        while let Some(character) = characters.next() {
            if character != '%' {
                message.push(character);
                continue;
            }

            match characters.next() {
                Some('%') => message.push('%'),
                Some(verb @ ('s' | 'v' | 'd')) => {
                    if let Some(argument) = arguments.next() {
                        message.push_str(argument);
                    } else {
                        message.push('%');
                        message.push(verb);
                    }
                }
                Some(character) => {
                    message.push('%');
                    message.push(character);
                }
                None => message.push('%'),
            }
        }

        message
    }
}

impl fmt::Display for TypedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}]{}", self.code(), self.message())?;
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl Error for TypedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl PartialEq for TypedError {
    fn eq(&self, other: &Self) -> bool {
        self.code() == other.code()
            && self.args == other.args
            && source_chains_equal(self.source(), other.source())
    }
}

impl Eq for TypedError {}

fn source_chains_equal(
    mut left: Option<&(dyn Error + 'static)>,
    mut right: Option<&(dyn Error + 'static)>,
) -> bool {
    loop {
        match (left, right) {
            (Some(left_error), Some(right_error)) => {
                if left_error.to_string() != right_error.to_string() {
                    return false;
                }
                left = left_error.source();
                right = right_error.source();
            }
            (None, None) => return true,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod error_test;

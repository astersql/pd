// Copyright 2026 AsterSQL.
//! Protocol-independent store label validation from config/util.go.

/// Validates label keys and values in input order, returning the first error.
/// Empty values mark labels for deletion and are valid. Borrowed pairs let
/// adapters use their own label types without introducing a protocol dependency.
pub fn validate_labels<'a>(
    labels: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), String> {
    for (key, value) in labels {
        super::validate_label_key(key)?;
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
        {
            return Err(format!(
                "{value} does not match format \"^[-A-Za-z0-9_./]*$\""
            ));
        }
    }
    Ok(())
}

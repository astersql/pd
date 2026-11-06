// Copyright 2026 AsterSQL.
/// Shared RE2-compatible parsing for HTTP label and placement selectors.
pub fn go_pattern(pattern: &str) -> Result<regex::Regex, String> {
    // Go's Perl classes and word boundaries are ASCII, while its literal
    // case folding remains Unicode. Quoted literals are accepted by RE2.
    let mut translated = String::new();
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    let mut class_first = false;
    while let Some(c) = chars.next() {
        if c == '[' {
            if in_class && chars.peek() != Some(&':') {
                translated.push_str("\\[");
                class_first = false;
                continue;
            }
            if in_class {
                // POSIX classes retain their own closing bracket.
                let mut posix = String::from("[");
                while let Some(c) = chars.next() {
                    posix.push(c);
                    if c == ']' {
                        break;
                    }
                }
                translated.push_str(&posix);
                class_first = false;
                continue;
            }
            in_class = true;
            class_first = true;
            translated.push(c);
            continue;
        }
        if in_class && c != '\\' {
            if c == ']' && !class_first {
                in_class = false;
                translated.push(c);
                continue;
            }
            if c == '^' && class_first {
                translated.push(c);
                continue;
            }
            class_first = false;
            // RE2 has no Rust character-class intersection or subtraction.
            if matches!(c, '&' | '~' | '[' | ']') {
                translated.push('\\');
            }
            translated.push(c);
            continue;
        }
        if c == '{' && !in_class {
            let tail: String = chars.clone().take_while(|c| *c != '}').collect();
            let parts: Vec<_> = tail.split(',').collect();
            let valid = chars.clone().nth(tail.chars().count()) == Some('}')
                && parts.len() <= 2
                && !parts[0].is_empty()
                && parts.iter().all(|p| {
                    p.bytes().all(|b| b.is_ascii_digit()) && !(p.len() > 1 && p.starts_with('0'))
                });
            if valid {
                let bounds: Vec<_> = parts
                    .iter()
                    .filter(|p| !p.is_empty())
                    .map(|p| p.parse::<u64>().unwrap_or(u64::MAX))
                    .collect();
                if bounds.iter().any(|n| *n > 1000) || bounds.len() == 2 && bounds[0] > bounds[1] {
                    return Err(format!(
                        "error parsing regexp: invalid repeat count: `{{{tail}}}`"
                    ));
                }
                translated.push_str(&format!("{{{tail}}}"));
                for _ in 0..=tail.chars().count() {
                    chars.next();
                }
            } else {
                translated.push_str("\\{");
            }
            continue;
        }
        if c != '\\' {
            translated.push(c);
            continue;
        }
        let Some(c) = chars.next() else {
            return Err("error parsing regexp: trailing backslash at end of expression: ``".into());
        };
        if in_class {
            class_first = false;
        }
        if ('0'..='7').contains(&c) {
            if c != '0' && !chars.peek().is_some_and(|c| ('0'..='7').contains(c)) {
                return Err(format!(
                    "error parsing regexp: invalid escape sequence: `\\{c}`"
                ));
            }
            let mut value = c as u32 - '0' as u32;
            for _ in 0..2 {
                if let Some(c) = chars.peek().filter(|c| ('0'..='7').contains(*c)).copied() {
                    chars.next();
                    value = value * 8 + c as u32 - '0' as u32;
                } else {
                    break;
                }
            }
            translated.push_str(&format!("\\x{{{value:x}}}"));
            continue;
        }
        if matches!(c, 'u' | 'U') {
            return Err(format!(
                "error parsing regexp: invalid escape sequence: `\\{c}`"
            ));
        }
        if c.is_ascii() && !c.is_ascii_alphanumeric() {
            translated.push_str(&regex::escape(&c.to_string()));
            continue;
        }

        if c == 'Q' {
            let mut literal = String::new();
            while let Some(c) = chars.next() {
                if c == '\\' && chars.peek() == Some(&'E') {
                    chars.next();
                    break;
                }
                literal.push(c);
            }
            translated.push_str(&regex::escape(&literal));
            continue;
        }
        match c {
            'd' => translated.push_str("[0-9]"),
            'D' => translated.push_str("[^0-9]"),
            'w' => translated.push_str("[A-Za-z0-9_]"),
            'W' => translated.push_str("[^A-Za-z0-9_]"),
            's' => translated.push_str("[\\t\\n\\f\\r ]"),
            'S' => translated.push_str("[^\\t\\n\\f\\r ]"),
            'b' => translated.push_str("(?-u:\\b)"),
            'B' => translated.push_str("(?-u:\\B)"),
            _ => {
                translated.push('\\');
                translated.push(c);
            }
        }
    }
    regex::Regex::new(&translated).map_err(|e| {
        let text = e.to_string();
        let lines: Vec<_> = text.lines().collect();
        let marker = lines.get(2).copied().unwrap_or("");
        let start = marker.find('^').unwrap_or(4).saturating_sub(4);
        let width = marker.bytes().filter(|b| *b == b'^').count();
        let span: String = translated.chars().skip(start).take(width).collect();
        let error = |kind: &str, expr: &str| format!("error parsing regexp: {kind}: `{expr}`");
        if text.contains("missing expression") {
            error("missing argument to repetition operator", &span)
        } else if text.contains("unrecognized escape sequence")
            || text.contains("backreferences are not supported")
        {
            error("invalid escape sequence", &span)
        } else if text.contains("look-around") || text.contains("unrecognized flag") {
            error("invalid or unsupported Perl syntax", &span)
        } else if text.contains("invalid character class range") {
            error("invalid character class range", &span)
        } else if text.contains("unclosed character class") {
            format!(
                "error parsing regexp: missing closing ]: `{}`",
                &pattern[pattern.rfind('[').unwrap_or(0)..]
            )
        } else if text.contains("unclosed group") {
            format!("error parsing regexp: missing closing ): `{pattern}`")
        } else if text.contains("unopened group") {
            format!("error parsing regexp: unexpected ): `{pattern}`")
        } else {
            text
        }
    })
}

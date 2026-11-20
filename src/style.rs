//! Shared, conservative text helpers. None of these expand RPM macros.
use crate::rpm;

pub(crate) fn section_word(line: &str) -> &str {
    let rest = line.trim_start().strip_prefix('%').unwrap_or("");
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Split a tag into borrowed key (without colon), value, and value byte offset.
pub(crate) fn split_tag_line(line: &str) -> Option<(&str, &str, usize)> {
    let bytes = line.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let mut index = 0;
    while bytes
        .get(index)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        index += 1;
    }
    if bytes.get(index) == Some(&b'(') {
        let end = line[index + 1..].find(')')? + index + 1;
        index = end + 1;
    }
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    let rest = &line[index + 1..];
    let value = rest.trim_start();
    Some((
        &line[..index],
        value.trim_end(),
        index + 1 + rest.len() - value.len(),
    ))
}

pub(crate) fn is_section(line: &str) -> bool {
    let word = section_word(line);
    rpm::BODY_SECTIONS.contains(&word) || matches!(word, "package" | "autochangelog" | "conf")
}

pub(crate) fn preamble_end(lines: &[&str]) -> usize {
    lines
        .iter()
        .position(|line| is_section(line))
        .unwrap_or(lines.len())
}

pub(crate) fn executable_macro(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with('%') && !rpm::DEFINITION_DIRECTIVES.contains(&section_word(line))
}

fn conditional(line: &str) -> bool {
    rpm::CONDITIONAL_DIRECTIVES.contains(&section_word(line))
}

/// Executable macros preceding tags and conditionals are unsafe. A trailing
/// macro block is safe provided it remains after all explicitly written tags.
pub(crate) fn unsafe_preamble(lines: &[&str]) -> Option<usize> {
    let end = preamble_end(lines);
    let last_tag = lines[..end]
        .iter()
        .rposition(|line| crate::format::split_tag(line).is_some());
    lines[..end].iter().enumerate().find_map(|(index, line)| {
        (conditional(line) || (executable_macro(line) && last_tag.is_some_and(|last| index < last)))
            .then_some(index)
    })
}

pub(crate) fn tag_order(tag: &str) -> (usize, usize, usize) {
    let name = tag.trim_end_matches(':').split('(').next().unwrap_or(tag);
    for (index, prefix) in rpm::TAG_GROUPS[rpm::SOURCE_GROUP].iter().enumerate() {
        if name
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
        {
            let suffix = &name[prefix.len()..];
            if suffix.is_empty() || suffix.bytes().all(|b| b.is_ascii_digit()) {
                return (rpm::SOURCE_GROUP, index, suffix.parse().unwrap_or(0));
            }
        }
    }
    for (group, tags) in rpm::TAG_GROUPS.iter().enumerate() {
        if let Some(index) = tags.iter().position(|t| name.eq_ignore_ascii_case(t)) {
            return (group, index, 0);
        }
    }
    (rpm::TAG_GROUPS.len(), 0, 0)
}

/// Split only at top level; comparisons and rich dependencies stay together.
pub(crate) fn dependencies(value: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut stack = Vec::new();
    for (index, c) in value.char_indices() {
        if c == '\\' || c == '#' || c == '\'' || c == '"' {
            return None;
        }
        // A bare/parameterized call can consume the rest of the physical line.
        // Braced references have explicit boundaries and remain indivisible.
        if c == '%' && stack.last() != Some(&'}') && !value[index + 1..].starts_with('{') {
            return None;
        }
        match c {
            '(' => stack.push(')'),
            '{' => stack.push('}'),
            ')' | '}' if stack.pop() != Some(c) => return None,
            _ => {}
        }
        if stack.is_empty() && (c.is_whitespace() || c == ',') {
            if let Some(begin) = start.take() {
                tokens.push(value[begin..index].to_string());
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if !stack.is_empty() {
        return None;
    }
    if let Some(begin) = start {
        tokens.push(value[begin..].to_string());
    }
    if tokens.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let mut dependency = tokens[i].clone();
        if dependency.starts_with("%(")
            || dependency
                .as_bytes()
                .first()
                .is_some_and(|b| b"<>=!".contains(b))
        {
            return None;
        }
        i += 1;
        let trailing_operator = dependency
            .bytes()
            .rev()
            .take_while(|b| b"<>=!".contains(b))
            .count();
        if trailing_operator > 0 {
            // Also accept `foo>= 1.2`, retaining its original token spelling.
            if i >= tokens.len()
                || tokens[i]
                    .as_bytes()
                    .first()
                    .is_some_and(|b| b"<>=!".contains(b))
            {
                return None;
            }
            dependency.push(' ');
            dependency.push_str(&tokens[i]);
            i += 1;
        } else if i < tokens.len()
            && tokens[i]
                .as_bytes()
                .first()
                .is_some_and(|b| b"<>=!".contains(b))
        {
            let operator_length = tokens[i]
                .bytes()
                .take_while(|b| b"<>=!".contains(b))
                .count();
            dependency.push(' ');
            dependency.push_str(&tokens[i]);
            if operator_length < tokens[i].len() {
                // `foo >=1.2` is one condition, not two dependencies.
                i += 1;
            } else {
                if i + 1 >= tokens.len()
                    || tokens[i + 1]
                        .as_bytes()
                        .first()
                        .is_some_and(|b| b"<>=!".contains(b))
                {
                    return None;
                }
                dependency.push(' ');
                dependency.push_str(&tokens[i + 1]);
                i += 2;
            }
        }
        out.push(dependency);
    }
    Some(out)
}

/// Brace bare references, but not directives, argument-taking calls, or %% escapes.
pub(crate) fn brace_macros_with_names(line: &str, names: &[&str]) -> String {
    if line.trim_start().starts_with('#') {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'%') {
            i += 2;
            continue;
        }
        // Braced builtins may contain Lua or shell code, not macro references.
        if matches!(bytes.get(i + 1), Some(b'{' | b'(')) {
            let open = bytes[i + 1];
            let close = if open == b'{' { b'}' } else { b')' };
            let mut depth = 1;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == open {
                    depth += 1;
                }
                if bytes[i] == close {
                    depth -= 1;
                }
                i += 1;
            }
            continue;
        }
        let start = i;
        i += 1;
        if bytes.get(i) == Some(&b'!') {
            i += 1;
        }
        if bytes.get(i) == Some(&b'?') {
            i += 1;
        }
        let name_start = i;
        if !bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        {
            continue;
        }
        while bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            i += 1;
        }
        let name = &line[name_start..i];
        let conditional = name_start > start + 1;
        // Parameterized definitions override scalar/built-in assumptions.
        if names.iter().any(|declaration| {
            declaration.contains('(') && declaration.split('(').next() == Some(name)
        }) {
            continue;
        }
        let reference =
            names.contains(&name) || name.starts_with('_') || rpm::MACRO_REFERENCES.contains(&name);
        if !conditional && !reference {
            continue;
        }
        let at_start = line[..start].trim().is_empty();
        let followed_by_args =
            bytes.get(i).is_some_and(u8::is_ascii_whitespace) && !line[i..].trim().is_empty();
        if !names.contains(&name)
            && ((at_start
                && !conditional
                && !name.starts_with('_')
                && !rpm::STANDALONE_REFERENCES.contains(&name))
                || (followed_by_args
                    && !conditional
                    && !name.starts_with('_')
                    && !rpm::INLINE_REFERENCES.contains(&name)))
        {
            continue;
        }
        out.push_str(&line[copied..start]);
        out.push_str("%{");
        out.push_str(&line[start + 1..i]);
        out.push('}');
        copied = i;
    }
    out.push_str(&line[copied..]);
    out
}

pub(crate) fn definition(line: &str) -> Option<(&str, &str, &str)> {
    let line = line.trim_start();
    let directive = section_word(line);
    if !rpm::DEFINITION_DIRECTIVES.contains(&directive) {
        return None;
    }
    let rest = line[directive.len() + 1..].trim_start();
    let end = rest.find(char::is_whitespace)?;
    Some((directive, &rest[..end], rest[end..].trim_start()))
}

pub(crate) fn literal_definition(line: &str) -> bool {
    definition(line).is_some_and(|(_, name, body)| {
        !name.is_empty()
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !body.contains(['%', '\\'])
            && !body.is_empty()
    })
}

pub(crate) fn macro_names<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .filter_map(|line| definition(line).map(|(_, name, _)| name))
        .filter(|name| {
            let base = name.split('(').next().unwrap_or(name);
            !base.is_empty() && base.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_attached_comparison_spelling() {
        for condition in ["gcc >= 1.2", "gcc >=1.2", "gcc>= 1.2", "gcc>=1.2"] {
            assert_eq!(
                dependencies(&format!("{condition} cmake")).unwrap(),
                [condition, "cmake"]
            );
        }
        assert!(dependencies("").is_none());
        assert!(dependencies("gcc >=").is_none());
        assert!(dependencies("%dynamic arguments gcc").is_none());
    }
}

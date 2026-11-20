/// Shared, conservative text helpers. None of these expand RPM macros.
pub(crate) fn section_word(line: &str) -> &str {
    let rest = line.trim_start().strip_prefix('%').unwrap_or("");
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    &rest[..end]
}

pub(crate) fn is_section(line: &str) -> bool {
    matches!(
        section_word(line),
        "package"
            | "description"
            | "files"
            | "changelog"
            | "autochangelog"
            | "prep"
            | "build"
            | "install"
            | "check"
            | "clean"
            | "conf"
            | "generate_buildrequires"
            | "sourcelist"
            | "patchlist"
            | "pre"
            | "post"
            | "preun"
            | "postun"
            | "pretrans"
            | "posttrans"
            | "preuntrans"
            | "postuntrans"
            | "verifyscript"
            | "sepolicy"
            | "trigger"
            | "triggerin"
            | "triggerun"
            | "triggerpostun"
            | "filetrigger"
            | "filetriggerin"
            | "filetriggerun"
            | "filetriggerpostun"
            | "transfiletrigger"
            | "transfiletriggerin"
            | "transfiletriggerun"
            | "transfiletriggerpostun"
    )
}

pub(crate) fn preamble_end(lines: &[&str]) -> usize {
    lines
        .iter()
        .position(|line| is_section(line))
        .unwrap_or(lines.len())
}

pub(crate) fn executable_macro(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with('%') && !matches!(section_word(line), "global" | "define")
}

fn conditional(line: &str) -> bool {
    matches!(
        section_word(line),
        "if" | "ifarch"
            | "ifnarch"
            | "ifos"
            | "ifnos"
            | "else"
            | "elif"
            | "elifarch"
            | "elifnarch"
            | "elifos"
            | "elifnos"
            | "endif"
    )
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
    let metadata = [
        "Name",
        "Epoch",
        "Version",
        "Release",
        "Summary",
        "License",
        "URL",
        "Packager",
        "Group",
        "Vendor",
        "Distribution",
        "DistTag",
        "BugURL",
        "ExclusiveArch",
        "ExcludeArch",
        "ExclusiveOS",
        "ExcludeOS",
        "BuildArch",
        "AutoReqProv",
        "AutoReq",
        "AutoProv",
    ];
    if let Some(index) = metadata.iter().position(|t| name.eq_ignore_ascii_case(t)) {
        return (0, index, 0);
    }
    for (index, prefix) in ["Source", "Patch"].iter().enumerate() {
        if name
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
        {
            let suffix = &name[prefix.len()..];
            if suffix.is_empty() || suffix.bytes().all(|b| b.is_ascii_digit()) {
                return (1, index, suffix.parse().unwrap_or(0));
            }
        }
    }
    let dependencies = [
        "Provides",
        "Supplements",
        "Enhances",
        "Recommends",
        "Suggests",
        "Obsoletes",
        "Conflicts",
        "Requires",
        "OrderWithRequires",
    ];
    if let Some(index) = dependencies
        .iter()
        .position(|t| name.eq_ignore_ascii_case(t))
    {
        return (2, index, 0);
    }
    if name.eq_ignore_ascii_case("BuildRequires") || name.eq_ignore_ascii_case("BuildConflicts") {
        return (
            3,
            usize::from(name.eq_ignore_ascii_case("BuildConflicts")),
            0,
        );
    }
    if name.eq_ignore_ascii_case("BuildSystem") || name.eq_ignore_ascii_case("BuildOption") {
        return (4, usize::from(name.eq_ignore_ascii_case("BuildOption")), 0);
    }
    (5, 0, 0)
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
        match c {
            '(' => stack.push(')'),
            '{' => stack.push('}'),
            ')' | '}' => {
                if stack.pop() != Some(c) {
                    return None;
                }
            }
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
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let mut dependency = tokens[i].clone();
        if dependency.starts_with("%(") {
            return None;
        }
        i += 1;
        if i < tokens.len() && tokens[i].bytes().all(|b| b"<>=!".contains(&b)) {
            if i + 1 >= tokens.len() {
                return None;
            }
            dependency.push(' ');
            dependency.push_str(&tokens[i]);
            dependency.push(' ');
            dependency.push_str(&tokens[i + 1]);
            i += 2;
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
        if matches!(bytes.get(i + 1), Some(b'{') | Some(b'(')) {
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
        let reference = names.contains(&name)
            || name.starts_with('_')
            || matches!(
                name,
                "name"
                    | "version"
                    | "release"
                    | "buildroot"
                    | "dist"
                    | "evr"
                    | "epoch"
                    | "summary"
                    | "license"
                    | "url"
                    | "arch"
                    | "optflags"
                    | "buildsubdir"
                    | "sourcedir"
                    | "patchdir"
            );
        if !conditional && !reference {
            continue;
        }
        let at_start = line[..start].trim().is_empty();
        let followed_by_args =
            bytes.get(i).is_some_and(|b| b.is_ascii_whitespace()) && !line[i..].trim().is_empty();
        if !names.contains(&name)
            && ((at_start
                && !conditional
                && !name.starts_with('_')
                && !matches!(name, "name" | "version" | "release" | "buildroot"))
                || (followed_by_args
                    && !conditional
                    && !name.starts_with('_')
                    && !matches!(name, "name" | "version" | "release" | "buildroot" | "dist")))
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
    if !matches!(directive, "define" | "global") {
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
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .collect()
}

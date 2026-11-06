use crate::parse;
use anyhow::Result;
use tree_sitter::Node;

/// Formatting options.
#[derive(Debug, Clone)]
pub struct Config {
    /// Column (0-based) where tag values start.
    pub tag_column: usize,
    /// Maximum line width used for wrapping.
    pub max_width: usize,
    /// Sort values of consecutive tags with the same name.
    pub sort: bool,
    /// Wrap long dependency tag lines.
    pub wrap: bool,
    /// Replace leading tabs in scriptlet bodies with four spaces.
    pub reindent: bool,
    /// Re-wrap text in `%description` sections.
    pub wrap_description: bool,
    /// Rewrite paths in `%files` sections with macros.
    pub convert_paths: bool,
    /// Path prefixes and their macro replacements, longest prefix first.
    pub path_macros: Vec<(String, String)>,
}

pub const DEFAULT_PATH_MACROS: &[(&str, &str)] = &[
    // Common top-level paths.
    ("/etc", "%{_sysconfdir}"),
    ("/run", "%{_rundir}"),
    ("/usr", "%{_prefix}"),
    ("/var", "%{_localstatedir}"),
    // /etc
    ("/etc/fonts", "%{_fontconfig_masterdir}"),
    ("/etc/fonts/conf.d", "%{_fontconfig_confdir}"),
    ("/etc/java", "%{_javaconfdir}"),
    ("/etc/jvm", "%{_jvmsysconfdir}"),
    ("/etc/jvm-common", "%{_jvmcommonsysconfdir}"),
    ("/etc/pam.d", "%{_pam_confdir}"),
    ("/etc/rc.d/init.d", "%{_initddir}"),
    ("/etc/security", "%{_pam_secconfdir}"),
    // /usr
    ("/usr/bin", "%{_bindir}"),
    ("/usr/sbin", "%{_sbindir}"),
    ("/usr/src", "%{_usrsrc}"),
    ("/usr/include", "%{_includedir}"),
    ("/usr/include/d", "%{_d_includedir}"),
    ("/usr/lib64", "%{_libdir}"),
    ("/usr/lib64/gfortran/modules", "%{_fmoddir}"),
    ("/usr/lib64/ocaml", "%{ocamldir}"),
    ("/usr/lib64/pkgconfig", "%{_pkgconfigdir}"),
    ("/usr/lib64/security", "%{_pam_moduledir}"),
    ("/usr/lib/binfmt.d", "%{_binfmtdir}"),
    ("/usr/lib/environment.d", "%{_environmentdir}"),
    ("/usr/lib/java", "%{_jnidir}"),
    ("/usr/lib/jvm", "%{_jvmdir}"),
    ("/usr/lib/jvm-common", "%{_jvmcommonlibdir}"),
    ("/usr/lib/jvm-private", "%{_jvmprivdir}"),
    ("/usr/lib/kernel/install.d", "%{_kernel_install_dir}"),
    ("/usr/lib/modprobe.d", "%{_modprobedir}"),
    ("/usr/lib/modules-load.d", "%{_modulesloaddir}"),
    ("/usr/lib/mono", "%{_monodir}"),
    ("/usr/lib/mono/gac", "%{_monogacdir}"),
    ("/usr/lib/rpm", "%{_rpmconfigdir}"),
    ("/usr/lib/rpm/fileattrs", "%{_fileattrsdir}"),
    ("/usr/lib/rpm/lua", "%{_rpmluadir}"),
    ("/usr/lib/rpm/macros.d", "%{_rpmmacrodir}"),
    ("/usr/lib/sysctl.d", "%{_sysctldir}"),
    ("/usr/lib/sysusers.d", "%{_sysusersdir}"),
    ("/usr/lib/swidtag/fedoraproject.org", "%{_swidtagdir}"),
    ("/usr/lib/systemd", "%{_systemd_util_dir}"),
    ("/usr/lib/systemd/catalog", "%{_journalcatalogdir}"),
    ("/usr/lib/systemd/system", "%{_unitdir}"),
    (
        "/usr/lib/systemd/system-generators",
        "%{_systemdgeneratordir}",
    ),
    ("/usr/lib/systemd/system-preset", "%{_presetdir}"),
    (
        "/usr/lib/systemd/system-environment-generators",
        "%{_systemd_system_env_generator_dir}",
    ),
    ("/usr/lib/systemd/user", "%{_userunitdir}"),
    (
        "/usr/lib/systemd/user-generators",
        "%{_systemdusergeneratordir}",
    ),
    ("/usr/lib/systemd/user-preset", "%{_userpresetdir}"),
    (
        "/usr/lib/systemd/user-environment-generators",
        "%{_systemd_user_env_generator_dir}",
    ),
    ("/usr/lib/tmpfiles.d", "%{_tmpfilesdir}"),
    ("/usr/lib/udev/hwdb.d", "%{_udevhwdbdir}"),
    ("/usr/lib/udev/rules.d", "%{_udevrulesdir}"),
    ("/usr/libexec", "%{_libexecdir}"),
    ("/usr/libexec/perl5-tests", "%{perl_testdir}"),
    // /usr/share
    ("/usr/share", "%{_datadir}"),
    ("/usr/share/applications", "%{_appsdir}"),
    (
        "/usr/share/bash-completion/completions",
        "%{bash_completions_dir}",
    ),
    ("/usr/share/color", "%{_colordir}"),
    ("/usr/share/color/cmms", "%{_cmmscolordir}"),
    ("/usr/share/color/icc", "%{_icccolordir}"),
    ("/usr/share/color/settings", "%{_settingscolordir}"),
    ("/usr/share/doc", "%{_docdir}"),
    (
        "/usr/share/elvish/lib/completions",
        "%{elvish_completions_dir}",
    ),
    (
        "/usr/share/fish/vendor_completions.d",
        "%{fish_completions_dir}",
    ),
    ("/usr/share/fonts", "%{_fontbasedir}"),
    (
        "/usr/share/fontconfig/conf.avail",
        "%{_fontconfig_templatedir}",
    ),
    (
        "/usr/share/gnome-shell/extensions",
        "%{_gnomeextensionsdir}",
    ),
    ("/usr/share/icons", "%{_iconsdir}"),
    ("/usr/share/icons/hicolor", "%{_hicolordir}"),
    (
        "/usr/share/icons/hicolor/scalable/apps",
        "%{_scalableiconsdir}",
    ),
    ("/usr/share/info", "%{_infodir}"),
    ("/usr/share/ivy-xmls", "%{_ivyxmldir}"),
    ("/usr/share/java", "%{_javadir}"),
    ("/usr/share/javadoc", "%{_javadocdir}"),
    ("/usr/share/jvm", "%{_jvmdatadir}"),
    ("/usr/share/jvm-common", "%{_jvmcommondatadir}"),
    ("/usr/share/licenses", "%{_defaultlicensedir}"),
    ("/usr/share/man", "%{_mandir}"),
    ("/usr/share/maven-poms", "%{_mavenpomdir}"),
    ("/usr/share/metainfo", "%{_metainfodir}"),
    (
        "/usr/share/nushell/vendor/autoload",
        "%{nushell_completions_dir}",
    ),
    ("/usr/share/pam.d", "%{_pam_vendordir}"),
    (
        "/usr/share/pkgconfig/personality.d",
        "%{pkgconfig_personalitydir}",
    ),
    ("/usr/share/systemtap/tapset", "%{_systemtap_tapsetdir}"),
    ("/usr/share/user-tmpfiles.d", "%{_user_tmpfilesdir}"),
    ("/usr/share/zsh/site-functions", "%{zsh_completions_dir}"),
    // /var
    ("/var/lib", "%{_sharedstatedir}"),
    // EFI
    ("/boot/efi/EFI", "%{efi_esp_efi}"),
    ("/boot/efi/EFI/fedora", "%{efi_esp_dir}"),
];

impl Default for Config {
    fn default() -> Self {
        let mut path_macros: Vec<(String, String)> = DEFAULT_PATH_MACROS
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect();
        path_macros.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));

        Self {
            tag_column: 16,
            max_width: 100,
            sort: true,
            wrap: true,
            reindent: true,
            wrap_description: false,
            convert_paths: false,
            path_macros,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Other = 0,
    Body = 1,
    DescriptionBody = 2,
    FilesBody = 3,
    SectionHeader = 4,
    Tag = 5,
    Skip = 6,
}

fn is_body(kind: Kind) -> bool {
    matches!(kind, Kind::Body | Kind::DescriptionBody | Kind::FilesBody)
}

const SECTION_KINDS: &[&str] = &[
    "description",
    "package",
    "files",
    "changelog",
    "prep_scriptlet",
    "build_scriptlet",
    "install_scriptlet",
    "check_scriptlet",
    "clean_scriptlet",
    "conf_scriptlet",
    "runtime_scriptlet",
    "runtime_scriptlet_interpreter",
    "trigger",
    "file_trigger",
    "generate_buildrequires",
    "sourcelist",
    "patchlist",
];

const DEPENDENCY_TAGS: &[&str] = &[
    "BuildRequires",
    "Requires",
    "Provides",
    "Conflicts",
    "Obsoletes",
    "Recommends",
    "Suggests",
    "Supplements",
    "Enhances",
    "BuildConflicts",
    "OrderWithRequires",
];

/// Format a spec file.
pub fn format(source: &str, cfg: &Config) -> Result<String> {
    if source.is_empty() {
        return Ok(String::new());
    }

    // A missing final newline makes the grammar report a MISSING token at EOF,
    // which would mark the last section as unformattable. Parse a normalised
    // copy instead; node rows still map one-to-one onto the source lines.
    let parse_source = if source.ends_with('\n') {
        std::borrow::Cow::Borrowed(source)
    } else {
        std::borrow::Cow::Owned(format!("{source}\n"))
    };
    let tree = parse::parse(&parse_source)?;
    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    if source.ends_with('\n') {
        lines.pop();
    }

    let mut kinds = vec![Kind::Other; lines.len()];
    walk(tree.root_node(), &mut kinds);

    mark_heredocs(&lines, &mut kinds);

    // Split tags into (tag, value) pairs.
    let mut parts: Vec<Option<(String, String)>> = lines
        .iter()
        .zip(&kinds)
        .map(|(line, kind)| {
            if *kind == Kind::Tag {
                split_tag(line)
            } else {
                None
            }
        })
        .collect();

    if cfg.sort {
        sort_tag_runs(&kinds, &mut parts);
    }

    // Rows replaced by a pre-rendered block (dependency wrapping/sorting).
    let plans = build_dependency_plans(&kinds, &mut parts, cfg);

    let mut rows: Vec<(Kind, String)> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if let Some(block) = &plans[i] {
            for part in block {
                rows.push((Kind::Tag, part.clone()));
            }
            continue;
        }

        let kind = kinds[i];
        let text = match kind {
            Kind::Skip => line.clone(),
            Kind::Tag => match &parts[i] {
                Some((tag, value)) => render_tag(tag, value, cfg),
                None => line.trim_end().to_string(),
            },
            Kind::Body => {
                let line = if cfg.reindent {
                    expand_leading_tabs(line)
                } else {
                    line.clone()
                };
                line.trim_end().to_string()
            }
            Kind::DescriptionBody => line.trim_end().to_string(),
            Kind::FilesBody => {
                let line = if cfg.convert_paths {
                    convert_paths_in_line(line, &cfg.path_macros)
                } else {
                    line.clone()
                };
                line.trim_end().to_string()
            }
            _ => line.trim_end().to_string(),
        };
        rows.push((kind, text));
    }

    let rows = if cfg.wrap_description {
        wrap_descriptions(rows, cfg.max_width)
    } else {
        rows
    };
    let rows = apply_spacing(rows);
    let mut out: Vec<&str> = rows.iter().map(|(_, text)| text.as_str()).collect();
    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    let mut text = out.join("\n");
    text.push('\n');
    Ok(text)
}

fn walk(node: Node<'_>, kinds: &mut [Kind]) {
    if node.is_error() || node.is_missing() {
        mark_skip(kinds, node.start_position().row, node.end_position().row);
        return;
    }

    let kind = node.kind();

    if kind == "preamble_tag" || kind == "packager_tag" {
        set_kind(kinds, node.start_position().row, Kind::Tag);
    } else if SECTION_KINDS.contains(&kind) {
        let start = node.start_position().row;
        let end = node.end_position();
        let last = if end.column == 0 {
            end.row.saturating_sub(1)
        } else {
            end.row
        };

        if node.has_error() {
            mark_skip(kinds, start, last);
            return;
        }

        set_kind(kinds, start, Kind::SectionHeader);
        let body_kind = match kind {
            "description" => Kind::DescriptionBody,
            "files" => Kind::FilesBody,
            _ => Kind::Body,
        };
        for row in (start + 1)..=last {
            set_kind(kinds, row, body_kind);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, kinds);
    }
}

fn set_kind(kinds: &mut [Kind], row: usize, kind: Kind) {
    if row < kinds.len() && kinds[row] < kind {
        kinds[row] = kind;
    }
}

fn mark_skip(kinds: &mut [Kind], start: usize, end: usize) {
    for row in start..=end.min(kinds.len().saturating_sub(1)) {
        kinds[row] = Kind::Skip;
    }
}

/// Mark heredoc bodies as skipped so tabs and trailing spaces are preserved.
fn mark_heredocs(lines: &[String], kinds: &mut [Kind]) {
    let mut i = 0;
    while i < lines.len() {
        if is_body(kinds[i]) {
            if let Some(delim) = heredoc_delim(&lines[i]) {
                let mut j = i + 1;
                while j < lines.len() && is_body(kinds[j]) {
                    kinds[j] = Kind::Skip;
                    if lines[j].trim() == delim {
                        break;
                    }
                    j += 1;
                }
                i = j;
            }
        }
        i += 1;
    }
}

fn heredoc_delim(line: &str) -> Option<String> {
    let idx = line.find("<<")?;
    let rest = &line[idx + 2..];
    let rest = rest.trim_start_matches('-').trim_start();
    let rest = rest.trim_start_matches(['"', '\'']);
    let delim: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if delim.is_empty() {
        None
    } else {
        Some(delim)
    }
}

fn expand_leading_tabs(line: &str) -> String {
    let tabs = line.len() - line.trim_start_matches('\t').len();
    if tabs == 0 {
        return line.to_string();
    }
    format!("{}{}", "    ".repeat(tabs), &line[tabs..])
}

fn split_tag(line: &str) -> Option<(String, String)> {
    let s = line.trim_start();
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }

    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }

    if i < bytes.len() && bytes[i] == b'(' {
        let mut j = i + 1;
        while j < bytes.len() && bytes[j] != b')' {
            j += 1;
        }
        if j < bytes.len() {
            i = j + 1;
        }
    }

    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }

    Some((s[..=i].to_string(), s[i + 1..].trim().to_string()))
}

fn base_tag(tag: &str) -> &str {
    match tag.find('(') {
        Some(idx) => &tag[..idx],
        None => tag.trim_end_matches(':'),
    }
}

fn render_tag(tag: &str, value: &str, cfg: &Config) -> String {
    let tag = tag.trim_end();
    if value.is_empty() {
        return tag.to_string();
    }

    let padding = tag_padding(tag, cfg);
    format!("{tag}{}{value}", " ".repeat(padding))
}

fn tag_padding(tag: &str, cfg: &Config) -> usize {
    if tag.len() + 1 >= cfg.tag_column {
        1
    } else {
        cfg.tag_column - tag.len()
    }
}

fn token_is_simple(token: &str) -> bool {
    !token.contains(|c: char| "()<>=|'\"#".contains(c)) && !token.contains("%(")
}

/// Dependency tags are handled as a block so that sorting and wrapping are
/// stable under repeated formatting: the tokens are sorted once and then
/// wrapped deterministically.
fn build_dependency_plans(
    kinds: &[Kind],
    parts: &mut [Option<(String, String)>],
    cfg: &Config,
) -> Vec<Option<Vec<String>>> {
    let mut plans = vec![None; parts.len()];
    if !cfg.wrap && !cfg.sort {
        return plans;
    }

    let mut i = 0;
    while i < parts.len() {
        let Some((tag, _)) = &parts[i] else {
            i += 1;
            continue;
        };
        if kinds[i] != Kind::Tag || !DEPENDENCY_TAGS.contains(&base_tag(tag)) {
            i += 1;
            continue;
        }
        let tag = tag.clone();

        let mut j = i + 1;
        while j < parts.len()
            && kinds[j] == Kind::Tag
            && parts[j].as_ref().is_some_and(|(t, _)| *t == tag)
        {
            j += 1;
        }

        let values: Vec<String> = (i..j)
            .filter_map(|k| parts[k].as_ref().map(|(_, v)| v.clone()))
            .collect();
        let tokens: Vec<String> = values
            .iter()
            .flat_map(|value| value.split_whitespace().map(str::to_string))
            .collect();
        let simple = tokens.iter().all(|token| token_is_simple(token));

        if simple && cfg.wrap && !tokens.is_empty() {
            let mut tokens = tokens;
            if cfg.sort {
                tokens.sort_by_key(|token| token.to_lowercase());
            }
            let block = wrap_tokens(&tag, &tokens, cfg);
            plans[i] = Some(block);
            for slot in plans.iter_mut().take(j).skip(i + 1) {
                *slot = Some(Vec::new());
            }
        } else if cfg.sort {
            let mut sorted = values;
            sorted.sort_by_key(|value| value.to_lowercase());
            for (offset, value) in sorted.into_iter().enumerate() {
                parts[i + offset].as_mut().unwrap().1 = value;
            }
        }

        i = j;
    }

    plans
}

fn wrap_tokens(tag: &str, tokens: &[String], cfg: &Config) -> Vec<String> {
    let padding = tag_padding(tag, cfg);
    let gap = " ".repeat(padding);
    let prefix = tag.len() + padding;

    let mut out = Vec::new();
    let mut current = String::new();
    for token in tokens {
        if current.is_empty() {
            current.push_str(token);
        } else if prefix + current.len() + 1 + token.len() <= cfg.max_width {
            current.push(' ');
            current.push_str(token);
        } else {
            out.push(format!("{tag}{gap}{current}"));
            current = token.to_string();
        }
    }
    if !current.is_empty() {
        out.push(format!("{tag}{gap}{current}"));
    }
    out
}

fn sort_tag_runs(kinds: &[Kind], parts: &mut [Option<(String, String)>]) {
    let mut i = 0;
    while i < parts.len() {
        let Some((tag, _)) = &parts[i] else {
            i += 1;
            continue;
        };
        if DEPENDENCY_TAGS.contains(&base_tag(tag)) {
            i += 1;
            continue;
        }
        let tag = tag.clone();
        let mut j = i + 1;
        while j < parts.len()
            && kinds[j] == Kind::Tag
            && parts[j].as_ref().is_some_and(|(t, _)| *t == tag)
        {
            j += 1;
        }

        let mut values: Vec<String> = (i..j)
            .map(|k| parts[k].as_ref().unwrap().1.clone())
            .collect();
        values.sort_by_key(|value| value.to_lowercase());
        for (offset, value) in values.into_iter().enumerate() {
            parts[i + offset].as_mut().unwrap().1 = value;
        }

        i = j;
    }
}

fn apply_spacing(rows: Vec<(Kind, String)>) -> Vec<(Kind, String)> {
    // Insert one blank line before section headers. If a comment block sits
    // directly above the header, keep the comment attached to the section and
    // insert the blank line above the comment block instead.
    let mut spaced: Vec<(Kind, String)> = Vec::with_capacity(rows.len() + 8);
    for (kind, text) in rows {
        if kind == Kind::SectionHeader && !spaced.is_empty() {
            let last = spaced.last().unwrap();
            if !last.1.is_empty() && last.0 != Kind::SectionHeader {
                if last.1.trim_start().starts_with('#') {
                    let mut idx = spaced.len();
                    while idx > 0 && spaced[idx - 1].1.trim_start().starts_with('#') {
                        idx -= 1;
                    }
                    if idx > 0 && !spaced[idx - 1].1.is_empty() {
                        spaced.insert(idx, (Kind::Other, String::new()));
                    }
                } else {
                    spaced.push((Kind::Other, String::new()));
                }
            }
        }
        spaced.push((kind, text));
    }

    // Drop blank lines directly after a section header.
    let mut trimmed: Vec<(Kind, String)> = Vec::with_capacity(spaced.len());
    let mut i = 0;
    while i < spaced.len() {
        trimmed.push(spaced[i].clone());
        if spaced[i].0 == Kind::SectionHeader {
            i += 1;
            while i < spaced.len() && spaced[i].1.is_empty() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    // Collapse runs of blank lines outside section bodies.
    let mut out: Vec<(Kind, String)> = Vec::with_capacity(trimmed.len());
    let mut i = 0;
    while i < trimmed.len() {
        if trimmed[i].1.is_empty() {
            let mut j = i;
            while j < trimmed.len() && trimmed[j].1.is_empty() {
                j += 1;
            }
            let prev_body = out.last().is_some_and(|(kind, _)| is_body(*kind));
            let next_body = trimmed.get(j).is_some_and(|(kind, _)| is_body(*kind));
            let keep = if prev_body || next_body { j - i } else { 1 };
            for _ in 0..keep {
                out.push((Kind::Other, String::new()));
            }
            i = j;
        } else {
            out.push(trimmed[i].clone());
            i += 1;
        }
    }

    while out.first().is_some_and(|(_, text)| text.is_empty()) {
        out.remove(0);
    }

    out
}

/// Re-flow long lines in `%description` bodies.
///
/// Each line is wrapped on its own, so existing line breaks are never joined.
/// Blank lines, indentation and `*` / `-` / `+` bullets are kept, which also
/// makes the result idempotent.
fn wrap_descriptions(rows: Vec<(Kind, String)>, max_width: usize) -> Vec<(Kind, String)> {
    if max_width == 0 {
        return rows;
    }

    let mut out = Vec::with_capacity(rows.len());
    for (kind, text) in rows {
        if kind != Kind::DescriptionBody || text.trim().is_empty() {
            out.push((kind, text));
            continue;
        }
        for line in wrap_description_line(&text, max_width) {
            out.push((Kind::DescriptionBody, line));
        }
    }
    out
}

fn wrap_description_line(line: &str, max_width: usize) -> Vec<String> {
    if line.chars().count() <= max_width {
        return vec![line.to_string()];
    }

    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let (bullet, body) = if let Some(rest) = trimmed.strip_prefix("* ") {
        ("* ", rest)
    } else if let Some(rest) = trimmed.strip_prefix("- ") {
        ("- ", rest)
    } else if let Some(rest) = trimmed.strip_prefix("+ ") {
        ("+ ", rest)
    } else {
        ("", trimmed)
    };

    let first_prefix = format!("{indent}{bullet}");
    let cont_prefix = " ".repeat(first_prefix.chars().count());

    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_prefix = first_prefix;

    for word in body.split_whitespace() {
        let separator = usize::from(!current.is_empty());
        let projected = current_prefix.chars().count()
            + current.chars().count()
            + separator
            + word.chars().count();

        if current.is_empty() {
            current.push_str(word);
        } else if projected <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            out.push(format!("{current_prefix}{current}"));
            current_prefix = cont_prefix.clone();
            current.clear();
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        out.push(format!("{current_prefix}{current}"));
    }
    if out.is_empty() {
        vec![line.to_string()]
    } else {
        out
    }
}

/// Replace leading path prefixes in a `%files` line with macros.
fn convert_paths_in_line(line: &str, macros: &[(String, String)]) -> String {
    if line.trim_start().starts_with('#') {
        return line.to_string();
    }

    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find(|c: char| !c.is_whitespace()) {
        let (space, tail) = rest.split_at(start);
        out.push_str(space);

        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let (token, tail) = tail.split_at(end);
        out.push_str(&convert_path_token(token, macros));
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn convert_path_token(token: &str, macros: &[(String, String)]) -> String {
    // Quoted paths may contain spaces, so leave them alone.
    if token.starts_with('"') || token.starts_with('\'') {
        return token.to_string();
    }

    for (from, to) in macros {
        if token == from {
            return to.clone();
        }
        if let Some(suffix) = token.strip_prefix(from) {
            if suffix.starts_with('/') {
                return format!("{to}{suffix}");
            }
        }
    }
    token.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ws(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn aligns_packager_with_email() {
        let src = "Name: foo\nVersion: 1\nRelease: 1\nSummary: x\nLicense: MIT\nPackager: Alice <alice@example.com>\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.contains("Packager:       Alice <alice@example.com>"),
            "{out}"
        );
    }

    #[test]
    fn preserves_non_whitespace_bytes() {
        let src = "Name:   foo\nVersion: 1\n\n%description\n some text  \n\n * item\n";
        let out = format(src, &Config::default()).unwrap();
        assert_eq!(strip_ws(src), strip_ws(&out));
    }

    #[test]
    fn is_idempotent() {
        let src =
            "Name:   foo\nVersion: 1\nRelease: 1\n\n%description\nText\n\n%files\n%{_bindir}/foo\n";
        let once = format(src, &Config::default()).unwrap();
        let twice = format(&once, &Config::default()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn sorts_dependency_tokens() {
        let src = "Name: x\n\nBuildRequires:  zlib openssl\nBuildRequires:  cmake gcc\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(
            out.contains("BuildRequires:  cmake gcc openssl zlib"),
            "{out}"
        );
    }

    #[test]
    fn wraps_long_dependency_lines() {
        let src = "Name: x\n\nBuildRequires: alpha bravo charlie delta echo foxtrot golf hotel india juliet\n";
        let cfg = Config {
            max_width: 40,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert!(out.lines().count() > 2, "{out}");
        for line in out
            .lines()
            .filter(|line| line.starts_with("BuildRequires:"))
        {
            assert!(line.len() <= 40, "{line}");
        }
        assert_eq!(format(&out, &cfg).unwrap(), out);
    }

    #[test]
    fn safe_mode_keeps_dependency_order() {
        let src = "Name: x\n\nBuildRequires:  zlib\nBuildRequires:  cmake\n";
        let cfg = Config {
            sort: false,
            wrap: false,
            reindent: false,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert_eq!(
            out,
            "Name:           x\n\nBuildRequires:  zlib\nBuildRequires:  cmake\n"
        );
    }

    #[test]
    fn adds_blank_line_before_sections_only() {
        let src = "Name: x\n%description\nText\n%files\n/foo\n";
        let out = format(src, &Config::default()).unwrap();
        assert_eq!(
            out,
            "Name:           x\n\n%description\nText\n\n%files\n/foo\n"
        );
    }

    #[test]
    fn keeps_heredoc_content() {
        let src = "Name: x\n\n%build\ncat << 'EOF'\na\t b  \nEOF\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(out.contains("a\t b  \n"), "{out}");
    }

    #[test]
    fn removes_trailing_whitespace_and_adds_final_newline() {
        let out = format("Name:   x   ", &Config::default()).unwrap();
        assert_eq!(out, "Name:           x\n");
    }

    #[test]
    fn wraps_description_lines() {
        let src = "Name: x\n\n%description\nThis is a very long line of text that should be wrapped at a narrow width.\n\n* A bullet item that is also long enough to wrap onto a second line.\n";
        let cfg = Config {
            max_width: 40,
            wrap_description: true,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        for line in out.lines() {
            assert!(line.chars().count() <= 40, "{line}");
        }
        assert!(out.contains("This is a very long line of text that"));
        assert!(out.contains("\n\n* A bullet item"), "{out}");
        assert_eq!(format(&out, &cfg).unwrap(), out);
    }

    #[test]
    fn description_wrapping_is_off_by_default() {
        let src = "Name: x\n\n%description\nThis is a very long line of text that should stay on one line.\n";
        let out = format(src, &Config::default()).unwrap();
        assert!(out.contains("This is a very long line of text that should stay on one line."));
    }

    #[test]
    fn converts_paths_in_files_only() {
        let src = "Name: x\n\n%description\n/usr/share/keep\n\n%files\n/usr/bin/foo\n%{_datadir}/already\n/usr/lib/systemd/system/foo.service\n/etc/foo.conf\n/usr/share/doc/pkg/README\n/usr/share/man/man1/foo.1\n/usr/lib64/pkgconfig/foo.pc\n/var/lib/foo\n/usr/local/foo\n/usr/sharefoo/keep\n\n%changelog\n* Sat Nov 07 2026 Tester <t@example.com> - 1\n- Initial\n";
        let cfg = Config {
            convert_paths: true,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert!(out.contains("%{_bindir}/foo"), "{out}");
        assert!(out.contains("%{_datadir}/already"), "{out}");
        assert!(out.contains("%{_unitdir}/foo.service"), "{out}");
        assert!(out.contains("%{_sysconfdir}/foo.conf"), "{out}");
        assert!(out.contains("%{_docdir}/pkg/README"), "{out}");
        assert!(out.contains("%{_mandir}/man1/foo.1"), "{out}");
        assert!(out.contains("%{_pkgconfigdir}/foo.pc"), "{out}");
        assert!(out.contains("%{_sharedstatedir}/foo"), "{out}");
        assert!(out.contains("%{_prefix}/local/foo"), "{out}");
        // The /usr/share prefix must not match /usr/sharefoo.
        assert!(out.contains("%{_prefix}/sharefoo/keep"), "{out}");
        assert!(!out.contains("%{_datadir}sharefoo"), "{out}");
        assert!(out.contains("%description\n/usr/share/keep"), "{out}");
        assert_eq!(format(&out, &cfg).unwrap(), out);
    }

    #[test]
    fn keeps_comments_and_quoted_paths_in_files() {
        let src =
            "Name: x\n\n%files\n# /usr/share/comment\n\"/usr/share/with space\"\n/usr/bin/foo\n";
        let cfg = Config {
            convert_paths: true,
            ..Config::default()
        };
        let out = format(src, &cfg).unwrap();
        assert!(out.contains("# /usr/share/comment"), "{out}");
        assert!(out.contains("\"/usr/share/with space\""), "{out}");
        assert!(out.contains("%{_bindir}/foo"), "{out}");
    }
}

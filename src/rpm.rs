//! RPM vocabulary shared by the formatter and text-based style/lint helpers.
//! Keep policy tables here; parsing and transformation logic belongs elsewhere.

pub(crate) const DEPENDENCY_TAGS: &[&str] = &[
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

/// Tree-sitter node names, not literal spec directives.
pub(crate) const SECTION_KINDS: &[&str] = &[
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

/// Literal section directives whose bodies the text linter treats as non-tags.
pub(crate) const BODY_SECTIONS: &[&str] = &[
    "description",
    "files",
    "changelog",
    "prep",
    "build",
    "install",
    "check",
    "clean",
    "pre",
    "post",
    "preun",
    "postun",
    "pretrans",
    "posttrans",
    "preuntrans",
    "postuntrans",
    "trigger",
    "triggerin",
    "triggerun",
    "triggerpostun",
    "filetrigger",
    "filetriggerin",
    "filetriggerun",
    "filetriggerpostun",
    "transfiletrigger",
    "transfiletriggerin",
    "transfiletriggerun",
    "transfiletriggerpostun",
    "generate_buildrequires",
    "sourcelist",
    "patchlist",
    "verifyscript",
    "sepolicy",
];

pub(crate) const CONDITIONAL_DIRECTIVES: &[&str] = &[
    "if",
    "ifarch",
    "ifnarch",
    "ifos",
    "ifnos",
    "else",
    "elif",
    "elifarch",
    "elifnarch",
    "elifos",
    "elifnos",
    "endif",
];

pub(crate) const DEFINITION_DIRECTIVES: &[&str] = &["global", "define"];

/// Group order and tag order within each group. Unlisted tags form a final group.
/// Source/Patch suffixes are handled numerically by the style helper.
pub(crate) const SOURCE_GROUP: usize = 1;
pub(crate) const TAG_GROUPS: &[&[&str]] = &[
    &[
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
    ],
    &["Source", "Patch"],
    &[
        "Provides",
        "Supplements",
        "Enhances",
        "Recommends",
        "Suggests",
        "Obsoletes",
        "Conflicts",
        "Requires",
        "OrderWithRequires",
    ],
    &["BuildRequires", "BuildConflicts"],
    &["BuildSystem", "BuildOption"],
];

/// Known non-parameterized RPM references. Underscore-prefixed references and
/// macros defined in the spec are recognized separately.
pub(crate) const MACRO_REFERENCES: &[&str] = &[
    "name",
    "version",
    "release",
    "buildroot",
    "dist",
    "evr",
    "epoch",
    "summary",
    "license",
    "url",
    "arch",
    "optflags",
    "buildsubdir",
    "sourcedir",
    "patchdir",
];
pub(crate) const STANDALONE_REFERENCES: &[&str] = &["name", "version", "release", "buildroot"];
pub(crate) const INLINE_REFERENCES: &[&str] = &["name", "version", "release", "buildroot", "dist"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_not_duplicated_between_groups() {
        let tags: Vec<_> = TAG_GROUPS.iter().flat_map(|group| group.iter()).collect();
        for (index, tag) in tags.iter().enumerate() {
            assert!(
                !tags[..index]
                    .iter()
                    .any(|earlier| tag.eq_ignore_ascii_case(earlier)),
                "duplicate tag: {tag}"
            );
        }
    }

    #[test]
    fn dependency_groups_match_dependency_tag_membership() {
        let tags: Vec<_> = TAG_GROUPS[2..4]
            .iter()
            .flat_map(|group| group.iter())
            .copied()
            .collect();
        assert_eq!(tags.len(), DEPENDENCY_TAGS.len());
        for tag in DEPENDENCY_TAGS {
            assert!(tags.contains(tag), "dependency missing from groups: {tag}");
        }
    }

    #[test]
    fn context_specific_macros_are_known_references() {
        for name in STANDALONE_REFERENCES.iter().chain(INLINE_REFERENCES) {
            assert!(MACRO_REFERENCES.contains(name), "unknown reference: {name}");
        }
    }
}

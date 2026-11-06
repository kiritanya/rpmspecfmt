use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::format::Config;

/// Configuration file, normally `.rpmspecfmt.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub tag_column: Option<usize>,
    pub max_width: Option<usize>,
    pub sort: Option<bool>,
    pub wrap: Option<bool>,
    pub reindent: Option<bool>,
    pub wrap_description: Option<bool>,
    pub convert_paths: Option<bool>,
    pub path_macros: Option<BTreeMap<String, String>>,
}

impl FileConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Search the current directory and its parents for `.rpmspecfmt.toml`.
    pub fn find() -> Option<PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let candidate = dir.join(".rpmspecfmt.toml");
            if candidate.is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    pub fn apply(self, cfg: &mut Config) {
        if let Some(value) = self.tag_column {
            cfg.tag_column = value;
        }
        if let Some(value) = self.max_width {
            cfg.max_width = value;
        }
        if let Some(value) = self.sort {
            cfg.sort = value;
        }
        if let Some(value) = self.wrap {
            cfg.wrap = value;
        }
        if let Some(value) = self.reindent {
            cfg.reindent = value;
        }
        if let Some(value) = self.wrap_description {
            cfg.wrap_description = value;
        }
        if let Some(value) = self.convert_paths {
            cfg.convert_paths = value;
        }
        if let Some(macros) = self.path_macros {
            for (from, to) in macros {
                match cfg
                    .path_macros
                    .iter_mut()
                    .find(|(prefix, _)| *prefix == from)
                {
                    Some(entry) => entry.1 = to,
                    None => cfg.path_macros.push((from, to)),
                }
            }
            cfg.path_macros
                .sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Config;

    #[test]
    fn reads_config() {
        let file: FileConfig = toml::from_str(
            "max_width = 80\nwrap_description = true\nconvert_paths = true\n\
             [path_macros]\n\"/opt\" = \"%{_prefix}/opt\"\n",
        )
        .unwrap();

        let mut cfg = Config::default();
        file.apply(&mut cfg);

        assert_eq!(cfg.max_width, 80);
        assert!(cfg.wrap_description);
        assert!(cfg.convert_paths);
        assert!(cfg
            .path_macros
            .iter()
            .any(|(from, to)| from == "/opt" && to == "%{_prefix}/opt"));
        // User entries extend the defaults instead of replacing them.
        assert!(cfg.path_macros.iter().any(|(from, _)| from == "/usr/share"));
    }

    #[test]
    fn keeps_longest_prefix_first() {
        let file: FileConfig =
            toml::from_str("[path_macros]\n\"/opt\" = \"%{_prefix}/opt\"\n").unwrap();
        let mut cfg = Config::default();
        file.apply(&mut cfg);

        for pair in cfg.path_macros.windows(2) {
            assert!(pair[0].0.len() >= pair[1].0.len(), "{:?}", cfg.path_macros);
        }
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(toml::from_str::<FileConfig>("unknown = true").is_err());
    }
}

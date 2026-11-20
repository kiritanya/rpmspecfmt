use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};
use stable_eyre::eyre::{ensure, Result, WrapErr};

use crate::format::Config;

/// Configuration file, normally `.rpmspecfmt.toml`.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_tags: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_buildrequires: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalize_macros: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_macros: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reindent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_description: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convert_paths: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_macros: Option<BTreeMap<String, String>>,
}

impl FileConfig {
    /// Load an explicitly selected configuration file.
    ///
    /// # Errors
    /// Returns an error if the file is missing, unreadable, or invalid TOML.
    pub fn load(path: &Path) -> Result<Self> {
        ensure!(
            path.is_file(),
            "config file does not exist: {}",
            path.display()
        );
        Figment::from(Toml::file(path))
            .extract()
            .wrap_err_with(|| format!("failed to load {}", path.display()))
    }

    /// Defaults < TOML < prefixed environment < explicitly supplied CLI values.
    ///
    /// # Errors
    /// Returns an error if the selected file or merged configuration is invalid.
    pub fn resolve(path: Option<&Path>, defaults: &Config, overrides: &Self) -> Result<Config> {
        let values = Self {
            tag_column: Some(defaults.tag_column),
            group_tags: Some(defaults.group_tags),
            split_buildrequires: Some(defaults.split_buildrequires),
            normalize_macros: Some(defaults.normalize_macros),
            global_macros: Some(defaults.global_macros),
            max_width: Some(defaults.max_width),
            sort: Some(defaults.sort),
            wrap: Some(defaults.wrap),
            reindent: Some(defaults.reindent),
            wrap_description: Some(defaults.wrap_description),
            convert_paths: Some(defaults.convert_paths),
            path_macros: None,
        };
        let mut figment = Figment::from(Serialized::defaults(values));
        if let Some(path) = path {
            ensure!(
                path.is_file(),
                "config file does not exist: {}",
                path.display()
            );
            tracing::debug!(path = %path.display(), "loading configuration");
            figment = figment.merge(Toml::file(path));
        }
        let file: Self = figment
            .merge(Env::prefixed("RPMSPECFMT_").split("__"))
            .merge(Serialized::defaults(overrides))
            .extract()
            .wrap_err("invalid rpmspecfmt configuration")?;
        let mut cfg = defaults.clone();
        file.apply(&mut cfg);
        Ok(cfg)
    }

    /// Search the current directory and its parents for `.rpmspecfmt.toml`.
    #[must_use]
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
        if let Some(value) = self.group_tags {
            cfg.group_tags = value;
        }
        if let Some(value) = self.split_buildrequires {
            cfg.split_buildrequires = value;
        }
        if let Some(value) = self.normalize_macros {
            cfg.normalize_macros = value;
        }
        if let Some(value) = self.global_macros {
            cfg.global_macros = value;
        }
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

    #[test]
    fn can_disable_new_style_changes() {
        let file: FileConfig = toml::from_str("group_tags = false\nsplit_buildrequires = false\nnormalize_macros = false\nglobal_macros = false\n").unwrap();
        let mut cfg = Config::default();
        file.apply(&mut cfg);
        assert!(!cfg.group_tags);
        assert!(!cfg.split_buildrequires);
        assert!(!cfg.normalize_macros);
        assert!(!cfg.global_macros);
    }
}

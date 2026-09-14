use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// Validated source-relative backup exclusion patterns.
///
/// Basename patterns match at any depth; patterns containing a separator are
/// rooted at each source. A trailing separator restricts a rule to directories.
/// Matching a directory also excludes its descendants. Matching ignores case
/// and accepts both Windows and forward-slash separators. Negation is unsupported.
#[derive(Debug)]
pub struct ExclusionRules {
    paths: GlobSet,
    directories: GlobSet,
}

impl ExclusionRules {
    /// Compiles exclusion rules, ignoring blank lines.
    ///
    /// # Errors
    /// Returns an error for invalid globs, absolute paths, registry paths,
    /// traversal components, NUL characters, or negation rules.
    pub fn new(patterns: &[String]) -> Result<Self, String> {
        let mut paths = GlobSetBuilder::new();
        let mut directories = GlobSetBuilder::new();
        for original in patterns {
            let normalized = original.trim().replace('\\', "/");
            if normalized.is_empty() {
                continue;
            }
            let directory_only = normalized.ends_with('/');
            let pattern = normalized.trim_end_matches('/');
            let first = pattern.split('/').next().unwrap_or_default();
            let registry_hive = matches!(
                first.to_ascii_uppercase().as_str(),
                "HKCU"
                    | "HKLM"
                    | "HKCR"
                    | "HKU"
                    | "HKCC"
                    | "HKEY_CURRENT_USER"
                    | "HKEY_LOCAL_MACHINE"
                    | "HKEY_CLASSES_ROOT"
                    | "HKEY_USERS"
                    | "HKEY_CURRENT_CONFIG"
            );
            if normalized.starts_with('/')
                || pattern.is_empty()
                || pattern.starts_with('!')
                || pattern.contains([':', '\0'])
                || pattern
                    .split('/')
                    .any(|part| matches!(part, ".." | "." | ""))
                || registry_hive
            {
                return Err(format!(
                    "Invalid source-relative exclusion pattern: {original:?}"
                ));
            }
            let glob = if pattern.contains('/') {
                pattern.to_owned()
            } else {
                format!("**/{pattern}")
            };
            let glob = GlobBuilder::new(&glob)
                .literal_separator(true)
                .case_insensitive(true)
                .backslash_escape(false)
                .build()
                .map_err(|error| format!("Invalid exclusion pattern {original:?}: {error}"))?;
            if directory_only {
                directories.add(glob);
            } else {
                paths.add(glob);
            }
        }
        Ok(Self {
            paths: paths.build().map_err(|error| error.to_string())?,
            directories: directories.build().map_err(|error| error.to_string())?,
        })
    }

    /// Tests a path relative to its configured source root, without filesystem IO.
    /// Ancestors are directories even when the candidate no longer exists.
    #[must_use]
    pub fn is_excluded(&self, relative_path: &Path, is_dir: bool) -> bool {
        let normalized = relative_path.to_string_lossy().replace('\\', "/");
        let mut candidate = normalized.trim_end_matches('/');
        let mut directory = is_dir;
        while !candidate.is_empty() {
            if self.paths.is_match(candidate) || (directory && self.directories.is_match(candidate))
            {
                return true;
            }
            let Some((parent, _)) = candidate.rsplit_once('/') else {
                break;
            };
            candidate = parent;
            directory = true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::ExclusionRules;
    use std::path::Path;

    fn rules(patterns: &[&str]) -> ExclusionRules {
        ExclusionRules::new(&patterns.iter().map(ToString::to_string).collect::<Vec<_>>())
            .expect("valid exclusions")
    }

    #[test]
    fn basename_globs_match_at_any_depth_without_crossing_components() {
        let rules = rules(&["*.tmp"]);
        assert!(rules.is_excluded(Path::new("save.tmp"), false));
        assert!(rules.is_excluded(Path::new("nested/SAVE.TMP"), false));
        assert!(!rules.is_excluded(Path::new("save.tmp.bak"), false));
        assert!(!rules.is_excluded(Path::new("nested/save.dat"), false));
    }

    #[test]
    fn directory_rules_protect_descendants_without_matching_same_named_files() {
        let rules = rules(&["cache/"]);
        assert!(rules.is_excluded(Path::new("cache"), true));
        assert!(rules.is_excluded(Path::new("nested/cache"), true));
        assert!(rules.is_excluded(Path::new("nested/cache/gone.dat"), false));
        assert!(rules.is_excluded(Path::new("cache/nested/gone.dat"), false));
        assert!(!rules.is_excluded(Path::new("cache"), false));
        assert!(!rules.is_excluded(Path::new("nested/cache"), false));
        assert!(!rules.is_excluded(Path::new("cache.txt"), true));
        assert!(!rules.is_excluded(Path::new("cached/file"), false));
    }

    #[test]
    fn slash_patterns_are_root_relative_and_accept_windows_separators() {
        let rules = rules(&[r"logs\debug.log", "temp/cache/"]);
        assert!(rules.is_excluded(Path::new("logs/debug.log"), false));
        assert!(rules.is_excluded(Path::new(r"LOGS\DEBUG.LOG"), false));
        assert!(!rules.is_excluded(Path::new("other/logs/debug.log"), false));
        assert!(rules.is_excluded(Path::new("temp/cache/file"), false));
        assert!(!rules.is_excluded(Path::new("other/temp/cache/file"), false));
    }

    #[test]
    fn matched_directories_exclude_descendants_even_without_trailing_slash() {
        let rules = rules(&["*.tmp", "logs/debug"]);
        assert!(rules.is_excluded(Path::new("scratch.tmp/save.dat"), false));
        assert!(rules.is_excluded(Path::new("logs/debug/deep/save.dat"), false));
        assert!(!rules.is_excluded(Path::new("other/logs/debug/save.dat"), false));
    }

    #[test]
    fn empty_lines_are_ignored() {
        let rules = rules(&["", "  ", "\r\n"]);
        assert!(!rules.is_excluded(Path::new("save.dat"), false));
        assert!(!rules.is_excluded(Path::new(""), true));
    }

    #[test]
    fn unsafe_and_unsupported_patterns_report_errors() {
        for pattern in [
            "/cache",
            r"C:\cache",
            "C:cache",
            r"\\server\cache",
            "../cache",
            "cache/../save",
            "cache\0file",
            "!save.dat",
            "[",
            "HKCU/Software",
            r"HKEY_CURRENT_USER\Software",
            "/",
            "./",
        ] {
            assert!(
                ExclusionRules::new(&[pattern.to_owned()]).is_err(),
                "accepted {pattern:?}"
            );
        }
    }

    #[test]
    fn globstars_and_single_component_stars_preserve_directory_boundaries() {
        let rules = rules(&["logs/*.log", "state/**/cache/"]);
        assert!(rules.is_excluded(Path::new("logs/debug.log"), false));
        assert!(!rules.is_excluded(Path::new("logs/deep/debug.log"), false));
        assert!(rules.is_excluded(Path::new("state/cache/file"), false));
        assert!(rules.is_excluded(Path::new("state/a/b/cache/file"), false));
        assert!(!rules.is_excluded(Path::new("other/state/cache/file"), false));
    }
}

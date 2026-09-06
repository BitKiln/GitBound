//! Turning filesystem paths into text a person or a shell can use.

use std::path::Path;

/// A path as it should be shown to a person, written into a config file, or
/// handed to a shell.
///
/// On Windows `canonicalize` answers with a verbatim path — `\\?\C:\repo` —
/// which is what the filesystem APIs want and wrong everywhere a human reads
/// it. Stripping it in one place keeps the CLI's messages, the rules the
/// desktop lists, and the `exec` line a hook runs from each growing their own
/// half of the rule.
pub fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(suffix) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{suffix}");
        }
        if let Some(suffix) = value.strip_prefix(r"\\?\") {
            return suffix.to_string();
        }
    }
    value.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn an_ordinary_path_is_unchanged() {
        let path = PathBuf::from(if cfg!(windows) {
            r"C:\repositories\work"
        } else {
            "/home/user/work"
        });
        assert_eq!(display_path(&path), path.to_string_lossy());
    }

    #[cfg(windows)]
    #[test]
    fn a_verbatim_path_loses_its_prefix() {
        assert_eq!(
            display_path(std::path::Path::new(r"\\?\C:\repositories\work")),
            r"C:\repositories\work"
        );
    }

    /// A UNC share keeps the leading `\\`, or the path stops naming the host.
    #[cfg(windows)]
    #[test]
    fn a_verbatim_unc_path_keeps_its_share() {
        assert_eq!(
            display_path(std::path::Path::new(r"\\?\UNC\server\share\work")),
            r"\\server\share\work"
        );
    }
}

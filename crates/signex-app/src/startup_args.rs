//! Command-line document arguments (#317).
//!
//! The installers register Signex as the handler for its native file
//! types (`signex.exe "%1"`, `Exec=signex %F`), so the OS launches the
//! binary with the document path(s) as positional arguments. This module
//! turns that argv tail into the paths to open — nothing else. Opening
//! goes through the ordinary `FileMsg::Opened` flow, which already knows
//! every supported extension and reports failures.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Paths named on the command line, resolved against `cwd`.
///
/// `args` is argv *without* the program name. Anything starting with `-`
/// is a flag this app does not own and is skipped, as is an empty
/// argument. Extensions are deliberately not filtered: an unsupported
/// file must reach the open flow so the user sees an error rather than an
/// empty workspace.
pub fn document_paths<I>(args: I, cwd: &Path) -> Vec<PathBuf>
where
    I: IntoIterator<Item = OsString>,
{
    args.into_iter()
        .filter(|arg| !arg.is_empty() && !arg.to_string_lossy().starts_with('-'))
        .map(|arg| cwd.join(PathBuf::from(arg)))
        .collect()
}

/// [`document_paths`] over the real process argv and working directory.
/// An unreadable working directory only costs relative-path resolution:
/// absolute paths (what every OS launcher passes) still work.
pub fn document_paths_from_env() -> Vec<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_default();
    document_paths(std::env::args_os().skip(1), &cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_opens_nothing() {
        assert!(document_paths(args(&[]), Path::new("/work")).is_empty());
    }

    #[test]
    fn relative_path_resolves_against_cwd() {
        let cwd = Path::new("/work");
        assert_eq!(
            document_paths(args(&["a/b.snxsch"]), cwd),
            vec![cwd.join("a/b.snxsch")]
        );
    }

    #[test]
    fn absolute_path_is_kept_as_is() {
        let abs = std::env::temp_dir().join("p.snxprj");
        let got = document_paths(
            vec![abs.clone().into_os_string()],
            Path::new("/somewhere/else"),
        );
        assert_eq!(got, vec![abs]);
    }

    #[test]
    fn flags_and_empty_arguments_are_ignored() {
        let cwd = Path::new("/work");
        assert_eq!(
            document_paths(args(&["--foo", "", "-x", "a.snxsch"]), cwd),
            vec![cwd.join("a.snxsch")]
        );
    }

    #[test]
    fn several_paths_keep_their_order() {
        let cwd = Path::new("/work");
        assert_eq!(
            document_paths(args(&["b.snxsch", "a.snxpcb", "p.snxprj"]), cwd),
            vec![
                cwd.join("b.snxsch"),
                cwd.join("a.snxpcb"),
                cwd.join("p.snxprj")
            ]
        );
    }

    #[test]
    fn unsupported_extension_is_passed_through_for_the_open_flow_to_report() {
        let cwd = Path::new("/work");
        assert_eq!(
            document_paths(args(&["notes.txt"]), cwd),
            vec![cwd.join("notes.txt")]
        );
    }
}

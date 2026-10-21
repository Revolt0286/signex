//! Commit-author identity resolution shared by every git entry point
//! (library adapter, project adapter, "Enable Version Control").
//!
//! Durable rule: a commit is authored by the active user, never by a
//! generic Signex identity. When no user name resolves, callers get an
//! error they surface to the user and the commit is not made.

use crate::adapter::LibraryError;

/// Resolve the commit identity for `repo`. Sources, first hit wins:
///
/// 1. git config `user.name` / `user.email` (repo, then global).
/// 2. `GIT_AUTHOR_*` / `GIT_COMMITTER_*` env vars.
/// 3. `USER` / `USERNAME` and `EMAIL` shell hints. A missing email
///    becomes `<name>@<hostname>`; only a missing *name* blocks.
pub fn identity_for_repo(repo: &git2::Repository) -> Result<(String, String), LibraryError> {
    resolve_identity(repo.config().ok().as_ref(), |k| std::env::var(k).ok())
}

/// Same rule as [`identity_for_repo`] for callers that have no repo yet
/// (checks the default git config chain).
pub fn identity_for_default_config() -> Result<(String, String), LibraryError> {
    resolve_identity(git2::Config::open_default().ok().as_ref(), |k| {
        std::env::var(k).ok()
    })
}

/// Pure resolver behind the helpers above; `env` is injected so the
/// "nothing configured" path is testable without touching the process
/// environment.
pub fn resolve_identity(
    cfg: Option<&git2::Config>,
    env: impl Fn(&str) -> Option<String>,
) -> Result<(String, String), LibraryError> {
    let non_empty = |s: String| (!s.trim().is_empty()).then_some(s);
    let cfg_get = |key: &str| cfg.and_then(|c| c.get_string(key).ok()).and_then(non_empty);
    let env_get = |key: &str| env(key).and_then(non_empty);

    let name = cfg_get("user.name")
        .or_else(|| env_get("GIT_AUTHOR_NAME"))
        .or_else(|| env_get("GIT_COMMITTER_NAME"))
        .or_else(|| env_get("USER"))
        .or_else(|| env_get("USERNAME"))
        .ok_or_else(|| {
            LibraryError::Backend(
                "git identity not configured — set user.name via \
                 `git config --global user.name \"…\"` (or set GIT_AUTHOR_NAME). \
                 Signex won't author commits with a generic identity."
                    .to_string(),
            )
        })?;
    let email = cfg_get("user.email")
        .or_else(|| env_get("GIT_AUTHOR_EMAIL"))
        .or_else(|| env_get("GIT_COMMITTER_EMAIL"))
        .or_else(|| env_get("EMAIL"))
        .unwrap_or_else(|| {
            let host = env_get("HOSTNAME")
                .or_else(|| env_get("COMPUTERNAME"))
                .unwrap_or_else(|| "localhost".to_string());
            format!("{name}@{host}")
        });
    Ok((name, email))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn no_identity_anywhere_is_an_error_not_a_generic_author() {
        let err = resolve_identity(None, no_env).unwrap_err();
        assert!(err.to_string().contains("identity not configured"));
    }

    #[test]
    fn blank_values_do_not_count_as_an_identity() {
        let env = |k: &str| (k == "GIT_AUTHOR_NAME").then(|| "  ".to_string());
        assert!(resolve_identity(None, env).is_err());
    }

    #[test]
    fn env_name_without_email_gets_a_host_derived_email() {
        let env = |k: &str| match k {
            "GIT_AUTHOR_NAME" => Some("Ada".to_string()),
            "COMPUTERNAME" => Some("box".to_string()),
            _ => None,
        };
        assert_eq!(
            resolve_identity(None, env).unwrap(),
            ("Ada".to_string(), "Ada@box".to_string())
        );
    }
}

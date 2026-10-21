//! Containment guard for the Remove-from-Project "Delete file" choice (#139).
//!
//! A project can reference `.snxlib` packages that live outside its own
//! folder (`Shared` / `Global` libraries). Those belong to other projects
//! too, so the project tree must never delete them from disk.

use std::path::Path;

/// Is `target` a path strictly inside `project_root`, after symlinks and
/// `..` segments are resolved?
///
/// Both paths are canonicalised, so this fails closed: a target or root that
/// cannot be resolved (missing, unreadable) is reported as not contained.
pub(super) fn is_inside_project_root(target: &Path, project_root: &Path) -> bool {
    let (Ok(target), Ok(root)) = (target.canonicalize(), project_root.canonicalize()) else {
        return false;
    };
    target != root && target.starts_with(&root)
}

/// Should the dialog treat `target` as living outside the project? An
/// orphan (missing) target has nothing on disk to delete, so it is not
/// flagged — the exclude/delete choices are equivalent for it.
pub(super) fn is_outside_project(target: &Path, project_root: Option<&Path>) -> bool {
    if !target.exists() {
        return false;
    }
    match project_root {
        Some(root) => !is_inside_project_root(target, root),
        None => true,
    }
}

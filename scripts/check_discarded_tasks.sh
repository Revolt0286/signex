#!/usr/bin/env bash
#
# Discarded-Task ratchet gate (GH #99 part 1) — `let _ = self.handle_x(...)`
# silently throws away an `iced::Task<Message>`. `Task` is `#[must_use]`
# (iced_runtime-0.14.0/src/task.rs), but binding the result to `_` counts as
# "used" under rustc's `unused_must_use` lint (it only fires on a bare
# expression-statement), so the compiler never catches THIS shape.
#
# It does catch the neighbouring one. `self.handle_x(...)?;` — a bare
# expression-statement — has always made rustc emit `unused Task that must
# be used`, and since GH #548 that is DENIED for signex-app
# (crates/signex-app/Cargo.toml, `[lints.rust]`), so it fails the build
# rather than adding one more line to a ~324-warning wall. Do not widen this
# grep to cover `expr?;`: that shape belongs to the type checker, and a bash
# approximation of it would be strictly worse.
#
# COUNTER-GUIDANCE, and the reason this paragraph exists: rustc's own help
# text for that error reads `use `let _ = ...` to ignore the resulting
# value`. Following it converts a defect the COMPILER sees into one only
# this grep can see — and this grep documents its blind spots below. Never
# silence `unused_must_use` with `let _ =`; handle the Task or return it.
#
# A dropped `Task` is a silent no-op today only because every
# offending callee happens to return `Task::none()` on every path; the
# moment one of them starts doing real async work (e.g. `.snxlib` mounting
# going async), the dropped work vanishes with no warning anywhere.
#
# LIMITS — read before trusting a clean run. This is a grep, not a type
# checker, so it can only catch the textual shape it's told to look for:
#
#   - It also matches `drop(<recv>.handle_x(...))` (one line or wrapped) and
#     `drop(x)` where `x` was bound earlier in the same file by a single-line
#     `let x = <recv>.handle_...`. `drop(expr)` is not `let _ = expr`, so
#     neither rustc nor the `let _` grep saw it. Bare `drop(guard)` on
#     anything else is left alone: lock guards make it legitimate. Bindings
#     are tracked per fn and cleared when the name is rebound.
#     Still missed: `let t =` with the `handle_` call on the next line then
#     `drop(t)`; `std::mem::drop(t)`; `drop(foo(self.handle_x()))`; and
#     field-chain receivers such as `self.items.handle_x()`.
#   - It matches `let _ = <receiver>.handle_<name>(...)` (or `::`), on one
#     line OR rustfmt's line-wrapped `let _ =` / `    <receiver>.handle_
#     <name>(...)` two-line form (see `find_discards` below) — the exact
#     shapes of every one of the 15 sites fixed in #99 part 1. The receiver
#     used to be anchored to `self`, which let `let _ = app.handle_x()`,
#     `Self::handle_x()` and `state.handle_x()` through; #571 widened it to
#     any single identifier receiver. It does NOT catch:
#       * a discard through a differently-named method, e.g.
#         `let _ = self.some_other_call();` (not prefixed `handle_`).
#         This one is LIVE in production today:
#         crates/signex-app/src/app/handlers/canvas/clicked.rs — `let _ =
#         self.place_selected_component(wx, wy);`. Benign only because that
#         method returns `bool` (crates/signex-app/src/app/actions.rs);
#         change it to `Task<Message>` and the #99 bug class returns
#         invisibly. Closing this needs `clippy::let_underscore_must_use`
#         at deny, which is blocked on the 13 sites tracked in #533 — no
#         grep can cover it, because the shape is indistinguishable from a
#         legitimate discard without the type.
#       * a receiver that is itself an expression, e.g.
#         `let _ = self.foo().handle_x();` or `let _ = v[0].handle_x();`
#       * any other must-use-defeating idiom (`std::mem::forget`, a
#         `let _task = ...` binding that is never read, ...)
#       * a genuinely unused `let task = self.handle_x(...);` binding
#         (rustc's own `unused_variables` lint already flags that one,
#         just not the `let _ =` form this gate exists for)
#       * a wrap deeper than one continuation line (e.g. `let _ =` /
#         `self` / `    .handle_x(...)` split across three lines)
#   - A `handle_*` method that legitimately returns something other than
#     a `Task` (e.g. `bool`) and is discarded on purpose is not a false
#     positive this gate should ever have to special-case away — prefer
#     renaming the discard target off `handle_*`, or fixing the discard,
#     over growing the allowlist. The allowlist exists as an escape hatch
#     for a case that turns out to need it, not a place to grandfather new
#     `let _ = self.handle_` sites; the list should stay empty.
#
# Test exemption: production line count is what matters here, same
# convention as scripts/check_file_size.sh — everything up to a trailing
# INLINE `#[cfg(test)] mod tests { … }` block (running to EOF) is skipped,
# via the identical `prod_cutoff` truncation `check_file_size.sh` uses (its
# `prod_lines`, adapted to return a line number instead of a count — see
# that script for the full rationale on why only a *trailing* inline
# module counts). Dedicated test files (`tests/` dirs, `tests.rs`,
# `*_tests.rs`) are excluded from `list_files` entirely, matching
# check_file_size.sh's scope.
#
# That exclusion is by FILENAME, not by `#[cfg(test)]` (#571). A `tests.rs`
# or `*_tests.rs` sitting inside `src/` is skipped wherever it lives — ten
# discards currently sit in
# crates/signex-app/src/app/handlers/menu/export/tests.rs and
# crates/signex-app/src/app/view/dialogs/annotate_preview/tests.rs. They are
# test code, so this is not a defect; it is a naming convention doing a
# `cfg` attribute's job, and a production file that happens to be named
# `tests.rs` would be invisible to this gate.
#
# Usage:
#   scripts/check_discarded_tasks.sh   # check (CI); exit 1 on a violation
#
set -euo pipefail

ROOT="${DISCARDED_TASKS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
ALLOWLIST="$ROOT/scripts/discarded-tasks-allowlist.txt"

cd "$ROOT"

list_files() {
  find crates -type f -name '*.rs' -path '*/src/*' \
    -not -path '*/target/*' \
    -not -path '*/tests/*' \
    -not -name 'tests.rs' \
    -not -name '*_tests.rs' \
    | sort
}

# Last production line number in `$1` — everything from a trailing inline
# `#[cfg(test)] mod name { ... }` to EOF is test code and doesn't count.
# Lifted from check_file_size.sh's `prod_lines`; see that script for the
# full rationale (external `mod tests;` declarations and mid-file
# `#[cfg(test)] fn helper()` do NOT truncate — only the LAST attribute
# immediately followed by a `mod <name> {` opener does).
prod_cutoff() {
  local f="$1" n
  n=$(awk '
    /^#\[cfg\(test\)\][[:space:]]*$/ { pending = NR; next }
    pending {
      if ($0 ~ /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\{[[:space:]]*$/) {
        last = pending
      }
      pending = 0
    }
    END { if (last) print last }
  ' "$f")
  if [[ -n "$n" ]]; then
    echo $((n - 1))
  else
    wc -l < "$f" | tr -d ' '
  fi
}

# Emits one line number per discard found in the production-only prefix of
# `$1` (lines 1..cutoff) — both `let _ = <recv>.handle_...(...)` on a single
# line, and rustfmt's wrapped `let _ =` / `<recv>.handle_...(...)` two-line
# form, reporting the line the `self.handle_` call itself sits on.
find_discards() {
  local f="$1" cutoff="$2"
  awk -v cutoff="$cutoff" '
    NR > cutoff { exit }
    {
      trimmed = $0
      gsub(/^[ \t]+/, "", trimmed)
      gsub(/[ \t]+$/, "", trimmed)
      if (trimmed ~ /^let _ = ([A-Za-z_][A-Za-z0-9_]*)(\.|::)handle_/) {
        print NR
      } else if (prev_bare && trimmed ~ /^([A-Za-z_][A-Za-z0-9_]*)(\.|::)handle_/) {
        print NR
      } else if (trimmed ~ /^drop\(([A-Za-z_][A-Za-z0-9_]*)(\.|::)handle_/) {
        print NR
      } else if (prev_drop && trimmed ~ /^([A-Za-z_][A-Za-z0-9_]*)(\.|::)handle_/) {
        print NR
      } else if (match(trimmed, /^drop\([A-Za-z_][A-Za-z0-9_]*\)/)) {
        # `drop(x)` where `x` was bound from a handle_ call earlier in this file.
        name = substr(trimmed, 6, RLENGTH - 6)
        if (name in task_vars) print NR
      }
      # Bindings are per-fn: forget them all at a fn boundary, and forget a
      # name when it is rebound to anything that is not a handle_ call.
      if (trimmed ~ /(^|[ \t])fn [A-Za-z_]/) delete task_vars
      if (match(trimmed, /^let (mut )?[A-Za-z_][A-Za-z0-9_]*/)) {
        rebind = substr(trimmed, RSTART, RLENGTH)
        sub(/^let (mut )?/, "", rebind)
        delete task_vars[rebind]
      }
      # `let [mut] x[: T] = <recv>.handle_...` — remember x for a later drop(x).
      if (match(trimmed, /^let (mut )?[A-Za-z_][A-Za-z0-9_]*(: [^=]+)? = ([A-Za-z_][A-Za-z0-9_]*)(\.|::)handle_/)) {
        bind = trimmed
        sub(/^let (mut )?/, "", bind)
        sub(/[^A-Za-z0-9_].*$/, "", bind)
        task_vars[bind] = 1
      }
      prev_bare = (trimmed == "let _ =")
      prev_drop = (trimmed == "drop(")
    }
  ' "$f"
}

# `path:line` exact-match allowlist, one entry per line, blank lines and
# `#`-comments ignored. Looked up line-by-line (not an associative array)
# so this runs on the bash 3.2 that ships with macOS as well as bash 5 on
# CI, matching check_file_size.sh's convention.
is_allowlisted() {
  local entry="$1"
  [[ -f "$ALLOWLIST" ]] || return 1
  while IFS= read -r row; do
    [[ -z "$row" || "$row" == \#* ]] && continue
    [[ "$row" == "$entry" ]] && return 0
  done < "$ALLOWLIST"
  return 1
}

fail=0
while IFS= read -r f; do
  cutoff=$(prod_cutoff "$f")
  while IFS= read -r lineno; do
    [[ -z "${lineno:-}" ]] && continue
    entry="$f:$lineno"
    if is_allowlisted "$entry"; then
      continue
    fi
    echo "FAIL  $entry — discards a Task via \`let _ =\` / \`drop(...)\` on \`<recv>.handle_...\`."
    echo "      Return the Task to the caller instead (see GH #99 part 1)."
    echo "      If the callee genuinely doesn't return a Task, add"
    echo "      \"$entry\" to ${ALLOWLIST#"$ROOT"/} with a comment saying why."
    fail=1
  done < <(find_discards "$f" "$cutoff")
done < <(list_files)

if (( fail )); then
  echo ""
  echo "Discarded-Task gate failed. See scripts/check_discarded_tasks.sh header."
  exit 1
fi
echo "Discarded-Task gate: OK — no discarded \`<recv>.handle_...\` Tasks found."

//! Per-rule severity overrides. Applied after a run so the engine stays
//! ignorant of user preferences; the app hands in its override map and shows
//! whatever comes back.

use std::collections::HashMap;

use crate::{RuleKind, Severity, Violation};

/// Apply the user's per-rule severity overrides to `violations`, drop the
/// ones overridden to [`Severity::Off`], and order the rest Error → Warning →
/// Info (ties broken by rule name for a stable report).
///
/// An override is the user's verbatim choice: it replaces the severity of
/// every violation of that rule, including rules that grade individual
/// findings (`BadHierSheetPin`).
pub fn apply_overrides(
    mut violations: Vec<Violation>,
    overrides: &HashMap<RuleKind, Severity>,
) -> Vec<Violation> {
    for v in &mut violations {
        if let Some(&sev) = overrides.get(&v.rule) {
            v.severity = sev;
        }
    }
    violations.retain(|v| v.severity != Severity::Off);
    violations.sort_by_key(|v| {
        let bucket = match v.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
            Severity::Off => 3,
        };
        (bucket, format!("{:?}", v.rule))
    });
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::violations_for;

    const LEVELS: [Severity; 4] = [
        Severity::Error,
        Severity::Warning,
        Severity::Info,
        Severity::Off,
    ];

    #[test]
    fn overriding_each_rule_changes_every_one_of_its_violations() {
        for kind in RuleKind::ALL {
            let base = violations_for(kind);
            assert!(!base.is_empty(), "fixture for {kind:?} produced nothing");
            for level in LEVELS {
                let map = HashMap::from([(kind, level)]);
                let out = apply_overrides(base.clone(), &map);
                if level == Severity::Off {
                    assert!(out.is_empty(), "{kind:?} Off must drop every finding");
                } else {
                    assert_eq!(out.len(), base.len(), "{kind:?} -> {level:?}");
                    assert!(
                        out.iter().all(|v| v.severity == level),
                        "{kind:?} -> {level:?} left a finding at its old severity",
                    );
                }
            }
        }
    }

    #[test]
    fn an_override_only_touches_its_own_rule() {
        let mut all = violations_for(RuleKind::UnusedPin);
        all.extend(violations_for(RuleKind::MissingPowerFlag));
        let map = HashMap::from([(RuleKind::MissingPowerFlag, Severity::Error)]);
        let out = apply_overrides(all, &map);
        for v in out {
            match v.rule {
                RuleKind::MissingPowerFlag => assert_eq!(v.severity, Severity::Error),
                other => assert_eq!(v.severity, other.default_severity()),
            }
        }
    }

    #[test]
    fn output_is_sorted_error_first() {
        let mut all = violations_for(RuleKind::MissingPowerFlag);
        all.extend(violations_for(RuleKind::DuplicateRefDesignator));
        let out = apply_overrides(all, &HashMap::new());
        let ranks: Vec<_> = out.iter().map(|v| v.severity).collect();
        assert_eq!(ranks.first(), Some(&Severity::Error));
        assert_eq!(ranks.last(), Some(&Severity::Info));
    }
}

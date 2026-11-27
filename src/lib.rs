//! An independent evaluator and regression gate for agent snapshots.
//!
//! # Case identity encoding
//!
//! [`CaseId`] version 1 is SHA-256 over this canonical byte sequence:
//!
//! 1. the ASCII bytes `hertz-evaluator.case.v1`, followed by one zero byte;
//! 2. the case wording as an unsigned 64-bit big-endian byte length followed by
//!    its UTF-8 bytes;
//! 3. the number of required-fact checks as an unsigned 64-bit big-endian
//!    integer; and
//! 4. each required fact, in order, as an unsigned 64-bit big-endian byte
//!    length followed by its UTF-8 bytes.
//!
//! Strings are encoded exactly as supplied: no trimming or Unicode
//! normalization is performed. Check order is significant. The display form
//! is `case-v1-sha256:` followed by the lowercase hexadecimal digest. This
//! encoding and hash are frozen for version 1; any future change must use a new
//! `CaseId` version.

mod case;
mod compare;
mod promotion;
mod score;
mod store;

pub use case::{Case, CaseId};
pub use compare::{compare_scorecards, ComparisonOutcome, ComparisonReport};
pub use promotion::CasePromotion;
pub use score::{
    score_run, CaseScore, CheckKind, CheckResult, PinError, PinnedCase, RecordedRun, Scorecard,
};
pub use store::{CaseStore, InMemoryStore, StoreError};

#[cfg(test)]
mod tests {
    use super::{
        compare_scorecards, score_run, Case, CaseId, CasePromotion, CaseStore, CheckKind,
        ComparisonOutcome, InMemoryStore, PinnedCase, RecordedRun, Scorecard, StoreError,
    };
    use std::time::Duration;

    fn case() -> Case {
        Case::new(
            "Explain why the snapshot is safe.",
            ["The regression suite passed."],
        )
    }

    #[test]
    fn same_content_has_same_id() {
        let mut store = InMemoryStore::new();

        let first = store.register(case()).unwrap();
        let second = store.register(case()).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first.to_string(),
            "case-v1-sha256:c40e83946e79db11ae7c1a939a42806e0fb378e9cea7d451208d4a1a73d5bc59"
        );
    }

    #[test]
    fn material_edits_have_new_ids() {
        let mut store = InMemoryStore::new();
        let original = store.register(case()).unwrap();
        let wording_edit = store
            .register(Case::new(
                "Explain why this snapshot is safe.",
                ["The regression suite passed."],
            ))
            .unwrap();
        let check_edit = store
            .register(Case::new(
                "Explain why the snapshot is safe.",
                ["The production regression suite passed."],
            ))
            .unwrap();

        assert_ne!(original, wording_edit);
        assert_ne!(original, check_edit);
    }

    #[test]
    fn registered_content_loads_exactly() {
        let mut store = InMemoryStore::new();
        let registered = case();

        let id = store.register(registered.clone()).unwrap();

        assert_eq!(store.load(&id), Ok(registered));
    }

    #[test]
    fn missing_id_is_explicitly_not_found() {
        let store = InMemoryStore::new();
        let missing = CaseId::from_digest([0; 32]);

        assert_eq!(store.load(&missing), Err(StoreError::NotFound(missing)));
    }

    #[test]
    fn approval_promotes_only_the_exact_registered_id() {
        let mut store = InMemoryStore::new();
        let approved = store.register(case()).unwrap();
        let unreviewed = store
            .register(Case::new("Unreviewed case.", ["Still loadable."]))
            .unwrap();
        let missing = CaseId::from_digest([0; 32]);

        store.approve(&approved).unwrap();

        assert_eq!(store.prod().len(), 1);
        assert!(store.prod().contains(&approved));
        assert_eq!(store.approve(&missing), Err(StoreError::NotFound(missing)));
        assert_eq!(store.prod().len(), 1);
        assert!(store.prod().contains(&approved));
        assert!(store.load(&unreviewed).is_ok());
        assert!(!store.prod().contains(&unreviewed));
    }

    #[test]
    fn modified_case_does_not_replace_approved_version() {
        let mut store = InMemoryStore::new();
        let original = case();
        let original_id = store.register(original.clone()).unwrap();
        store.approve(&original_id).unwrap();

        let modified_id = store
            .register(Case::new(
                "Explain why the modified snapshot is safe.",
                ["The regression suite passed."],
            ))
            .unwrap();

        assert_ne!(modified_id, original_id);
        assert_eq!(store.prod().len(), 1);
        assert!(store.prod().contains(&original_id));
        assert_eq!(store.load(&original_id), Ok(original));
    }

    #[test]
    fn demotion_removes_prod_membership_without_rewriting_case() {
        let mut store = InMemoryStore::new();
        let registered = case();
        let id = store.register(registered.clone()).unwrap();
        store.approve(&id).unwrap();

        store.demote(&id).unwrap();

        assert!(store.prod().is_empty());
        assert_eq!(store.load(&id), Ok(registered));
    }

    fn scoring_case() -> (CaseId, Case, PinnedCase) {
        let mut store = InMemoryStore::new();
        let case = Case::new(
            "Evaluate a recorded deployment check.",
            ["The regression suite passed."],
        );
        let id = store.register(case.clone()).unwrap();
        let pinned = PinnedCase::new(
            id,
            case.clone(),
            ["search", "report"],
            ["plan", "search", "report"],
            Duration::from_millis(100),
        )
        .unwrap();
        (id, case, pinned)
    }

    fn passing_run() -> RecordedRun {
        RecordedRun::new(
            "The regression suite passed.",
            ["search", "report"],
            ["plan", "search", "report", "finish"],
            Duration::from_millis(80),
        )
    }

    fn score(output: &str) -> Scorecard {
        let (_, _, pinned) = scoring_case();
        score_run(
            &RecordedRun::new(
                output,
                ["search", "report"],
                ["plan", "search", "report", "finish"],
                Duration::from_millis(80),
            ),
            &[pinned],
        )
    }

    #[test]
    fn scorecard_comparison_reports_all_outcomes() {
        let passing = score("The regression suite passed.");
        let failing = score("No result was recorded.");

        assert_eq!(
            compare_scorecards(&passing, Some(&failing)).outcome(),
            ComparisonOutcome::Improved
        );
        assert_eq!(
            compare_scorecards(&passing, Some(&passing)).outcome(),
            ComparisonOutcome::Unchanged
        );
        assert_eq!(
            compare_scorecards(&failing, Some(&passing)).outcome(),
            ComparisonOutcome::Regressed
        );
    }

    #[test]
    fn scorecard_comparison_is_deterministic() {
        let passing = score("The regression suite passed.");
        let failing = score("No result was recorded.");

        assert_eq!(
            compare_scorecards(&passing, Some(&failing)),
            compare_scorecards(&passing, Some(&failing))
        );
    }

    #[test]
    fn missing_baseline_is_an_explicit_first_run() {
        let passing = score("The regression suite passed.");
        let report = compare_scorecards(&passing, None);

        assert_eq!(report.outcome(), ComparisonOutcome::FirstRun);
        assert_eq!(report.baseline_passed_checks(), None);
        assert_eq!(report.candidate_passed_checks(), 4);
    }

    #[test]
    fn deterministic_checks_pass_through_public_api() {
        let (_, _, pinned) = scoring_case();
        let scorecard = score_run(&passing_run(), &[pinned]);

        assert!(scorecard.passed());
        assert_eq!(scorecard.cases()[0].checks().len(), 4);
        assert!(scorecard.cases()[0]
            .checks()
            .iter()
            .all(|check| check.passed()));
    }

    #[test]
    fn each_deterministic_check_can_fail_alone() {
        let (_, _, pinned) = scoring_case();
        let runs = [
            (
                CheckKind::RequiredFacts,
                RecordedRun::new(
                    "No result was recorded.",
                    ["search", "report"],
                    ["plan", "search", "report", "finish"],
                    Duration::from_millis(80),
                ),
            ),
            (
                CheckKind::RequiredTools,
                RecordedRun::new(
                    "The regression suite passed.",
                    ["search"],
                    ["plan", "search", "report", "finish"],
                    Duration::from_millis(80),
                ),
            ),
            (
                CheckKind::Trajectory,
                RecordedRun::new(
                    "The regression suite passed.",
                    ["search", "report"],
                    ["plan", "report", "search", "finish"],
                    Duration::from_millis(80),
                ),
            ),
            (
                CheckKind::Latency,
                RecordedRun::new(
                    "The regression suite passed.",
                    ["search", "report"],
                    ["plan", "search", "report", "finish"],
                    Duration::from_millis(101),
                ),
            ),
        ];

        for (expected_failure, run) in runs {
            let scorecard = score_run(&run, std::slice::from_ref(&pinned));
            let failed: Vec<_> = scorecard.cases()[0]
                .checks()
                .iter()
                .filter(|check| !check.passed())
                .map(|check| check.kind())
                .collect();
            assert_eq!(failed, [expected_failure]);
        }
    }

    #[test]
    fn scoring_unreviewed_cases_does_not_change_storage_or_prod() {
        let (id, case, pinned) = scoring_case();
        let mut store = InMemoryStore::new();
        assert_eq!(store.register(case.clone()), Ok(id));
        let prod_before = store.prod().clone();

        let _ = score_run(&passing_run(), &[pinned]);

        assert!(store.prod().is_empty());
        assert_eq!(store.prod(), &prod_before);
        assert_eq!(store.load(&id), Ok(case));
    }
}

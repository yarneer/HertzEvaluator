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
//!
//! # Check outcomes
//!
//! A deterministic check is evaluated when, and only when, its [`PinnedCase`]
//! states a requirement for it: a non-empty required-fact list, a non-empty
//! required-tool list, a non-empty required trajectory, or a latency ceiling
//! that is `Some`. Without a requirement there is nothing to compare the run
//! against, and the check reports [`CheckOutcome::NotEvaluated`] rather than
//! passing on an empty requirement. [`CheckResult::passed`] is true only for
//! [`CheckOutcome::Passed`], and [`compare_scorecards`] counts only those, so
//! an unrecorded constraint never reads as a satisfied one.
//!
//! # Comparability
//!
//! Counting only true passes leaves the count dependent on what each scorecard
//! stated a requirement for. Two scorecards therefore carry a passed-check
//! delta only when they judged the same things: the same cases, and for each
//! case the same set of evaluated checks. Otherwise [`compare_scorecards`]
//! reports [`ComparisonOutcome::Incomparable`] with the [`Incomparability`]
//! that applies, and there is no direction to read. This is what keeps a
//! withdrawn requirement, which lowers the count without the run changing a
//! byte, from reading as a regression, and a newly stated one from reading as
//! an improvement or as no change at all.

mod case;
mod compare;
mod file_store;
mod promotion;
mod score {
    include!("score.rs");

    pub(crate) fn restore_scorecard(
        stored: Vec<(CaseId, [(CheckKind, CheckOutcome); 4])>,
    ) -> Scorecard {
        Scorecard {
            cases: stored
                .into_iter()
                .map(|(case_id, checks)| CaseScore {
                    case_id,
                    checks: checks.map(|(kind, outcome)| CheckResult { kind, outcome }),
                })
                .collect(),
        }
    }
}
mod store;

pub use case::{Case, CaseId};
pub use compare::{compare_scorecards, ComparisonOutcome, ComparisonReport, Incomparability};
pub use file_store::FileStore;
pub use promotion::CasePromotion;
pub use score::{
    score_run, CaseScore, CheckKind, CheckOutcome, CheckResult, PinError, PinnedCase, RecordedRun,
    Scorecard,
};
pub use store::{CaseStore, InMemoryStore, ScorecardStore, StoreError};

#[cfg(test)]
mod tests {
    use super::{
        compare_scorecards, score_run, Case, CaseId, CasePromotion, CaseStore, CheckKind,
        CheckOutcome, ComparisonOutcome, FileStore, InMemoryStore, Incomparability, PinnedCase,
        RecordedRun, Scorecard, ScorecardStore, StoreError,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

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
            Some(Duration::from_millis(100)),
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
                .filter(|check| check.failed())
                .map(|check| check.kind())
                .collect();
            assert_eq!(failed, [expected_failure]);
        }
    }

    const NO_REQUIREMENT: [&str; 0] = [];

    fn mixed_requirement_case() -> (CaseId, Case) {
        let mut store = InMemoryStore::new();
        let case = Case::new(
            "Evaluate a run recorded without tool or latency data.",
            ["The regression suite passed."],
        );
        let id = store.register(case.clone()).unwrap();
        (id, case)
    }

    #[test]
    fn a_stated_requirement_a_missed_one_and_an_absent_one_read_differently() {
        let (id, case) = mixed_requirement_case();
        let pinned = PinnedCase::new(id, case, ["search"], NO_REQUIREMENT, None).unwrap();
        let run = RecordedRun::new(
            "The regression suite passed.",
            ["report"],
            ["plan"],
            Duration::from_millis(80),
        );

        let scorecard = score_run(&run, &[pinned]);

        let outcomes: Vec<_> = scorecard.cases()[0]
            .checks()
            .iter()
            .map(|check| (check.kind(), check.outcome()))
            .collect();
        assert_eq!(
            outcomes,
            [
                (CheckKind::RequiredFacts, CheckOutcome::Passed),
                (CheckKind::RequiredTools, CheckOutcome::Failed),
                (CheckKind::Trajectory, CheckOutcome::NotEvaluated),
                (CheckKind::Latency, CheckOutcome::NotEvaluated),
            ]
        );
        assert_eq!(scorecard.passed_checks(), 1);
        assert_eq!(scorecard.evaluated_checks(), 2);
        assert!(!scorecard.passed());
    }

    #[test]
    fn a_case_that_states_no_requirement_verifies_nothing_and_does_not_pass() {
        let mut store = InMemoryStore::new();
        let case = Case::new(
            "Evaluate a run with no recorded constraint.",
            NO_REQUIREMENT,
        );
        let id = store.register(case.clone()).unwrap();
        let pinned = PinnedCase::new(id, case, NO_REQUIREMENT, NO_REQUIREMENT, None).unwrap();

        let scorecard = score_run(&passing_run(), &[pinned]);

        assert!(scorecard.cases()[0]
            .checks()
            .iter()
            .all(|check| check.outcome() == CheckOutcome::NotEvaluated));
        assert_eq!(scorecard.evaluated_checks(), 0);
        assert_eq!(scorecard.passed_checks(), 0);
        assert!(!scorecard.cases()[0].passed());
        assert!(!scorecard.passed());
    }

    #[test]
    fn supplying_a_constraint_the_baseline_lacked_is_not_a_regression() {
        let (id, case) = mixed_requirement_case();
        let unconstrained =
            PinnedCase::new(id, case.clone(), NO_REQUIREMENT, NO_REQUIREMENT, None).unwrap();
        let constrained = PinnedCase::new(
            id,
            case,
            NO_REQUIREMENT,
            NO_REQUIREMENT,
            Some(Duration::from_millis(100)),
        )
        .unwrap();
        let run = RecordedRun::new(
            "The regression suite passed.",
            NO_REQUIREMENT,
            NO_REQUIREMENT,
            Duration::from_millis(500),
        );
        let baseline = score_run(&run, &[unconstrained]);
        let candidate = score_run(&run, &[constrained]);

        assert_eq!(
            baseline.cases()[0].checks()[3].outcome(),
            CheckOutcome::NotEvaluated
        );
        assert_eq!(
            candidate.cases()[0].checks()[3].outcome(),
            CheckOutcome::Failed
        );
        let report = compare_scorecards(&candidate, Some(&baseline));
        assert_ne!(report.outcome(), ComparisonOutcome::Regressed);
        assert_eq!(
            report.outcome(),
            ComparisonOutcome::Incomparable(Incomparability::StatedRequirementsDiffer)
        );
        assert_eq!(report.baseline_passed_checks(), Some(1));
        assert_eq!(report.candidate_passed_checks(), 1);
    }

    #[test]
    fn withdrawing_a_requirement_is_not_a_regression() {
        let (id, case) = mixed_requirement_case();
        let stated = PinnedCase::new(
            id,
            case.clone(),
            ["search", "report"],
            ["plan", "search", "report"],
            Some(Duration::from_millis(100)),
        )
        .unwrap();
        let withdrawn = PinnedCase::new(
            id,
            case,
            ["search", "report"],
            ["plan", "search", "report"],
            None,
        )
        .unwrap();
        let run = RecordedRun::new(
            "No result was recorded.",
            ["search", "report"],
            ["plan", "search", "report", "finish"],
            Duration::from_millis(80),
        );
        let baseline = score_run(&run, &[stated]);
        let candidate = score_run(&run, &[withdrawn]);

        assert_eq!(
            baseline.cases()[0].checks()[3].outcome(),
            CheckOutcome::Passed
        );
        assert_eq!(
            candidate.cases()[0].checks()[3].outcome(),
            CheckOutcome::NotEvaluated
        );
        assert_eq!(baseline.passed_checks(), 3);
        assert_eq!(candidate.passed_checks(), 2);

        let report = compare_scorecards(&candidate, Some(&baseline));
        assert_ne!(report.outcome(), ComparisonOutcome::Regressed);
        assert_eq!(
            report.outcome(),
            ComparisonOutcome::Incomparable(Incomparability::StatedRequirementsDiffer)
        );
        assert_eq!(report.baseline_passed_checks(), Some(3));
        assert_eq!(report.candidate_passed_checks(), 2);
    }

    #[test]
    fn scorecards_covering_different_cases_are_incomparable() {
        let mut store = InMemoryStore::new();
        let covered = Case::new(
            "Evaluate the case the candidate covers.",
            ["The regression suite passed."],
        );
        let other = Case::new(
            "Evaluate the case the baseline covers.",
            ["The deployment was rolled back."],
        );
        let covered_id = store.register(covered.clone()).unwrap();
        let other_id = store.register(other.clone()).unwrap();
        let run = RecordedRun::new(
            "The regression suite passed.",
            ["search"],
            NO_REQUIREMENT,
            Duration::from_millis(80),
        );
        let candidate = score_run(
            &run,
            &[PinnedCase::new(covered_id, covered, ["search"], NO_REQUIREMENT, None).unwrap()],
        );
        let baseline = score_run(
            &run,
            &[PinnedCase::new(other_id, other, ["report"], NO_REQUIREMENT, None).unwrap()],
        );

        assert_ne!(
            candidate.cases()[0].case_id(),
            baseline.cases()[0].case_id()
        );
        assert_eq!(candidate.evaluated_checks(), baseline.evaluated_checks());
        assert_eq!(candidate.passed_checks(), 2);
        assert_eq!(baseline.passed_checks(), 0);

        let report = compare_scorecards(&candidate, Some(&baseline));
        assert_ne!(report.outcome(), ComparisonOutcome::Improved);
        assert_eq!(
            report.outcome(),
            ComparisonOutcome::Incomparable(Incomparability::CaseSetDiffers)
        );
        assert_eq!(report.baseline_passed_checks(), Some(0));
        assert_eq!(report.candidate_passed_checks(), 2);
    }

    #[test]
    fn fewer_passes_on_the_same_evaluated_checks_is_still_a_regression() {
        let (_, _, pinned) = scoring_case();
        let baseline = score_run(&passing_run(), std::slice::from_ref(&pinned));
        let candidate = score_run(
            &RecordedRun::new(
                "No result was recorded.",
                ["search", "report"],
                ["plan", "search", "report", "finish"],
                Duration::from_millis(80),
            ),
            &[pinned],
        );

        assert_eq!(candidate.evaluated_checks(), 4);
        assert_eq!(baseline.evaluated_checks(), 4);

        let report = compare_scorecards(&candidate, Some(&baseline));
        assert_eq!(report.outcome(), ComparisonOutcome::Regressed);
        assert_eq!(report.baseline_passed_checks(), Some(4));
        assert_eq!(report.candidate_passed_checks(), 3);
    }

    #[test]
    fn file_store_restores_an_unevaluated_check() {
        let path = temp_store("unevaluated");
        let (id, case) = mixed_requirement_case();
        let pinned = PinnedCase::new(id, case.clone(), ["search"], NO_REQUIREMENT, None).unwrap();
        let scorecard = score_run(&passing_run(), &[pinned]);
        {
            let mut store = FileStore::open(&path).unwrap();
            store.register(case).unwrap();
            store.save_scorecard("candidate", &scorecard).unwrap();
        }

        let store = FileStore::open(&path).unwrap();
        let loaded = store.load_scorecard("candidate").unwrap();

        assert_eq!(loaded, scorecard);
        assert_eq!(loaded.evaluated_checks(), 2);
        assert_eq!(
            loaded.cases()[0].checks()[2].outcome(),
            CheckOutcome::NotEvaluated
        );

        drop(store);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn a_version_one_store_file_is_rejected_instead_of_reinterpreted() {
        let path = temp_store("legacy");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("store.bin"), b"HERTZ-FILE-1\n").unwrap();

        let opened = FileStore::open(&path);

        match opened {
            Err(StoreError::UnsupportedFormat(message)) => {
                assert!(message.contains("version 1"), "{message}");
            }
            other => panic!("expected an unsupported-format error, got {other:?}"),
        }

        fs::remove_dir_all(path).unwrap();
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

    /// A repeated check kind must not be mistaken for a different kind.
    ///
    /// The verdict surface is a bitmask keyed by [`CheckKind`] so that it does
    /// not depend on array position. Accumulating those bits by addition would
    /// undo that: two `RequiredFacts` verdicts carry into the `RequiredTools`
    /// bit, giving the same mask as one `RequiredTools` verdict, and the two
    /// scorecards would compare as though they had judged the same things.
    #[test]
    fn a_repeated_check_kind_does_not_alias_another_kind() {
        let id = CaseId::from_digest([7; 32]);
        let twice_the_same_kind = super::score::restore_scorecard(vec![(
            id,
            [
                (CheckKind::RequiredFacts, CheckOutcome::Passed),
                (CheckKind::RequiredFacts, CheckOutcome::Passed),
                (CheckKind::Trajectory, CheckOutcome::NotEvaluated),
                (CheckKind::Latency, CheckOutcome::NotEvaluated),
            ],
        )]);
        let the_neighbouring_kind = super::score::restore_scorecard(vec![(
            id,
            [
                (CheckKind::RequiredTools, CheckOutcome::Passed),
                (CheckKind::RequiredFacts, CheckOutcome::NotEvaluated),
                (CheckKind::Trajectory, CheckOutcome::NotEvaluated),
                (CheckKind::Latency, CheckOutcome::NotEvaluated),
            ],
        )]);

        let report = compare_scorecards(&twice_the_same_kind, Some(&the_neighbouring_kind));

        assert_eq!(
            report.outcome(),
            ComparisonOutcome::Incomparable(Incomparability::StatedRequirementsDiffer)
        );
    }

    fn temp_store(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "hertz-evaluator-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn exercise_store<S>(store: &mut S) -> (CaseId, CaseId, Scorecard)
    where
        S: CaseStore + CasePromotion + ScorecardStore,
    {
        let original = case();
        let original_id = store.register(original.clone()).unwrap();
        store.approve(&original_id).unwrap();
        let modified = Case::new(
            "Explain why the modified snapshot is safe.",
            ["The regression suite passed."],
        );
        let modified_id = store.register(modified).unwrap();
        let pinned = PinnedCase::new(
            original_id,
            original,
            ["search", "report"],
            ["plan", "search", "report"],
            Some(Duration::from_millis(100)),
        )
        .unwrap();
        let scorecard = score_run(&passing_run(), &[pinned]);
        store.save_scorecard("candidate", &scorecard).unwrap();
        (original_id, modified_id, scorecard)
    }

    #[test]
    fn file_store_survives_a_new_instance() {
        let path = temp_store("reopen");
        let (original_id, modified_id, expected_scorecard) = {
            let mut store = FileStore::open(&path).unwrap();
            exercise_store(&mut store)
        };

        let store = FileStore::open(&path).unwrap();
        assert_eq!(store.load(&original_id), Ok(case()));
        assert!(store.prod().contains(&original_id));
        assert!(!store.prod().contains(&modified_id));
        let loaded = store.load_scorecard("candidate").unwrap();
        assert_eq!(loaded, expected_scorecard);
        assert_eq!(
            compare_scorecards(&loaded, Some(&expected_scorecard)),
            compare_scorecards(&expected_scorecard, Some(&expected_scorecard))
        );

        drop(store);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn file_and_memory_backends_have_the_same_visible_results() {
        let path = temp_store("parity");
        let mut memory = InMemoryStore::new();
        let mut file = FileStore::open(&path).unwrap();
        let memory_result = exercise_store(&mut memory);
        let file_result = exercise_store(&mut file);

        assert_eq!(memory_result, file_result);
        assert_eq!(memory.prod(), file.prod());
        assert_eq!(memory.load(&memory_result.0), file.load(&file_result.0));
        assert_eq!(
            memory.load_scorecard("candidate"),
            file.load_scorecard("candidate")
        );

        drop(file);
        fs::remove_dir_all(path).unwrap();
    }
}

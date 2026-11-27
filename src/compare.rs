use crate::{CaseScore, CheckKind, Scorecard};
use std::fmt;

/// The aggregate result of comparing a candidate scorecard with a baseline.
///
/// [`Improved`](ComparisonOutcome::Improved),
/// [`Unchanged`](ComparisonOutcome::Unchanged), and
/// [`Regressed`](ComparisonOutcome::Regressed) report a delta in passed checks,
/// and a delta only describes the run when both scorecards judged the same
/// things. When they did not, the outcome is
/// [`Incomparable`](ComparisonOutcome::Incomparable) and there is no delta to
/// read.
///
/// Only a check that passed a stated requirement counts. A check reporting
/// [`crate::CheckOutcome::NotEvaluated`] counts for neither side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonOutcome {
    /// No baseline was supplied.
    FirstRun,
    /// The two scorecards did not judge the same things, so their passed-check
    /// counts are not a delta.
    Incomparable(Incomparability),
    /// The candidate passed more stated requirements than the baseline.
    Improved,
    /// The candidate passed as many stated requirements as the baseline.
    Unchanged,
    /// The candidate passed fewer stated requirements than the baseline.
    Regressed,
}

/// Why two scorecards cannot be subtracted from one another.
///
/// Both variants are one defect: the set of case-and-check pairs that carried a
/// verdict is not the same on both sides, so the two passed-check totals are
/// drawn from different denominators and their difference measures the suite
/// rather than the run. They stay apart because the first is not a special case
/// of the second and because they are repaired differently. A scorecard may
/// cover a case that stated no requirement at all, which changes the covered
/// case set while leaving the evaluated-check set untouched, so the case set is
/// checked first and on its own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Incomparability {
    /// The two scorecards do not cover the same cases.
    CaseSetDiffers,
    /// The two scorecards cover the same cases, but at least one case stated a
    /// different set of requirements, so a different set of checks was
    /// evaluated.
    ///
    /// This is the shape of a withdrawn requirement and of a newly stated one.
    /// Neither is the run getting worse or better, and neither leaves a
    /// passed-check delta that means anything.
    StatedRequirementsDiffer,
}

/// A deterministic summary of passed checks in two scorecards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComparisonReport {
    outcome: ComparisonOutcome,
    baseline_passed_checks: Option<usize>,
    candidate_passed_checks: usize,
}

impl ComparisonReport {
    /// Returns the comparison outcome.
    pub const fn outcome(&self) -> ComparisonOutcome {
        self.outcome
    }

    /// Returns how many stated requirements the baseline passed, or `None` for
    /// a first run.
    ///
    /// Checks reporting [`crate::CheckOutcome::NotEvaluated`] are excluded, so
    /// this is at most the number of checks the baseline evaluated, not the
    /// number it ran.
    ///
    /// This counts one scorecard on its own and its meaning has not changed.
    /// Under [`ComparisonOutcome::Incomparable`] it is still that count, but
    /// the two counts describe different sets of checks, so subtracting one
    /// from the other states nothing about the run.
    pub const fn baseline_passed_checks(&self) -> Option<usize> {
        self.baseline_passed_checks
    }

    /// Returns how many stated requirements the candidate passed.
    ///
    /// Checks reporting [`crate::CheckOutcome::NotEvaluated`] are excluded, so
    /// this is at most the number of checks the candidate evaluated, not the
    /// number it ran.
    ///
    /// This counts one scorecard on its own and its meaning has not changed.
    /// Under [`ComparisonOutcome::Incomparable`] it is still that count, but
    /// the two counts describe different sets of checks, so subtracting one
    /// from the other states nothing about the run.
    pub const fn candidate_passed_checks(&self) -> usize {
        self.candidate_passed_checks
    }
}

impl fmt::Display for ComparisonReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.baseline_passed_checks {
            Some(baseline) => write!(
                formatter,
                "{:?}: baseline={baseline} candidate={}",
                self.outcome, self.candidate_passed_checks
            ),
            None => write!(
                formatter,
                "FirstRun: baseline=missing candidate={}",
                self.candidate_passed_checks
            ),
        }
    }
}

/// Compares passed-check counts without adopting or mutating either scorecard.
///
/// Only checks that passed a stated requirement are counted. An unevaluated
/// check contributes to neither count.
///
/// A count delta is reported only when the two scorecards judged the same
/// things: the same cases, and for each case the same set of evaluated checks.
/// Otherwise the outcome is [`ComparisonOutcome::Incomparable`] with the
/// [`Incomparability`] that applies. Case order does not matter, and a case
/// listed twice must be listed twice on both sides.
///
/// [`ComparisonOutcome::FirstRun`] still means exactly that no baseline was
/// supplied, and a missing baseline is decided before comparability.
///
/// # Why a variant and not a `Result` or a report field
///
/// [`ComparisonOutcome`] is already where "there is no delta to read" lives.
/// [`ComparisonOutcome::FirstRun`] says it for a missing baseline, and
/// incomparability says it for a baseline that measured something else, so the
/// two belong side by side. A variant also removes the misreading instead of
/// labelling it: there is no [`ComparisonOutcome::Improved`] value to pick up,
/// and every exhaustive `match` on the enum has to account for the new case.
///
/// Returning `Result` was rejected because nothing failed. Both scorecards are
/// well formed and the comparison reached a definite conclusion, namely that
/// the two were not measured the same way. An `Err` would also split "no delta"
/// across two channels, with [`ComparisonOutcome::FirstRun`] inside `Ok`, and a
/// caller reporting the situation still wants both passed-check counts, which
/// the error would have to carry a second time.
///
/// Carrying comparability as a separate field on [`ComparisonReport`] was
/// rejected because it would leave [`ComparisonReport::outcome`] returning a
/// direction that reads as normal without the new field ever being consulted.
/// The defect being repaired is precisely a plausible-looking direction, so a
/// shape that keeps one reachable is not a repair.
pub fn compare_scorecards(candidate: &Scorecard, baseline: Option<&Scorecard>) -> ComparisonReport {
    let candidate_passed_checks = candidate.passed_checks();
    let Some(baseline) = baseline else {
        return ComparisonReport {
            outcome: ComparisonOutcome::FirstRun,
            baseline_passed_checks: None,
            candidate_passed_checks,
        };
    };

    let baseline_passed_checks = baseline.passed_checks();
    let outcome = match incomparability(candidate, baseline) {
        Some(reason) => ComparisonOutcome::Incomparable(reason),
        None if candidate_passed_checks > baseline_passed_checks => ComparisonOutcome::Improved,
        None if candidate_passed_checks < baseline_passed_checks => ComparisonOutcome::Regressed,
        None => ComparisonOutcome::Unchanged,
    };

    ComparisonReport {
        outcome,
        baseline_passed_checks: Some(baseline_passed_checks),
        candidate_passed_checks,
    }
}

fn incomparability(candidate: &Scorecard, baseline: &Scorecard) -> Option<Incomparability> {
    let candidate_surface = evaluated_surface(candidate);
    let baseline_surface = evaluated_surface(baseline);
    if candidate_surface
        .iter()
        .map(|(case, _)| case)
        .ne(baseline_surface.iter().map(|(case, _)| case))
    {
        return Some(Incomparability::CaseSetDiffers);
    }
    if candidate_surface != baseline_surface {
        return Some(Incomparability::StatedRequirementsDiffer);
    }
    None
}

/// Returns which checks carried a verdict, as one sorted entry per scored case.
///
/// Sorting by case digest makes the comparison independent of pinned order, and
/// keeping one entry per case rather than collapsing by id keeps a case listed
/// twice distinguishable from a case listed once.
fn evaluated_surface(scorecard: &Scorecard) -> Vec<([u8; 32], u8)> {
    let mut surface: Vec<_> = scorecard
        .cases()
        .iter()
        .map(|case| (case.case_id().digest(), evaluated_kinds(case)))
        .collect();
    surface.sort_unstable();
    surface
}

fn evaluated_kinds(case: &CaseScore) -> u8 {
    case.checks()
        .iter()
        .filter(|check| check.evaluated())
        .map(|check| 1 << kind_index(check.kind()))
        .sum()
}

const fn kind_index(kind: CheckKind) -> u8 {
    match kind {
        CheckKind::RequiredFacts => 0,
        CheckKind::RequiredTools => 1,
        CheckKind::Trajectory => 2,
        CheckKind::Latency => 3,
    }
}

use crate::Scorecard;
use std::fmt;

/// The aggregate result of comparing a candidate scorecard with a baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonOutcome {
    /// No baseline was supplied.
    FirstRun,
    /// The candidate passed more checks than the baseline.
    Improved,
    /// The candidate passed the same number of checks as the baseline.
    Unchanged,
    /// The candidate passed fewer checks than the baseline.
    Regressed,
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

    /// Returns the baseline's passed-check count, or `None` for a first run.
    pub const fn baseline_passed_checks(&self) -> Option<usize> {
        self.baseline_passed_checks
    }

    /// Returns the candidate's passed-check count.
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
pub fn compare_scorecards(candidate: &Scorecard, baseline: Option<&Scorecard>) -> ComparisonReport {
    let candidate_passed_checks = passed_checks(candidate);
    let baseline_passed_checks = baseline.map(passed_checks);
    let outcome = match baseline_passed_checks {
        None => ComparisonOutcome::FirstRun,
        Some(count) if candidate_passed_checks > count => ComparisonOutcome::Improved,
        Some(count) if candidate_passed_checks < count => ComparisonOutcome::Regressed,
        Some(_) => ComparisonOutcome::Unchanged,
    };

    ComparisonReport {
        outcome,
        baseline_passed_checks,
        candidate_passed_checks,
    }
}

fn passed_checks(scorecard: &Scorecard) -> usize {
    scorecard
        .cases()
        .iter()
        .flat_map(|case| case.checks())
        .filter(|check| check.passed())
        .count()
}

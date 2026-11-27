use crate::Scorecard;
use std::fmt;

/// The aggregate result of comparing a candidate scorecard with a baseline.
///
/// Only a check that passed a stated requirement counts. A check reporting
/// [`crate::CheckOutcome::NotEvaluated`] counts for neither side, so supplying
/// a constraint the baseline never stated cannot read as a regression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonOutcome {
    /// No baseline was supplied.
    FirstRun,
    /// The candidate passed more stated requirements than the baseline.
    Improved,
    /// The candidate passed as many stated requirements as the baseline.
    Unchanged,
    /// The candidate passed fewer stated requirements than the baseline.
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

    /// Returns how many stated requirements the baseline passed, or `None` for
    /// a first run.
    ///
    /// Checks reporting [`crate::CheckOutcome::NotEvaluated`] are excluded, so
    /// this is at most the number of checks the baseline evaluated, not the
    /// number it ran.
    pub const fn baseline_passed_checks(&self) -> Option<usize> {
        self.baseline_passed_checks
    }

    /// Returns how many stated requirements the candidate passed.
    ///
    /// Checks reporting [`crate::CheckOutcome::NotEvaluated`] are excluded, so
    /// this is at most the number of checks the candidate evaluated, not the
    /// number it ran.
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
pub fn compare_scorecards(candidate: &Scorecard, baseline: Option<&Scorecard>) -> ComparisonReport {
    let candidate_passed_checks = candidate.passed_checks();
    let baseline_passed_checks = baseline.map(Scorecard::passed_checks);
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

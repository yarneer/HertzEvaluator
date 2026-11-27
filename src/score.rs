use crate::{Case, CaseId};
use std::{error::Error, fmt, time::Duration};

/// One completed run, recorded before scoring begins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedRun {
    output: String,
    tools: Vec<String>,
    trajectory: Vec<String>,
    elapsed: Duration,
}

impl RecordedRun {
    /// Creates a recorded run without starting an agent or making a model call.
    pub fn new(
        output: impl Into<String>,
        tools: impl IntoIterator<Item = impl Into<String>>,
        trajectory: impl IntoIterator<Item = impl Into<String>>,
        elapsed: Duration,
    ) -> Self {
        Self {
            output: output.into(),
            tools: tools.into_iter().map(Into::into).collect(),
            trajectory: trajectory.into_iter().map(Into::into).collect(),
            elapsed,
        }
    }
}

/// An exact case version and its deterministic run constraints.
///
/// Every constraint is optional, and an absent constraint is not a satisfied
/// one. A constraint left out here makes its check report
/// [`CheckOutcome::NotEvaluated`] instead of passing on an empty requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedCase {
    id: CaseId,
    case: Case,
    required_tools: Vec<String>,
    required_trajectory: Vec<String>,
    latency_ceiling: Option<Duration>,
}

impl PinnedCase {
    /// Pins a case id and configures exact tool names, ordered trajectory
    /// steps, and an inclusive latency ceiling.
    ///
    /// An empty tool list, an empty trajectory, or a `None` ceiling states no
    /// requirement, and the matching check reports
    /// [`CheckOutcome::NotEvaluated`].
    pub fn new(
        id: CaseId,
        case: Case,
        required_tools: impl IntoIterator<Item = impl Into<String>>,
        required_trajectory: impl IntoIterator<Item = impl Into<String>>,
        latency_ceiling: Option<Duration>,
    ) -> Result<Self, PinError> {
        let actual = case.id();
        if id != actual {
            return Err(PinError::IdMismatch {
                supplied: id,
                actual,
            });
        }
        Ok(Self {
            id,
            case,
            required_tools: required_tools.into_iter().map(Into::into).collect(),
            required_trajectory: required_trajectory.into_iter().map(Into::into).collect(),
            latency_ceiling,
        })
    }

    /// Returns the exact content address being scored.
    pub const fn id(&self) -> CaseId {
        self.id
    }
}

/// Failure to pair a case with its exact content address.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PinError {
    /// The supplied id does not address the supplied case.
    IdMismatch { supplied: CaseId, actual: CaseId },
}

impl fmt::Display for PinError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdMismatch { supplied, actual } => {
                write!(formatter, "case id {supplied} does not match {actual}")
            }
        }
    }
}

impl Error for PinError {}

/// A deterministic check category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckKind {
    RequiredFacts,
    RequiredTools,
    Trajectory,
    Latency,
}

/// What one deterministic check concluded about a recorded run.
///
/// A check is evaluated when, and only when, the pinned case states a
/// requirement for it. Without a requirement there is nothing to compare the
/// run against, so the check reports [`CheckOutcome::NotEvaluated`] rather than
/// passing on an empty requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckOutcome {
    /// The case stated a requirement and the run satisfied it.
    Passed,
    /// The case stated a requirement and the run did not satisfy it.
    Failed,
    /// The case stated no requirement, so the run was not judged.
    NotEvaluated,
}

/// The result of one deterministic check category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckResult {
    kind: CheckKind,
    outcome: CheckOutcome,
}

impl CheckResult {
    /// Returns the check category.
    pub const fn kind(&self) -> CheckKind {
        self.kind
    }

    /// Returns what this check concluded.
    pub const fn outcome(&self) -> CheckOutcome {
        self.outcome
    }

    /// Returns whether the case stated a requirement that the run satisfied.
    ///
    /// A check with no requirement to compare against is not a pass.
    pub const fn passed(&self) -> bool {
        matches!(self.outcome, CheckOutcome::Passed)
    }

    /// Returns whether the case stated a requirement that the run missed.
    pub const fn failed(&self) -> bool {
        matches!(self.outcome, CheckOutcome::Failed)
    }

    /// Returns whether the case stated a requirement at all.
    pub const fn evaluated(&self) -> bool {
        !matches!(self.outcome, CheckOutcome::NotEvaluated)
    }
}

/// All check results for one pinned case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseScore {
    case_id: CaseId,
    checks: [CheckResult; 4],
}

impl CaseScore {
    /// Returns the exact case version scored.
    pub const fn case_id(&self) -> CaseId {
        self.case_id
    }

    /// Returns one result for each deterministic check category.
    pub const fn checks(&self) -> &[CheckResult; 4] {
        &self.checks
    }

    /// Returns whether this case is evidence that the run was correct.
    ///
    /// This requires both that no check failed and that at least one check was
    /// evaluated. A case whose every check reports
    /// [`CheckOutcome::NotEvaluated`] states no requirement at all, so it
    /// verified nothing and cannot stand in for a pass. Counting it as one is
    /// what let cases with no tool, trajectory, or latency requirement score
    /// the same as cases that met every requirement they stated.
    pub fn passed(&self) -> bool {
        self.checks.iter().any(CheckResult::passed) && !self.checks.iter().any(CheckResult::failed)
    }

    /// Returns how many checks were evaluated against a stated requirement.
    pub fn evaluated_checks(&self) -> usize {
        self.checks.iter().filter(|check| check.evaluated()).count()
    }

    /// Returns how many checks passed a stated requirement.
    pub fn passed_checks(&self) -> usize {
        self.checks.iter().filter(|check| check.passed()).count()
    }
}

/// Deterministic results for one recorded run and a pinned set of cases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scorecard {
    cases: Vec<CaseScore>,
}

impl Scorecard {
    /// Returns case results in the same order as the pinned input.
    pub fn cases(&self) -> &[CaseScore] {
        &self.cases
    }

    /// Returns whether every pinned case passed, by [`CaseScore::passed`].
    ///
    /// An empty scorecard does not pass. A suite that scored no case, like a
    /// case whose every check was not evaluated, produced no evidence, and a
    /// regression gate must not open on none.
    pub fn passed(&self) -> bool {
        !self.cases.is_empty() && self.cases.iter().all(CaseScore::passed)
    }

    /// Returns how many checks were evaluated against a stated requirement.
    pub fn evaluated_checks(&self) -> usize {
        self.cases.iter().map(CaseScore::evaluated_checks).sum()
    }

    /// Returns how many checks passed a stated requirement.
    pub fn passed_checks(&self) -> usize {
        self.cases.iter().map(CaseScore::passed_checks).sum()
    }
}

/// Scores an already-recorded run without mutating case storage.
///
/// Each check reports [`CheckOutcome::NotEvaluated`] when its pinned case
/// states no requirement for it, so an unrecorded constraint never reads as a
/// satisfied one.
pub fn score_run(run: &RecordedRun, pinned_cases: &[PinnedCase]) -> Scorecard {
    let cases = pinned_cases
        .iter()
        .map(|pinned| CaseScore {
            case_id: pinned.id,
            checks: [
                result(
                    CheckKind::RequiredFacts,
                    judge(pinned.case.required_facts(), |facts| {
                        facts.iter().all(|fact| run.output.contains(fact))
                    }),
                ),
                result(
                    CheckKind::RequiredTools,
                    judge(&pinned.required_tools, |tools| {
                        tools.iter().all(|tool| run.tools.contains(tool))
                    }),
                ),
                result(
                    CheckKind::Trajectory,
                    judge(&pinned.required_trajectory, |steps| {
                        is_ordered_subsequence(steps, &run.trajectory)
                    }),
                ),
                result(
                    CheckKind::Latency,
                    pinned
                        .latency_ceiling
                        .map(|ceiling| run.elapsed <= ceiling),
                ),
            ],
        })
        .collect();
    Scorecard { cases }
}

fn result(kind: CheckKind, judgment: Option<bool>) -> CheckResult {
    let outcome = match judgment {
        Some(true) => CheckOutcome::Passed,
        Some(false) => CheckOutcome::Failed,
        None => CheckOutcome::NotEvaluated,
    };
    CheckResult { kind, outcome }
}

fn judge(required: &[String], satisfied: impl FnOnce(&[String]) -> bool) -> Option<bool> {
    if required.is_empty() {
        return None;
    }
    Some(satisfied(required))
}

fn is_ordered_subsequence(required: &[String], actual: &[String]) -> bool {
    let mut position = 0;
    for step in required {
        let Some(offset) = actual[position..].iter().position(|item| item == step) else {
            return false;
        };
        position += offset + 1;
    }
    true
}

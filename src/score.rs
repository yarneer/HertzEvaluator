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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedCase {
    id: CaseId,
    case: Case,
    required_tools: Vec<String>,
    required_trajectory: Vec<String>,
    latency_ceiling: Duration,
}

impl PinnedCase {
    /// Pins a case id and configures exact tool names, ordered trajectory
    /// steps, and an inclusive latency ceiling.
    pub fn new(
        id: CaseId,
        case: Case,
        required_tools: impl IntoIterator<Item = impl Into<String>>,
        required_trajectory: impl IntoIterator<Item = impl Into<String>>,
        latency_ceiling: Duration,
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

/// The result of one deterministic check category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckResult {
    kind: CheckKind,
    passed: bool,
}

impl CheckResult {
    /// Returns the check category.
    pub const fn kind(&self) -> CheckKind {
        self.kind
    }

    /// Returns whether the recorded run satisfied this check.
    pub const fn passed(&self) -> bool {
        self.passed
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

    /// Returns whether every check passed.
    pub fn passed(&self) -> bool {
        self.checks.iter().all(CheckResult::passed)
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

    /// Returns whether every check for every pinned case passed.
    pub fn passed(&self) -> bool {
        self.cases.iter().all(CaseScore::passed)
    }
}

/// Scores an already-recorded run without mutating case storage.
pub fn score_run(run: &RecordedRun, pinned_cases: &[PinnedCase]) -> Scorecard {
    let cases = pinned_cases
        .iter()
        .map(|pinned| CaseScore {
            case_id: pinned.id,
            checks: [
                result(
                    CheckKind::RequiredFacts,
                    pinned
                        .case
                        .required_facts()
                        .iter()
                        .all(|fact| run.output.contains(fact)),
                ),
                result(
                    CheckKind::RequiredTools,
                    pinned
                        .required_tools
                        .iter()
                        .all(|tool| run.tools.contains(tool)),
                ),
                result(
                    CheckKind::Trajectory,
                    is_ordered_subsequence(&pinned.required_trajectory, &run.trajectory),
                ),
                result(CheckKind::Latency, run.elapsed <= pinned.latency_ceiling),
            ],
        })
        .collect();
    Scorecard { cases }
}

fn result(kind: CheckKind, passed: bool) -> CheckResult {
    CheckResult { kind, passed }
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

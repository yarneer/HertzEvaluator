use hertz_evaluator::{
    score_run, Case, CasePromotion, CaseStore, CheckKind, CheckOutcome, InMemoryStore, PinnedCase,
    RecordedRun, Scorecard,
};
use std::{fs, process::ExitCode, time::Duration, time::Instant};

const FIXTURES: [(&str, Option<CheckKind>); 5] = [
    ("pass.run", None),
    ("fail_fact.run", Some(CheckKind::RequiredFacts)),
    ("fail_tool.run", Some(CheckKind::RequiredTools)),
    ("fail_trajectory.run", Some(CheckKind::Trajectory)),
    ("fail_latency.run", Some(CheckKind::Latency)),
];

fn main() -> ExitCode {
    let result = if std::env::args().nth(1).as_deref() == Some("--bench-1000") {
        bench_1000()
    } else {
        demonstrate()
    };

    match result {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("scoring probe failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn demonstrate() -> Result<String, String> {
    let (store, pinned) = pinned_case()?;
    let prod_before = store.prod().clone();
    let mut output = format!("prod before: {}\n", store.prod().len());

    for (name, expected_failure) in FIXTURES {
        let run = read_fixture(name)?;
        let first = score_run(&run, std::slice::from_ref(&pinned));
        let second = score_run(&run, std::slice::from_ref(&pinned));
        if first != second {
            return Err(format!("{name} produced different scorecards"));
        }

        let failed: Vec<_> = first.cases()[0]
            .checks()
            .iter()
            .filter(|check| check.failed())
            .map(|check| check.kind())
            .collect();
        let expected: Vec<_> = expected_failure.into_iter().collect();
        if failed != expected {
            return Err(format!("{name} failed {failed:?}, expected {expected:?}"));
        }
        output.push_str(&render(name, &first));
    }

    if store.prod() != &prod_before {
        return Err("scoring changed prod".into());
    }
    output.push_str(&format!("prod after: {}\n", store.prod().len()));
    Ok(output)
}

fn bench_1000() -> Result<String, String> {
    let (_, pinned) = pinned_case()?;
    let run = read_fixture("pass.run")?;
    let started = Instant::now();
    for _ in 0..1_000 {
        if !score_run(&run, std::slice::from_ref(&pinned)).passed() {
            return Err("passing fixture failed during benchmark".into());
        }
    }
    Ok(format!(
        "scored 1000 recorded runs in {:.3?}\n",
        started.elapsed()
    ))
}

fn pinned_case() -> Result<(InMemoryStore, PinnedCase), String> {
    let case = Case::new(
        "Evaluate a recorded deployment check.",
        ["The regression suite passed."],
    );
    let mut store = InMemoryStore::new();
    let id = store
        .register(case.clone())
        .map_err(|error| error.to_string())?;
    let pinned = PinnedCase::new(
        id,
        case,
        ["search", "report"],
        ["plan", "search", "report"],
        Some(Duration::from_millis(100)),
    )
    .map_err(|error| error.to_string())?;
    Ok((store, pinned))
}

fn read_fixture(name: &str) -> Result<RecordedRun, String> {
    let path = format!("{}/examples/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut lines = content.lines();
    let output = value(lines.next(), "output")?;
    let tools = list(value(lines.next(), "tools")?);
    let trajectory = list(value(lines.next(), "trajectory")?);
    let elapsed_ms = value(lines.next(), "elapsed_ms")?
        .parse()
        .map_err(|error| format!("invalid elapsed_ms in {name}: {error}"))?;
    if lines.next().is_some() {
        return Err(format!("unexpected extra line in {name}"));
    }
    Ok(RecordedRun::new(
        output,
        tools,
        trajectory,
        Duration::from_millis(elapsed_ms),
    ))
}

fn value<'a>(line: Option<&'a str>, key: &str) -> Result<&'a str, String> {
    line.and_then(|line| line.strip_prefix(&format!("{key}=")))
        .ok_or_else(|| format!("missing {key}"))
}

fn list(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').filter(|item| !item.is_empty())
}

fn label(outcome: CheckOutcome) -> &'static str {
    match outcome {
        CheckOutcome::Passed => "pass",
        CheckOutcome::Failed => "fail",
        CheckOutcome::NotEvaluated => "not-evaluated",
    }
}

fn render(name: &str, scorecard: &Scorecard) -> String {
    let checks = scorecard.cases()[0]
        .checks()
        .iter()
        .map(|check| format!("{:?}={}", check.kind(), label(check.outcome())))
        .collect::<Vec<_>>()
        .join(",");
    format!("{name}: {checks}\n")
}

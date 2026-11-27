use hertz_evaluator::{
    compare_scorecards, score_run, Case, CaseStore, ComparisonOutcome, InMemoryStore, PinnedCase,
    RecordedRun, Scorecard,
};
use std::{
    fs,
    hint::black_box,
    process::ExitCode,
    time::{Duration, Instant},
};

// Exit 0 means every expected outcome and deterministic repeat matched; exit 1
// means fixture loading or a comparison check failed.
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
            eprintln!("comparison probe failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn demonstrate() -> Result<String, String> {
    let passing = score_fixture("pass.run")?;
    let fact_failure = score_fixture("fail_fact.run")?;
    let latency_failure = score_fixture("fail_latency.run")?;

    let improved = compare_scorecards(&passing, Some(&fact_failure));
    let improved_repeat = compare_scorecards(&passing, Some(&fact_failure));
    let unchanged = compare_scorecards(&passing, Some(&passing));
    let regressed = compare_scorecards(&latency_failure, Some(&passing));
    let first_run = compare_scorecards(&passing, None);

    expect(improved.outcome(), ComparisonOutcome::Improved)?;
    expect(unchanged.outcome(), ComparisonOutcome::Unchanged)?;
    expect(regressed.outcome(), ComparisonOutcome::Regressed)?;
    expect(first_run.outcome(), ComparisonOutcome::FirstRun)?;
    if improved != improved_repeat {
        return Err("repeated comparison changed".into());
    }

    Ok(format!(
        "improved: {improved}\n\
         unchanged: {unchanged}\n\
         regressed: {regressed}\n\
         no baseline: {first_run}\n\
         repeated comparison 1: {improved}\n\
         repeated comparison 2: {improved_repeat}\n"
    ))
}

fn bench_1000() -> Result<String, String> {
    let candidate = score_fixture("pass.run")?;
    let baseline = score_fixture("fail_fact.run")?;
    let started = Instant::now();
    for _ in 0..1_000 {
        let report = compare_scorecards(black_box(&candidate), Some(black_box(&baseline)));
        if report.outcome() != ComparisonOutcome::Improved {
            return Err("benchmark comparison was not improved".into());
        }
        black_box(report);
    }
    Ok(format!(
        "compared 1000 scorecard pairs in {:.3?}\n",
        started.elapsed()
    ))
}

fn expect(actual: ComparisonOutcome, expected: ComparisonOutcome) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {actual:?}"))
    }
}

fn score_fixture(name: &str) -> Result<Scorecard, String> {
    let case = Case::new(
        "Evaluate a recorded deployment check.",
        ["The regression suite passed."],
    );
    let id = InMemoryStore::new()
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
    Ok(score_run(
        &read_fixture(name)?,
        std::slice::from_ref(&pinned),
    ))
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

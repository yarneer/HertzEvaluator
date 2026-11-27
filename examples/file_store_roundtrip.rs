use hertz_evaluator::{
    compare_scorecards, score_run, Case, CasePromotion, CaseStore, FileStore, InMemoryStore,
    PinnedCase, RecordedRun, ScorecardStore,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [mode, path] if mode == "--write" => write_store(Path::new(path), 1, true),
        [mode, path] if mode == "--read" => read_store(Path::new(path), 1, true),
        [mode, path] if mode == "--bench-write" => write_store(Path::new(path), 1_000, false),
        [mode, path] if mode == "--bench-read" => read_store(Path::new(path), 1_000, false),
        [mode] if mode == "--bench-1000" => bench_1000(),
        [] => roundtrip(),
        _ => Err("usage: file_store_roundtrip [--bench-1000]".into()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("file-store probe failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn roundtrip() -> Result<(), String> {
    let path = temp_store("roundtrip");
    let writer = run_child("--write", &path)?;
    let expected =
        fs::read_to_string(path.join("expected.txt")).map_err(|error| error.to_string())?;
    if writer != expected {
        return Err("process 1 output did not match its recorded values".into());
    }

    let reader = run_child("--read", &path)?;
    if reader != expected {
        return Err("process 2 values did not match process 1".into());
    }
    print!("{reader}");
    fs::remove_dir_all(path).map_err(|error| error.to_string())
}

fn bench_1000() -> Result<(), String> {
    let started = Instant::now();
    let mut memory = InMemoryStore::new();
    for index in 0..1_000 {
        let case = benchmark_case(index);
        let id = memory.register(case.clone()).map_err(|e| e.to_string())?;
        if memory.load(&id).map_err(|e| e.to_string())? != case {
            return Err(format!("in-memory case {index} changed"));
        }
    }
    let control = started.elapsed();

    let path = temp_store("bench");
    let started = Instant::now();
    run_child("--bench-write", &path)?;
    run_child("--bench-read", &path)?;
    let file_elapsed = started.elapsed();
    fs::remove_dir_all(path).map_err(|error| error.to_string())?;

    println!("in-memory 1000-case register+load: {control:.3?}");
    println!("file-store 1000-case write+read: {file_elapsed:.3?}");
    println!(
        "file/in-memory ratio: {:.2}x",
        file_elapsed.as_secs_f64() / control.as_secs_f64()
    );
    if file_elapsed > control.saturating_mul(50) {
        return Err(format!(
            "file-store probe exceeded 50x control ({file_elapsed:.3?} > {:.3?})",
            control.saturating_mul(50)
        ));
    }
    Ok(())
}

fn run_child(mode: &str, path: &Path) -> Result<String, String> {
    let output = Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
        .arg(mode)
        .arg(path)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn write_store(path: &Path, count: usize, record: bool) -> Result<(), String> {
    let mut store = FileStore::open(path).map_err(|error| error.to_string())?;
    let first = benchmark_case(0);
    let mut first_id = None;
    for index in 0..count {
        let case = benchmark_case(index);
        let id = store
            .register(case.clone())
            .map_err(|error| error.to_string())?;
        store.approve(&id).map_err(|error| error.to_string())?;
        if index == 0 {
            first_id = Some(id);
        }
    }
    let id = first_id.ok_or_else(|| "no cases were written".to_string())?;
    let scorecard = score(&first, id)?;
    store
        .save_scorecard("candidate", &scorecard)
        .map_err(|error| error.to_string())?;

    if record {
        let summary = summary(&store, id, &scorecard)?;
        fs::write(path.join("expected.txt"), &summary).map_err(|error| error.to_string())?;
        print!("{summary}");
    }
    Ok(())
}

fn read_store(path: &Path, count: usize, print_summary: bool) -> Result<(), String> {
    let store = FileStore::open(path).map_err(|error| error.to_string())?;
    for index in 0..count {
        let case = benchmark_case(index);
        let id = InMemoryStore::new()
            .register(case.clone())
            .map_err(|error| error.to_string())?;
        if store.load(&id).map_err(|error| error.to_string())? != case {
            return Err(format!("file case {index} changed"));
        }
        if !store.prod().contains(&id) {
            return Err(format!("file case {index} was not in prod"));
        }
    }
    let first = benchmark_case(0);
    let first_id = InMemoryStore::new()
        .register(first)
        .map_err(|error| error.to_string())?;
    let scorecard = store
        .load_scorecard("candidate")
        .map_err(|error| error.to_string())?;
    if print_summary {
        print!("{}", summary(&store, first_id, &scorecard)?);
    }
    Ok(())
}

fn score(case: &Case, id: hertz_evaluator::CaseId) -> Result<hertz_evaluator::Scorecard, String> {
    let pinned = PinnedCase::new(
        id,
        case.clone(),
        ["search", "report"],
        ["plan", "search", "report"],
        Some(Duration::from_millis(100)),
    )
    .map_err(|error| error.to_string())?;
    Ok(score_run(
        &RecordedRun::new(
            "Recorded run number 0 completed.",
            ["search", "report"],
            ["plan", "search", "report", "finish"],
            Duration::from_millis(80),
        ),
        &[pinned],
    ))
}

fn summary(
    store: &(impl CasePromotion + ScorecardStore),
    id: hertz_evaluator::CaseId,
    scorecard: &hertz_evaluator::Scorecard,
) -> Result<String, String> {
    let mut prod: Vec<_> = store.prod().iter().map(ToString::to_string).collect();
    prod.sort();
    let persisted = store
        .load_scorecard("candidate")
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "case_id={id}\nprod={}\ncomparison={}\n",
        prod.join(","),
        compare_scorecards(scorecard, Some(&persisted))
    ))
}

fn benchmark_case(index: usize) -> Case {
    Case::new(
        format!("Evaluate distinct recorded run {index}."),
        [format!("Recorded run number {index} completed.")],
    )
}

fn temp_store(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "hertz-evaluator-example-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

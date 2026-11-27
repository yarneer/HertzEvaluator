use hertz_evaluator::{Case, CaseId, CaseStore, InMemoryStore, StoreError};
use std::{process::ExitCode, time::Instant};

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--bench-10k") {
        return match bench_10k() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("10,000-case probe failed: {error}");
                ExitCode::FAILURE
            }
        };
    }

    match demonstrate_not_found() {
        Err(StoreError::NotFound(id)) => {
            eprintln!("not-found error for forged id {id}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("unexpected store error: {error}");
            ExitCode::FAILURE
        }
        Ok(()) => {
            eprintln!("forged id unexpectedly loaded");
            ExitCode::FAILURE
        }
    }
}

fn demonstrate_not_found() -> Result<(), StoreError> {
    let mut store = InMemoryStore::new();
    let original = Case::new(
        "Explain why the snapshot is safe.",
        ["The regression suite passed."],
    );
    let changed_fact = Case::new(
        "Explain why the snapshot is safe.",
        ["The production regression suite passed."],
    );

    let first_id = store.register(original.clone())?;
    let repeated_id = store.register(original.clone())?;
    let changed_id = store.register(changed_fact)?;
    assert_eq!(first_id, repeated_id);
    assert_ne!(first_id, changed_id);

    println!("first registration:  {first_id}");
    println!("same case again:     {repeated_id}");
    println!("changed required fact: {changed_id}");

    let loaded = store.load(&first_id)?;
    assert_eq!(loaded, original);
    println!("loaded content matches: {}", loaded == original);

    store.load(&CaseId::from_digest([0; 32]))?;
    Ok(())
}

fn bench_10k() -> Result<(), StoreError> {
    const CASE_COUNT: usize = 10_000;

    let started = Instant::now();
    let mut store = InMemoryStore::new();
    for index in 0..CASE_COUNT {
        let case = Case::new(
            format!("Evaluate distinct recorded run {index}."),
            [format!("Recorded run number {index} completed.")],
        );
        let id = store.register(case.clone())?;
        if store.load(&id)? != case {
            return Err(StoreError::HashCollision(id));
        }
    }

    println!(
        "registered and loaded {CASE_COUNT} distinct cases in {:.3?}",
        started.elapsed()
    );
    Ok(())
}

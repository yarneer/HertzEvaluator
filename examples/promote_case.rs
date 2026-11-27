use hertz_evaluator::{Case, CaseId, CasePromotion, CaseStore, InMemoryStore, StoreError};
use std::{process::ExitCode, time::Instant};

fn main() -> ExitCode {
    let result = if std::env::args().nth(1).as_deref() == Some("--bench-1k") {
        bench_1k()
    } else {
        demonstrate_promotion()
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("promotion example failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn demonstrate_promotion() -> Result<(), StoreError> {
    let mut store = InMemoryStore::new();
    let first = Case::new("Evaluate the approved run.", ["The run passed."]);
    let unreviewed = Case::new("Evaluate the unreviewed run.", ["The run was recorded."]);
    let first_id = store.register(first)?;
    let unreviewed_id = store.register(unreviewed)?;

    store.approve(&first_id)?;
    assert_eq!(store.prod().len(), 1);
    assert!(store.prod().contains(&first_id));
    assert!(!store.prod().contains(&unreviewed_id));
    println!("prod after approval: {first_id}");

    let missing = CaseId::from_digest([0; 32]);
    assert_eq!(store.approve(&missing), Err(StoreError::NotFound(missing)));
    println!("unregistered promotion rejected: {missing}");

    let modified_id = store.register(Case::new(
        "Evaluate the approved run with changed wording.",
        ["The run passed."],
    ))?;
    assert_ne!(modified_id, first_id);
    assert_eq!(store.prod().len(), 1);
    assert!(store.prod().contains(&first_id));
    println!("prod after modification: {first_id}");

    store.demote(&first_id)?;
    assert!(store.prod().is_empty());
    println!("prod after demotion: empty");
    Ok(())
}

fn bench_1k() -> Result<(), StoreError> {
    const CASE_COUNT: usize = 1_000;
    let mut store = InMemoryStore::new();

    let register_started = Instant::now();
    let ids = (0..CASE_COUNT)
        .map(|index| {
            store.register(Case::new(
                format!("Evaluate promotion candidate {index}."),
                [format!("Candidate {index} passed.")],
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let register_elapsed = register_started.elapsed();

    let promotion_started = Instant::now();
    for id in &ids {
        store.approve(id)?;
        store.demote(id)?;
    }
    let promotion_elapsed = promotion_started.elapsed();

    assert!(store.prod().is_empty());
    println!("registered {CASE_COUNT} cases in {register_elapsed:.3?}");
    println!("approved and demoted {CASE_COUNT} cases in {promotion_elapsed:.3?}");
    Ok(())
}

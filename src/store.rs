use crate::{Case, CaseId, CasePromotion};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

/// Storage operations for immutable, content-addressed cases.
pub trait CaseStore {
    /// Registers a case and returns its stable content address.
    fn register(&mut self, case: Case) -> Result<CaseId, StoreError>;

    /// Loads the exact case addressed by `id`.
    fn load(&self, id: &CaseId) -> Result<Case, StoreError>;
}

/// Errors returned by a [`CaseStore`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// No case is registered under this id.
    NotFound(CaseId),
    /// Different content unexpectedly produced an occupied id.
    HashCollision(CaseId),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "case not found: {id}"),
            Self::HashCollision(id) => write!(formatter, "case id collision: {id}"),
        }
    }
}

impl Error for StoreError {}

/// A process-local case store backed by memory.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    cases: HashMap<CaseId, Case>,
    prod: HashSet<CaseId>,
}

impl InMemoryStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl CaseStore for InMemoryStore {
    fn register(&mut self, case: Case) -> Result<CaseId, StoreError> {
        let id = case.id();
        match self.cases.get(&id) {
            Some(existing) if existing != &case => Err(StoreError::HashCollision(id)),
            Some(_) => Ok(id),
            None => {
                self.cases.insert(id, case);
                Ok(id)
            }
        }
    }

    fn load(&self, id: &CaseId) -> Result<Case, StoreError> {
        self.cases.get(id).cloned().ok_or(StoreError::NotFound(*id))
    }
}

impl CasePromotion for InMemoryStore {
    fn approve(&mut self, id: &CaseId) -> Result<(), StoreError> {
        if !self.cases.contains_key(id) {
            return Err(StoreError::NotFound(*id));
        }
        self.prod.insert(*id);
        Ok(())
    }

    fn demote(&mut self, id: &CaseId) -> Result<(), StoreError> {
        if !self.cases.contains_key(id) {
            return Err(StoreError::NotFound(*id));
        }
        self.prod.remove(id);
        Ok(())
    }

    fn prod(&self) -> &HashSet<CaseId> {
        &self.prod
    }
}

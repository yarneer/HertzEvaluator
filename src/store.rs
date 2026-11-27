use crate::{Case, CaseId, CasePromotion, Scorecard};
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

/// Storage operations for named deterministic scorecards.
pub trait ScorecardStore {
    /// Saves or replaces a scorecard under `name`.
    fn save_scorecard(&mut self, name: &str, scorecard: &Scorecard) -> Result<(), StoreError>;

    /// Loads the scorecard saved under `name`.
    fn load_scorecard(&self, name: &str) -> Result<Scorecard, StoreError>;
}

/// Errors returned by a [`CaseStore`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// No case is registered under this id.
    NotFound(CaseId),
    /// Different content unexpectedly produced an occupied id.
    HashCollision(CaseId),
    /// No scorecard is stored under this name.
    ScorecardNotFound(String),
    /// A filesystem operation failed.
    Io(String),
    /// Stored bytes could not be decoded or failed validation.
    InvalidData(String),
    /// Stored bytes were written by an unsupported store format version.
    UnsupportedFormat(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "case not found: {id}"),
            Self::HashCollision(id) => write!(formatter, "case id collision: {id}"),
            Self::ScorecardNotFound(name) => write!(formatter, "scorecard not found: {name}"),
            Self::Io(message) => write!(formatter, "file store I/O error: {message}"),
            Self::InvalidData(message) => write!(formatter, "invalid file store data: {message}"),
            Self::UnsupportedFormat(message) => {
                write!(formatter, "unsupported file store format: {message}")
            }
        }
    }
}

impl Error for StoreError {}

/// A process-local case store backed by memory.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    cases: HashMap<CaseId, Case>,
    prod: HashSet<CaseId>,
    scorecards: HashMap<String, Scorecard>,
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

impl ScorecardStore for InMemoryStore {
    fn save_scorecard(&mut self, name: &str, scorecard: &Scorecard) -> Result<(), StoreError> {
        self.scorecards.insert(name.to_owned(), scorecard.clone());
        Ok(())
    }

    fn load_scorecard(&self, name: &str) -> Result<Scorecard, StoreError> {
        self.scorecards
            .get(name)
            .cloned()
            .ok_or_else(|| StoreError::ScorecardNotFound(name.to_owned()))
    }
}

use crate::{CaseId, StoreError};
use std::collections::HashSet;

/// Promotion operations for exact, registered case versions.
pub trait CasePromotion {
    /// Approves a registered case version and adds it to `prod`.
    fn approve(&mut self, id: &CaseId) -> Result<(), StoreError>;

    /// Removes a registered case version from `prod` without deleting it.
    fn demote(&mut self, id: &CaseId) -> Result<(), StoreError>;

    /// Returns the exact case versions currently in `prod`.
    fn prod(&self) -> &HashSet<CaseId>;
}

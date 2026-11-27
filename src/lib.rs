//! An independent evaluator and regression gate for agent snapshots.
//!
//! # Case identity encoding
//!
//! [`CaseId`] version 1 is SHA-256 over this canonical byte sequence:
//!
//! 1. the ASCII bytes `hertz-evaluator.case.v1`, followed by one zero byte;
//! 2. the case wording as an unsigned 64-bit big-endian byte length followed by
//!    its UTF-8 bytes;
//! 3. the number of required-fact checks as an unsigned 64-bit big-endian
//!    integer; and
//! 4. each required fact, in order, as an unsigned 64-bit big-endian byte
//!    length followed by its UTF-8 bytes.
//!
//! Strings are encoded exactly as supplied: no trimming or Unicode
//! normalization is performed. Check order is significant. The display form
//! is `case-v1-sha256:` followed by the lowercase hexadecimal digest. This
//! encoding and hash are frozen for version 1; any future change must use a new
//! `CaseId` version.

mod case;
mod promotion;
mod store;

pub use case::{Case, CaseId};
pub use promotion::CasePromotion;
pub use store::{CaseStore, InMemoryStore, StoreError};

#[cfg(test)]
mod tests {
    use super::{Case, CaseId, CasePromotion, CaseStore, InMemoryStore, StoreError};

    fn case() -> Case {
        Case::new(
            "Explain why the snapshot is safe.",
            ["The regression suite passed."],
        )
    }

    #[test]
    fn same_content_has_same_id() {
        let mut store = InMemoryStore::new();

        let first = store.register(case()).unwrap();
        let second = store.register(case()).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first.to_string(),
            "case-v1-sha256:c40e83946e79db11ae7c1a939a42806e0fb378e9cea7d451208d4a1a73d5bc59"
        );
    }

    #[test]
    fn material_edits_have_new_ids() {
        let mut store = InMemoryStore::new();
        let original = store.register(case()).unwrap();
        let wording_edit = store
            .register(Case::new(
                "Explain why this snapshot is safe.",
                ["The regression suite passed."],
            ))
            .unwrap();
        let check_edit = store
            .register(Case::new(
                "Explain why the snapshot is safe.",
                ["The production regression suite passed."],
            ))
            .unwrap();

        assert_ne!(original, wording_edit);
        assert_ne!(original, check_edit);
    }

    #[test]
    fn registered_content_loads_exactly() {
        let mut store = InMemoryStore::new();
        let registered = case();

        let id = store.register(registered.clone()).unwrap();

        assert_eq!(store.load(&id), Ok(registered));
    }

    #[test]
    fn missing_id_is_explicitly_not_found() {
        let store = InMemoryStore::new();
        let missing = CaseId::from_digest([0; 32]);

        assert_eq!(store.load(&missing), Err(StoreError::NotFound(missing)));
    }

    #[test]
    fn approval_promotes_only_the_exact_registered_id() {
        let mut store = InMemoryStore::new();
        let approved = store.register(case()).unwrap();
        let unreviewed = store
            .register(Case::new("Unreviewed case.", ["Still loadable."]))
            .unwrap();
        let missing = CaseId::from_digest([0; 32]);

        store.approve(&approved).unwrap();

        assert_eq!(store.prod().len(), 1);
        assert!(store.prod().contains(&approved));
        assert_eq!(store.approve(&missing), Err(StoreError::NotFound(missing)));
        assert_eq!(store.prod().len(), 1);
        assert!(store.prod().contains(&approved));
        assert!(store.load(&unreviewed).is_ok());
        assert!(!store.prod().contains(&unreviewed));
    }

    #[test]
    fn modified_case_does_not_replace_approved_version() {
        let mut store = InMemoryStore::new();
        let original = case();
        let original_id = store.register(original.clone()).unwrap();
        store.approve(&original_id).unwrap();

        let modified_id = store
            .register(Case::new(
                "Explain why the modified snapshot is safe.",
                ["The regression suite passed."],
            ))
            .unwrap();

        assert_ne!(modified_id, original_id);
        assert_eq!(store.prod().len(), 1);
        assert!(store.prod().contains(&original_id));
        assert_eq!(store.load(&original_id), Ok(original));
    }

    #[test]
    fn demotion_removes_prod_membership_without_rewriting_case() {
        let mut store = InMemoryStore::new();
        let registered = case();
        let id = store.register(registered.clone()).unwrap();
        store.approve(&id).unwrap();

        store.demote(&id).unwrap();

        assert!(store.prod().is_empty());
        assert_eq!(store.load(&id), Ok(registered));
    }
}

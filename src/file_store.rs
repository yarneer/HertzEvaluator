use crate::{
    score::restore_scorecard, Case, CaseId, CasePromotion, CaseStore, CheckKind, Scorecard,
    ScorecardStore, StoreError,
};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

const MAGIC: &[u8] = b"HERTZ-FILE-1\n";
const CASE: u8 = 1;
const APPROVE: u8 = 2;
const DEMOTE: u8 = 3;
const SCORECARD: u8 = 4;

/// A durable case, promotion, and scorecard store backed by one append-only file.
#[derive(Debug)]
pub struct FileStore {
    file: File,
    cases: HashMap<CaseId, Case>,
    prod: HashSet<CaseId>,
    scorecards: HashMap<String, Scorecard>,
}

impl FileStore {
    /// Opens a store directory, creating it when it does not exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        fs::create_dir_all(path).map_err(io_error)?;
        let file_path = path.join("store.bin");
        if !file_path.exists() {
            fs::write(&file_path, MAGIC).map_err(io_error)?;
        }

        let bytes = fs::read(&file_path).map_err(io_error)?;
        let mut reader = Reader::new(&bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(invalid("missing file-store header"));
        }

        let mut store = Self {
            file: OpenOptions::new()
                .append(true)
                .open(file_path)
                .map_err(io_error)?,
            cases: HashMap::new(),
            prod: HashSet::new(),
            scorecards: HashMap::new(),
        };
        while !reader.is_empty() {
            store.replay(reader.byte()?, &mut reader)?;
        }
        Ok(store)
    }

    fn replay(&mut self, tag: u8, reader: &mut Reader<'_>) -> Result<(), StoreError> {
        match tag {
            CASE => {
                let id = reader.id()?;
                let wording = reader.string()?;
                let count = reader.count(8)?;
                let mut facts = Vec::with_capacity(count);
                for _ in 0..count {
                    facts.push(reader.string()?);
                }
                let case = Case::new(wording, facts);
                if case.id() != id {
                    return Err(invalid("stored case id does not match its content"));
                }
                match self.cases.get(&id) {
                    Some(existing) if existing != &case => {
                        return Err(StoreError::HashCollision(id));
                    }
                    Some(_) => {}
                    None => {
                        self.cases.insert(id, case);
                    }
                }
            }
            APPROVE | DEMOTE => {
                let id = reader.id()?;
                if !self.cases.contains_key(&id) {
                    return Err(invalid("promotion references an unknown case"));
                }
                if tag == APPROVE {
                    self.prod.insert(id);
                } else {
                    self.prod.remove(&id);
                }
            }
            SCORECARD => {
                let name = reader.string()?;
                let count = reader.count(40)?;
                let mut cases = Vec::with_capacity(count);
                for _ in 0..count {
                    let id = reader.id()?;
                    let mut checks = [(CheckKind::RequiredFacts, false); 4];
                    for check in &mut checks {
                        *check = (reader.kind()?, reader.boolean()?);
                    }
                    cases.push((id, checks));
                }
                self.scorecards.insert(name, restore_scorecard(cases));
            }
            _ => return Err(invalid("unknown record type")),
        }
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), StoreError> {
        self.file.write_all(bytes).map_err(io_error)
    }
}

impl CaseStore for FileStore {
    fn register(&mut self, case: Case) -> Result<CaseId, StoreError> {
        let id = case.id();
        match self.cases.get(&id) {
            Some(existing) if existing != &case => return Err(StoreError::HashCollision(id)),
            Some(_) => return Ok(id),
            None => {}
        }

        let mut record = vec![CASE];
        record.extend_from_slice(&id.digest());
        encode_string(&mut record, case.wording());
        encode_u64(&mut record, case.required_facts().len())?;
        for fact in case.required_facts() {
            encode_string(&mut record, fact);
        }
        self.append(&record)?;
        self.cases.insert(id, case);
        Ok(id)
    }

    fn load(&self, id: &CaseId) -> Result<Case, StoreError> {
        self.cases.get(id).cloned().ok_or(StoreError::NotFound(*id))
    }
}

impl CasePromotion for FileStore {
    fn approve(&mut self, id: &CaseId) -> Result<(), StoreError> {
        if !self.cases.contains_key(id) {
            return Err(StoreError::NotFound(*id));
        }
        if self.prod.contains(id) {
            return Ok(());
        }
        let mut record = vec![APPROVE];
        record.extend_from_slice(&id.digest());
        self.append(&record)?;
        self.prod.insert(*id);
        Ok(())
    }

    fn demote(&mut self, id: &CaseId) -> Result<(), StoreError> {
        if !self.cases.contains_key(id) {
            return Err(StoreError::NotFound(*id));
        }
        if !self.prod.contains(id) {
            return Ok(());
        }
        let mut record = vec![DEMOTE];
        record.extend_from_slice(&id.digest());
        self.append(&record)?;
        self.prod.remove(id);
        Ok(())
    }

    fn prod(&self) -> &HashSet<CaseId> {
        &self.prod
    }
}

impl ScorecardStore for FileStore {
    fn save_scorecard(&mut self, name: &str, scorecard: &Scorecard) -> Result<(), StoreError> {
        let mut record = vec![SCORECARD];
        encode_string(&mut record, name);
        encode_u64(&mut record, scorecard.cases().len())?;
        for case in scorecard.cases() {
            record.extend_from_slice(&case.case_id().digest());
            for check in case.checks() {
                record.push(kind_byte(check.kind()));
                record.push(u8::from(check.passed()));
            }
        }
        self.append(&record)?;
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

fn encode_string(encoded: &mut Vec<u8>, value: &str) {
    encoded.extend_from_slice(&(value.len() as u64).to_be_bytes());
    encoded.extend_from_slice(value.as_bytes());
}

fn encode_u64(encoded: &mut Vec<u8>, value: usize) -> Result<(), StoreError> {
    let value = u64::try_from(value).map_err(|_| invalid("value is too large to store"))?;
    encoded.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn kind_byte(kind: CheckKind) -> u8 {
    match kind {
        CheckKind::RequiredFacts => 0,
        CheckKind::RequiredTools => 1,
        CheckKind::Trajectory => 2,
        CheckKind::Latency => 3,
    }
}

fn io_error(error: std::io::Error) -> StoreError {
    StoreError::Io(error.to_string())
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::InvalidData(message.into())
}

struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], StoreError> {
        if count > self.remaining.len() {
            return Err(invalid("truncated record"));
        }
        let (taken, remaining) = self.remaining.split_at(count);
        self.remaining = remaining;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8, StoreError> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64, StoreError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| invalid("invalid integer"))?,
        ))
    }

    fn count(&mut self, minimum_bytes: usize) -> Result<usize, StoreError> {
        let count =
            usize::try_from(self.u64()?).map_err(|_| invalid("record count is too large"))?;
        if count > self.remaining.len() / minimum_bytes {
            return Err(invalid("record count exceeds remaining data"));
        }
        Ok(count)
    }

    fn string(&mut self) -> Result<String, StoreError> {
        let length =
            usize::try_from(self.u64()?).map_err(|_| invalid("string length is too large"))?;
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| invalid("stored string is not UTF-8"))
    }

    fn id(&mut self) -> Result<CaseId, StoreError> {
        Ok(CaseId::from_digest(
            self.take(32)?
                .try_into()
                .map_err(|_| invalid("invalid case id"))?,
        ))
    }

    fn kind(&mut self) -> Result<CheckKind, StoreError> {
        match self.byte()? {
            0 => Ok(CheckKind::RequiredFacts),
            1 => Ok(CheckKind::RequiredTools),
            2 => Ok(CheckKind::Trajectory),
            3 => Ok(CheckKind::Latency),
            _ => Err(invalid("unknown check kind")),
        }
    }

    fn boolean(&mut self) -> Result<bool, StoreError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid("invalid boolean")),
        }
    }
}

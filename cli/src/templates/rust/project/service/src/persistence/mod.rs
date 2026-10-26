use std::sync::{Arc, Mutex};

use crate::{
    domain::Record,
    error::{AppError, Result},
};

#[derive(Debug, Clone, Default)]
pub struct InMemoryRepository {
    records: Arc<Mutex<Vec<Record>>>,
}

impl InMemoryRepository {
    pub fn seed(&self, record: Record) -> Result<()> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| AppError::Storage("record store lock was poisoned".to_string()))?;
        records.push(record);
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Record>> {
        let records = self
            .records
            .lock()
            .map_err(|_| AppError::Storage("record store lock was poisoned".to_string()))?;
        Ok(records.clone())
    }

    pub fn count(&self) -> Result<usize> {
        Ok(self.list()?.len())
    }

    pub fn find(&self, id: &str) -> Result<Option<Record>> {
        Ok(self.list()?.into_iter().find(|record| record.id == id))
    }
}

use std::collections::BTreeMap;

use crate::{
    error::{LibraryError, Result},
    models::LibraryItem,
};

#[derive(Debug, Clone, Default)]
pub struct LibraryService {
    items: BTreeMap<String, LibraryItem>,
}

impl LibraryService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, item: LibraryItem) {
        self.items.insert(item.key.clone(), item);
    }

    pub fn get(&self, key: &str) -> Result<&LibraryItem> {
        self.items
            .get(key)
            .ok_or_else(|| LibraryError::MissingItem(key.to_string()))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

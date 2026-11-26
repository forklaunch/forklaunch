#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryItem {
    pub key: String,
    pub value: String,
}

impl LibraryItem {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

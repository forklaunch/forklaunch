use forklaunch_common::{json_array, json_string};
pub use crate::contracts::{Record, RecordSummary};


impl Record {
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{}\"id\":{},\"name\":{}{}",
            "{",
            json_string(&self.id),
            json_string(&self.name),
            "}"
        )
    }
}


impl RecordSummary {
    #[must_use]
    pub fn new(service: impl Into<String>, total: usize) -> Self {
        Self {
            service: service.into(),
            total,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{}\"service\":{},\"total\":{}{}",
            "{",
            json_string(&self.service),
            self.total,
            "}"
        )
    }
}

#[must_use]
pub fn records_json(records: &[Record]) -> String {
    json_array(records.iter().map(Record::to_json))
}

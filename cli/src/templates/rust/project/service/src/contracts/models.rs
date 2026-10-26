#[forklaunch_core::contract::schema]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: String,
    pub name: String,
}

#[forklaunch_core::contract::schema]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSummary {
    pub service: String,
    #[field(max = 9007199254740991)]
    pub total: usize,
}


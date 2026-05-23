use super::DocKind;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub doc_id: u64,
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
    pub kind: DocKind,
}

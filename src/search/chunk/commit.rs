use std::time::SystemTime;

use crate::git::commit::CommitInfo;
use crate::search::chunk::Chunk;

pub fn commit_to_chunk(info: &CommitInfo) -> Chunk {
    let (title, body) = split_title_body(&info.message);
    let author_time = info
        .date
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Chunk::CommitMessage {
        oid: info.id.to_string(),
        title,
        body,
        author_time,
    }
}

fn split_title_body(msg: &str) -> (String, String) {
    let mut lines = msg.splitn(2, '\n');
    let title = lines.next().unwrap_or("").trim().to_string();
    let body = lines.next().unwrap_or("").trim().to_string();
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_title_and_body() {
        let (t, b) = split_title_body("Fix bug\n\nLong description here.");
        assert_eq!(t, "Fix bug");
        assert!(b.contains("Long description"));
    }

    #[test]
    fn title_only_yields_empty_body() {
        let (t, b) = split_title_body("Single-line message");
        assert_eq!(t, "Single-line message");
        assert!(b.is_empty());
    }
}

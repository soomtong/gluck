//! In-memory LRU of finished answers keyed by the exact request, so
//! re-asking about an unchanged page/selection (e.g. after closing the
//! panel) is free. Any change to the visible range, selection, detail,
//! prompt settings or endpoint yields a different key.

use super::provider::Endpoint;
use super::ChatRequest;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

pub const DEFAULT_CAPACITY: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub struct CachedAnswer {
    pub text: String,
    pub truncated: bool,
}

pub struct AnswerCache {
    entries: HashMap<u64, CachedAnswer>,
    order: VecDeque<u64>,
    capacity: usize,
}

impl AnswerCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    /// Hash of where the request goes plus the exact JSON body sent. The
    /// API key is left out on purpose — it doesn't change the answer.
    pub fn key(endpoint: &Endpoint, request: &ChatRequest) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        endpoint.provider.hash(&mut h);
        endpoint.base_url.hash(&mut h);
        request.to_json(&endpoint.model).to_string().hash(&mut h);
        h.finish()
    }

    pub fn get(&mut self, key: u64) -> Option<CachedAnswer> {
        let hit = self.entries.get(&key).cloned()?;
        self.touch(key);
        Some(hit)
    }

    pub fn insert(&mut self, key: u64, answer: CachedAnswer) {
        if self.entries.insert(key, answer).is_some() {
            self.touch(key);
            return;
        }
        self.order.push_back(key);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn touch(&mut self, key: u64) {
        if let Some(pos) = self.order.iter().position(|&k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_back(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask::provider::KeySource;
    use crate::ask::ChatMessage;

    fn endpoint(model: &str, key: &str) -> Endpoint {
        Endpoint {
            provider: "p".into(),
            base_url: "u".into(),
            model: model.into(),
            api_key: Some(key.into()),
            key_source: KeySource::Config,
        }
    }

    fn request(text: &str, max_tokens: u32) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage::user(text.into())],
            max_tokens,
            temperature: Some(0.2),
        }
    }

    fn answer(t: &str) -> CachedAnswer {
        CachedAnswer {
            text: t.into(),
            truncated: false,
        }
    }

    #[test]
    fn test_key_depends_on_request_and_model_not_api_key() {
        let k = AnswerCache::key(&endpoint("m", "a"), &request("x", 10));
        assert_eq!(k, AnswerCache::key(&endpoint("m", "b"), &request("x", 10)));
        assert_ne!(k, AnswerCache::key(&endpoint("m2", "a"), &request("x", 10)));
        assert_ne!(k, AnswerCache::key(&endpoint("m", "a"), &request("y", 10)));
        assert_ne!(k, AnswerCache::key(&endpoint("m", "a"), &request("x", 20)));
    }

    #[test]
    fn test_lru_evicts_least_recently_used() {
        let mut c = AnswerCache::new(2);
        c.insert(1, answer("one"));
        c.insert(2, answer("two"));
        assert!(c.get(1).is_some()); // 1 is now most recent
        c.insert(3, answer("three"));
        assert_eq!(c.len(), 2);
        assert!(c.get(2).is_none());
        assert_eq!(c.get(1).unwrap().text, "one");
        assert_eq!(c.get(3).unwrap().text, "three");
    }

    #[test]
    fn test_reinsert_replaces_without_growing() {
        let mut c = AnswerCache::new(2);
        c.insert(1, answer("old"));
        c.insert(1, answer("new"));
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(1).unwrap().text, "new");
    }
}

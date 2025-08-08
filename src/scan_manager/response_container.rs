use crate::response::FeroxResponse;
use serde::{ser::SerializeSeq, Serialize, Serializer};
use std::sync::{Arc, RwLock};

/// Container around a locked vector of `FeroxResponse`s, adds wrappers for insertion and search
#[derive(Debug, Default)]
pub struct FeroxResponses {
    /// Internal structure: locked hashset of `FeroxScan`s
    pub responses: Arc<RwLock<Vec<FeroxResponse>>>,
}

/// Serialize implementation for FeroxResponses
impl Serialize for FeroxResponses {
    /// Function that handles serialization of FeroxResponses
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Ok(responses) = self.responses.read() {
            let mut seq = serializer.serialize_seq(Some(responses.len()))?;

            for response in responses.iter() {
                seq.serialize_element(response)?;
            }

            seq.end()
        } else {
            // if for some reason we can't unlock the mutex, just write an empty list
            let seq = serializer.serialize_seq(Some(0))?;
            seq.end()
        }
    }
}

/// Implementation of `FeroxResponses`
impl FeroxResponses {
    /// Add a `FeroxResponse` to the internal container
    pub fn insert(&self, response: FeroxResponse) {
        if let Ok(mut responses) = self.responses.write() {
            responses.push(response);
        }
    }

    /// Simple check for whether or not a FeroxResponse is contained within the inner container
    pub fn contains(&self, other: &FeroxResponse) -> bool {
        if let Ok(responses) = self.responses.read() {
            for response in responses.iter() {
                if response.url() == other.url() && response.method() == other.method() {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::FeroxResponse;

    fn create_response_json(
        url: &str,
        status: u16,
        word_count: usize,
        content_length: u64,
    ) -> FeroxResponse {
        let json = format!(
            r#"{{"type":"response","url":"{url}","path":"/test","wildcard":false,"status":{status},"content_length":{content_length},"line_count":10,"word_count":{word_count},"headers":{{}},"extension":""}}"#,
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    /// test that contains method works correctly  
    fn contains_method_works_correctly() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/page1", 200, 100, 1024);
        responses.insert(response1.clone());

        // Same URL and method should be contained
        assert!(responses.contains(&response1));

        // Different URL should not be contained
        let response2 = create_response_json("http://example.com/page2", 200, 100, 1024);
        assert!(!responses.contains(&response2));
    }
}

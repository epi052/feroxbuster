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

    /// check whether a FeroxResponse is unique (i.e. has a different word count and status code)
    pub fn is_unique(&self, other: &FeroxResponse) -> bool {
        if let Ok(responses) = self.responses.read() {
            for response in responses.iter() {
                if other.status().is_redirection() {
                    // if the other response is a redirect, we want to check content length
                    // instead of word count. This is to catch cases where a redirect
                    // response has the same word count within its body.
                    // e.g. "Moved Permanently - redirecting to https://example.com"
                    // which has a word count of 5 but so would
                    // "Moved Permanently - redirecting to https://example.com/about"
                    // and showing redirects to the user is something we should maintain
                    if response.content_length() == other.content_length()
                        && response.status() == other.status()
                    {
                        return false;
                    }
                } else if response.word_count() == other.word_count()
                    && response.status() == other.status()
                {
                    return false;
                }
            }
        }
        true
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
    /// test that is_unique returns true when container is empty
    fn is_unique_returns_true_when_container_empty() {
        let responses = FeroxResponses::default();
        let response = create_response_json("http://example.com/test", 200, 100, 1024);

        assert!(responses.is_unique(&response));
    }

    #[test]
    /// test that is_unique returns false when response has same word count and status code
    fn is_unique_returns_false_for_duplicate_word_count_and_status() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/page1", 200, 100, 1024);
        responses.insert(response1);

        let response2 = create_response_json("http://example.com/page2", 200, 100, 2048); // same word count and status

        assert!(!responses.is_unique(&response2));
    }

    #[test]
    /// test that is_unique returns true when response has different word count
    fn is_unique_returns_true_for_different_word_count() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/page1", 200, 100, 1024);
        responses.insert(response1);

        let response2 = create_response_json("http://example.com/page2", 200, 150, 1024); // different word count

        assert!(responses.is_unique(&response2));
    }

    #[test]
    /// test that is_unique returns true when response has different status code
    fn is_unique_returns_true_for_different_status_code() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/page1", 200, 100, 1024);
        responses.insert(response1);

        let response2 = create_response_json("http://example.com/page2", 404, 100, 1024); // different status

        assert!(responses.is_unique(&response2));
    }

    #[test]
    /// test that is_unique handles redirects based on content length instead of word count
    fn is_unique_uses_content_length_for_redirects() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/redirect1", 301, 5, 50);
        responses.insert(response1);

        let response2 = create_response_json("http://example.com/redirect2", 301, 5, 50); // same content length and status

        assert!(!responses.is_unique(&response2));
    }

    #[test]
    /// test that is_unique returns true for redirects with different content length
    fn is_unique_returns_true_for_redirects_with_different_content_length() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/redirect1", 301, 5, 50);
        responses.insert(response1);

        let response2 = create_response_json("http://example.com/redirect2", 301, 5, 75); // different content length

        assert!(responses.is_unique(&response2));
    }

    #[test]
    /// test that is_unique returns true for redirects with different status code
    fn is_unique_returns_true_for_redirects_with_different_status() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/redirect1", 301, 5, 50);
        responses.insert(response1);

        let response2 = create_response_json("http://example.com/redirect2", 302, 5, 50); // different redirect status

        assert!(responses.is_unique(&response2));
    }

    #[test]
    /// test that is_unique works correctly with multiple responses in container
    fn is_unique_works_with_multiple_responses() {
        let responses = FeroxResponses::default();

        // Insert multiple different responses
        let response1 = create_response_json("http://example.com/page1", 200, 100, 1024);
        responses.insert(response1);

        let response2 = create_response_json("http://example.com/page2", 404, 50, 512);
        responses.insert(response2);

        let response3 = create_response_json("http://example.com/page3", 403, 25, 256);
        responses.insert(response3);

        // Test a response that matches the first one
        let duplicate = create_response_json("http://example.com/different", 200, 100, 2048); // matches response1 status and word count

        assert!(!responses.is_unique(&duplicate));

        // Test a truly unique response
        let unique = create_response_json("http://example.com/unique", 500, 200, 4096);

        assert!(responses.is_unique(&unique));
    }

    #[test]
    /// test that is_unique handles edge case with redirect status codes correctly
    fn is_unique_handles_all_redirect_status_codes() {
        let responses = FeroxResponses::default();

        // Test multiple redirect status codes
        let redirect_statuses = [301, 302, 303, 307, 308];

        for (i, status) in redirect_statuses.iter().enumerate() {
            let response =
                create_response_json(&format!("http://example.com/redirect{i}"), *status, 10, 100);
            responses.insert(response);
        }

        // Test another redirect with same content length - should not be unique
        let duplicate_redirect = create_response_json("http://example.com/new", 301, 10, 100);
        assert!(!responses.is_unique(&duplicate_redirect));

        // Test redirect with different content length - should be unique
        let unique_redirect = create_response_json("http://example.com/new2", 301, 10, 200);
        assert!(responses.is_unique(&unique_redirect));
    }

    #[test]
    /// test that non-redirects are not affected by redirect logic
    fn is_unique_non_redirects_use_word_count() {
        let responses = FeroxResponses::default();

        let response1 = create_response_json("http://example.com/page1", 200, 100, 50);
        responses.insert(response1);

        // Same word count and status, but different content length (should not be unique for non-redirects)
        let response2 = create_response_json("http://example.com/page2", 200, 100, 75);

        assert!(!responses.is_unique(&response2));
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

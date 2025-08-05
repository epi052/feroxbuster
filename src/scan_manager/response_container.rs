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

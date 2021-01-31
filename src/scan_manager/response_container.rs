use crate::ferox_response::FeroxResponse;
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
        match self.responses.write() {
            Ok(mut responses) => {
                responses.push(response);
            }
            Err(e) => {
                log::error!("FeroxResponses' container's mutex is poisoned: {}", e);
            }
        }
    }

    /// Simple check for whether or not a FeroxResponse is contained within the inner container
    pub fn contains(&self, other: &FeroxResponse) -> bool {
        match self.responses.read() {
            Ok(responses) => {
                for response in responses.iter() {
                    if response.url() == other.url() {
                        return true;
                    }
                }
            }
            Err(e) => {
                log::error!("FeroxResponses' container's mutex is poisoned: {}", e);
            }
        }
        false
    }
}

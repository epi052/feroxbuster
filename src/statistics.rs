// todo needs to be serializable and added to scan save/resume/output
// todo consider batch size for stats update/display (if display is used)
use crate::FeroxResponse;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
/// todo
pub enum StatError {
    /// todo
    Four_Oh_Three,
    /// todo
    Timeout,
}

/// Data collection of statistics related to a scan
#[derive(Default, Serialize, Deserialize, Debug)]
pub struct Stats {
    /// tracker for number of timeouts seen by the client
    pub timeouts: AtomicUsize,

    /// tracker for overall number of 403s seen by the client
    four_oh_threes: AtomicUsize,

    /// tracker for overall number of 408s seen by the client
    request_timeouts: AtomicUsize,

    /// tracker for overall number of 504s seen by the client
    gateway_timeouts: AtomicUsize,
}

impl Stats {
    pub fn update(&self, response: &FeroxResponse) {
        match response.status {
            StatusCode::FORBIDDEN => {
                self.four_oh_threes.fetch_add(1, Ordering::SeqCst);
            }
            StatusCode::REQUEST_TIMEOUT => {
                self.request_timeouts.fetch_add(1, Ordering::SeqCst);
            }
            StatusCode::GATEWAY_TIMEOUT => {
                self.gateway_timeouts.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

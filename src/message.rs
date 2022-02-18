use anyhow::Context;
use console::{style, Color};
use serde::{Deserialize, Serialize};

use crate::traits::FeroxSerialize;
use crate::utils::fmt_err;

#[derive(Serialize, Deserialize, Default, Debug)]
/// Representation of a log entry, can be represented as a human readable string or JSON
pub struct FeroxMessage {
    #[serde(rename = "type")]
    /// Name of this type of struct, used for serialization, i.e. `{"type":"log"}`
    pub(crate) kind: String,

    /// The log message
    pub(crate) message: String,

    /// The log level
    pub(crate) level: String,

    /// The number of seconds elapsed since the scan started
    pub(crate) time_offset: f32,

    /// The module from which log::* was called
    pub(crate) module: String,
}

/// Implementation of FeroxMessage
impl FeroxSerialize for FeroxMessage {
    /// Create a string representation of the log message
    ///
    /// ex:  301       10l       16w      173c https://localhost/api
    fn as_str(&self) -> String {
        let (level_name, level_color) = match self.level.as_str() {
            "ERROR" => ("ERR", Color::Red),
            "WARN" => ("WRN", Color::Red),
            "INFO" => ("INF", Color::Cyan),
            "DEBUG" => ("DBG", Color::Yellow),
            "TRACE" => ("TRC", Color::Magenta),
            "WILDCARD" => ("WLD", Color::Cyan),
            _ => ("MSG", Color::White),
        };

        format!(
            "{} {:10.03} {} {}\n",
            style(level_name).bg(level_color).black(),
            style(self.time_offset).dim(),
            self.module,
            style(&self.message).dim(),
        )
    }

    /// Create an NDJSON representation of the log message
    ///
    /// (expanded for clarity)
    /// ex:
    /// {
    ///   "type": "log",
    ///   "message": "Sent https://localhost/api to file handler",
    ///   "level": "DEBUG",
    ///   "time_offset": 0.86333454,
    ///   "module": "feroxbuster::reporter"
    /// }\n
    fn as_json(&self) -> anyhow::Result<String> {
        let mut json = serde_json::to_string(&self).with_context(|| {
            fmt_err(&format!(
                "Could not convert {}:{} to JSON",
                self.level, self.message
            ))
        })?;
        json.push('\n');
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// test as_str method of FeroxMessage
    fn ferox_message_as_str_returns_string_with_newline() {
        let message = FeroxMessage {
            message: "message".to_string(),
            module: "utils".to_string(),
            time_offset: 1.0,
            level: "INFO".to_string(),
            kind: "log".to_string(),
        };
        let message_str = message.as_str();

        assert!(message_str.contains("INF"));
        assert!(message_str.contains("1.000"));
        assert!(message_str.contains("utils"));
        assert!(message_str.contains("message"));
        assert!(message_str.ends_with('\n'));
    }

    #[test]
    /// test as_json method of FeroxMessage
    fn ferox_message_as_json_returns_json_representation_of_ferox_message_with_newline() {
        let message = FeroxMessage {
            message: "message".to_string(),
            module: "utils".to_string(),
            time_offset: 1.0,
            level: "INFO".to_string(),
            kind: "log".to_string(),
        };

        let message_str = message.as_json().unwrap();

        let error_margin = f32::EPSILON;

        let json: FeroxMessage = serde_json::from_str(&message_str).unwrap();
        assert_eq!(json.module, message.module);
        assert_eq!(json.message, message.message);
        assert!((json.time_offset - message.time_offset).abs() < error_margin);
        assert_eq!(json.level, message.level);
        assert_eq!(json.kind, message.kind);
    }

    #[test]
    /// test defaults for coverage
    fn message_defaults() {
        let msg = FeroxMessage::default();
        assert_eq!(msg.level, String::new());
        assert_eq!(msg.kind, String::new());
        assert_eq!(msg.message, String::new());
        assert_eq!(msg.module, String::new());
        assert!(msg.time_offset < 0.1);
    }

    #[test]
    /// ensure WILDCARD messages serialize to WLD and anything not known to UNK
    fn message_as_str_edges() {
        let mut msg = FeroxMessage {
            message: "message".to_string(),
            module: "utils".to_string(),
            time_offset: 1.0,
            level: "WILDCARD".to_string(),
            kind: "log".to_string(),
        };
        assert!(console::strip_ansi_codes(&msg.as_str()).starts_with("WLD"));

        msg.level = "UNKNOWN".to_string();
        assert!(console::strip_ansi_codes(&msg.as_str()).starts_with("MSG"));
    }
}

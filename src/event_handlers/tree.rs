//! tree-style rendering of accumulated `FeroxResponse`s
//!
//! Used when `--tree` is set. Accumulates URLs in `RESPONSES`, then renders a
//! single tree at end-of-scan grouped first by `scheme://host[:port]` and then
//! by path segments. Each leaf line preserves the existing status/method/lines/
//! words/content-length prefix from `create_report_string`, followed by the
//! tree connectors and the URL component for that node.
use std::collections::BTreeMap;

use crate::{
    config::OutputLevel,
    response::FeroxResponse,
    scan_manager::FeroxResponses,
    utils::{create_report_string, status_colorizer},
};

/// One node in the URL trie
#[derive(Default)]
struct TreeNode {
    /// child nodes keyed by their path segment (or empty string for terminal "/")
    children: BTreeMap<String, TreeNode>,

    /// response that terminates at this node, if any (a single URL may also be
    /// an interior node for deeper URLs)
    response: Option<FeroxResponse>,
}

impl TreeNode {
    fn insert(&mut self, segments: &[String], response: FeroxResponse) {
        if segments.is_empty() {
            self.response = Some(response);
            return;
        }
        let head = &segments[0];
        let child = self.children.entry(head.clone()).or_default();
        child.insert(&segments[1..], response);
    }
}

/// Split a URL into the parts used as tree keys.
///
/// Returns (root_label, path_segments). For `http://example.com/admin/login`
/// this yields (`http://example.com`, `["admin", "login"]`). A trailing slash
/// is preserved as an empty terminal segment so the renderer can distinguish
/// `/admin` from `/admin/`.
fn split_url(response: &FeroxResponse) -> (String, Vec<String>) {
    let url = response.url();
    let scheme = url.scheme();
    let host = url.host_str().unwrap_or("");
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    let root = format!("{scheme}://{host}{port}");

    let path = url.path();
    let mut segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    if path.ends_with('/') && !segments.is_empty() {
        // distinguish /admin/ (directory) from /admin (file) so they live as
        // distinct leaves in the tree
        segments.push(String::new());
    }

    (root, segments)
}

/// Render `responses` as a tree of `String`s, one per logical line (no trailing
/// newlines). Order is deterministic via `BTreeMap`.
pub(crate) fn render(responses: &FeroxResponses, output_level: OutputLevel) -> Vec<String> {
    let mut roots: BTreeMap<String, (TreeNode, Option<FeroxResponse>)> = BTreeMap::new();

    if let Ok(guard) = responses.responses.read() {
        for resp in guard.iter() {
            let (root, segments) = split_url(resp);
            let entry = roots.entry(root).or_default();
            if segments.is_empty() {
                // bare root URL like http://example.com/
                entry.1 = Some(resp.clone());
            } else {
                entry.0.insert(&segments, resp.clone());
            }
        }
    }

    let mut lines = Vec::new();
    for (root, (node, root_resp)) in &roots {
        // root row: prefer the recorded response so stats line up; otherwise
        // emit a host-only header without a status prefix
        if let Some(resp) = root_resp {
            lines.push(format_leaf(resp, "", root, output_level));
        } else {
            lines.push(format_root_header(root, output_level));
        }
        render_children(node, "", &mut lines, output_level);
    }
    lines
}

fn render_children(
    node: &TreeNode,
    prefix: &str,
    lines: &mut Vec<String>,
    output_level: OutputLevel,
) {
    let entries: Vec<_> = node.children.iter().collect();
    let last_idx = entries.len().saturating_sub(1);

    for (idx, (segment, child)) in entries.iter().enumerate() {
        let is_last = idx == last_idx;
        let connector = if is_last { "└── " } else { "├── " };
        let display_segment = if segment.is_empty() {
            "/".to_string()
        } else {
            segment.to_string()
        };
        let label = format!("{prefix}{connector}{display_segment}");

        if let Some(resp) = child.response.as_ref() {
            lines.push(format_leaf(resp, "", &label, output_level));
        } else {
            // interior node with no recorded response (a path component that
            // showed up only because deeper URLs exist underneath it)
            lines.push(format_interior(&label, output_level));
        }

        let next_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
        render_children(child, &next_prefix, lines, output_level);
    }
}

/// Format a node that has a recorded `FeroxResponse`, reusing the standard
/// stats prefix.
fn format_leaf(
    resp: &FeroxResponse,
    leading: &str,
    url_display: &str,
    output_level: OutputLevel,
) -> String {
    let lines = resp.line_count().to_string();
    let words = resp.word_count().to_string();
    let chars = resp.content_length().to_string();
    let status = resp.status().as_str();
    let method = resp.method().as_str();

    let combined = format!("{leading}{url_display}");
    let report = create_report_string(
        status,
        method,
        &lines,
        &words,
        &chars,
        &combined,
        output_level,
    );
    // create_report_string ends with '\n'; strip for the tree renderer's per-
    // line vector
    report.trim_end_matches('\n').to_string()
}

/// Format an interior tree node that has no recorded response
fn format_interior(label: &str, output_level: OutputLevel) -> String {
    if matches!(output_level, OutputLevel::Silent) {
        // --silent: emit just the tree label, matching `create_report_string`
        // for leaves
        return label.to_string();
    }
    // Pad with spaces so interior rows align with leaf rows that carry a
    // status/method/lines/words/chars prefix. The widths below mirror
    // `create_report_string`'s default branch:
    //   "{status:>3} {method:>8} {lines:>8}l {words:>8}w {chars:>8}c {url}"
    let blank_status = status_colorizer("---");
    format!(
        "{blank_status} {:>8} {:>8}  {:>8}  {:>8}  {label}",
        "-", "-", "-", "-"
    )
}

/// Format a root header row when no recorded response exists at the root URL
fn format_root_header(root: &str, output_level: OutputLevel) -> String {
    format_interior(root, output_level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr(url: &str, status: u16) -> FeroxResponse {
        let json = format!(
            r#"{{"type":"response","url":"{url}","path":"/","wildcard":false,"status":{status},"method":"GET","content_length":100,"line_count":5,"word_count":10,"headers":{{}},"extension":""}}"#,
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn split_url_strips_root_and_keeps_trailing_slash() {
        let r = fr("http://example.com/admin/", 301);
        let (root, segs) = split_url(&r);
        assert_eq!(root, "http://example.com");
        assert_eq!(segs, vec!["admin".to_string(), String::new()]);
    }

    #[test]
    fn split_url_handles_port_and_file() {
        let r = fr("https://example.com:8443/admin/login.php", 200);
        let (root, segs) = split_url(&r);
        assert_eq!(root, "https://example.com:8443");
        assert_eq!(segs, vec!["admin".to_string(), "login.php".to_string()]);
    }

    #[test]
    fn render_produces_expected_tree_shape() {
        let responses = FeroxResponses::default();
        responses.insert(fr("http://example.com/", 200));
        responses.insert(fr("http://example.com/admin/", 301));
        responses.insert(fr("http://example.com/admin/login.php", 200));
        responses.insert(fr("http://example.com/admin/config/", 403));
        responses.insert(fr("http://example.com/admin/backup.zip", 200));

        let lines = render(&responses, OutputLevel::Silent);

        // --silent strips status/stats columns, leaving just the URL part —
        // perfect for asserting tree shape
        let expected = vec![
            "http://example.com",
            "└── admin",
            "    ├── /",
            "    ├── backup.zip",
            "    ├── config",
            "    │   └── /",
            "    └── login.php",
        ];

        assert_eq!(
            lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn render_groups_multiple_hosts() {
        let responses = FeroxResponses::default();
        responses.insert(fr("http://a.example/x", 200));
        responses.insert(fr("http://b.example/y", 200));
        let lines = render(&responses, OutputLevel::Silent);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "http://a.example");
        assert_eq!(lines[1], "└── x");
        assert_eq!(lines[2], "http://b.example");
        assert_eq!(lines[3], "└── y");
    }
}

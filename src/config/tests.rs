use super::utils::*;
use super::*;
use crate::{traits::FeroxSerialize, DEFAULT_CONFIG_NAME};
use regex::Regex;
use reqwest::Url;
use std::{collections::HashMap, fs::write};
use tempfile::TempDir;

/// creates a dummy configuration file for testing
fn setup_config_test() -> Configuration {
    let data = r#"
            wordlist = "/some/path"
            status_codes = [201, 301, 401]
            replay_codes = [201, 301]
            threads = 40
            timeout = 5
            proxy = "http://127.0.0.1:8080"
            replay_proxy = "http://127.0.0.1:8081"
            quiet = true
            silent = true
            auto_tune = true
            auto_bail = true
            verbosity = 1
            scan_limit = 6
            parallel = 14
            rate_limit = 250
            time_limit = "10m"
            output = "/some/otherpath"
            debug_log = "/yet/anotherpath"
            resume_from = "/some/state/file"
            redirects = true
            insecure = true
            collect_backups = true
            collect_extensions = true
            collect_words = true
            extensions = ["html", "php", "js"]
            dont_collect = ["png", "gif", "jpg", "jpeg"]
            methods = ["GET", "PUT", "DELETE"]
            data = [31, 32, 33, 34]
            url_denylist = ["http://dont-scan.me", "https://also-not.me"]
            regex_denylist = ["/deny.*"]
            headers = {stuff = "things", mostuff = "mothings"}
            queries = [["name","value"], ["rick", "astley"]]
            no_recursion = true
            add_slash = true
            stdin = true
            dont_filter = true
            extract_links = false
            json = true
            save_state = false
            depth = 1
            limit_bars = 3
            protocol = "http"
            request_file = "/some/request/file"
            scan_dir_listings = true
            force_recursion = true
            filter_size = [4120]
            filter_regex = ["^ignore me$"]
            filter_similar = ["https://somesite.com/soft404"]
            filter_word_count = [994, 992]
            filter_line_count = [34]
            filter_status = [201]
            server_certs = ["/some/cert.pem", "/some/other/cert.pem"]
            client_cert = "/some/client/cert.pem"
            client_key = "/some/client/key.pem"
            backup_extensions = [".save"]
        "#;
    let tmp_dir = TempDir::new().unwrap();
    let file = tmp_dir.path().join(DEFAULT_CONFIG_NAME);
    write(&file, data).unwrap();
    Configuration::parse_config(file).unwrap()
}

#[test]
/// test that all default config values meet expectations
fn default_configuration() {
    let config = Configuration::default();
    assert_eq!(config.wordlist, wordlist());
    assert_eq!(config.proxy, String::new());
    assert_eq!(config.target_url, String::new());
    assert_eq!(config.time_limit, String::new());
    assert_eq!(config.resume_from, String::new());
    assert_eq!(config.debug_log, String::new());
    assert_eq!(config.config, String::new());
    assert_eq!(config.replay_proxy, String::new());
    assert_eq!(config.status_codes, status_codes());
    assert_eq!(config.replay_codes, config.status_codes);
    assert!(config.replay_client.is_none());
    assert_eq!(config.threads, threads());
    assert_eq!(config.depth, depth());
    assert_eq!(config.timeout, timeout());
    assert_eq!(config.verbosity, 0);
    assert_eq!(config.scan_limit, 0);
    assert_eq!(config.limit_bars, 0);
    assert!(!config.silent);
    assert!(!config.quiet);
    assert_eq!(config.output_level, OutputLevel::Default);
    assert!(!config.dont_filter);
    assert!(!config.auto_tune);
    assert!(!config.auto_bail);
    assert_eq!(config.requester_policy, RequesterPolicy::Default);
    assert!(!config.no_recursion);
    assert!(!config.random_agent);
    assert!(!config.json);
    assert!(config.save_state);
    assert!(!config.stdin);
    assert!(!config.add_slash);
    assert!(!config.force_recursion);
    assert!(!config.redirects);
    assert!(config.extract_links);
    assert!(!config.insecure);
    assert!(!config.collect_extensions);
    assert!(!config.collect_backups);
    assert!(!config.collect_words);
    assert!(!config.scan_dir_listings);
    assert!(config.regex_denylist.is_empty());
    assert_eq!(config.queries, Vec::new());
    assert_eq!(config.filter_size, Vec::<u64>::new());
    assert_eq!(config.extensions, Vec::<String>::new());
    assert_eq!(config.methods, vec!["GET"]);
    assert_eq!(config.data, Vec::<u8>::new());
    assert_eq!(config.url_denylist, Vec::<Url>::new());
    assert_eq!(config.dont_collect, ignored_extensions());
    assert_eq!(config.filter_regex, Vec::<String>::new());
    assert_eq!(config.filter_similar, Vec::<String>::new());
    assert_eq!(config.filter_word_count, Vec::<usize>::new());
    assert_eq!(config.filter_line_count, Vec::<usize>::new());
    assert_eq!(config.filter_status, Vec::<u16>::new());
    assert_eq!(config.headers, HashMap::new());
    assert_eq!(config.server_certs, Vec::<String>::new());
    assert_eq!(config.client_cert, String::new());
    assert_eq!(config.client_key, String::new());
    assert_eq!(config.backup_extensions, backup_extensions());
    assert_eq!(config.protocol, request_protocol());
    assert_eq!(config.request_file, String::new());
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_wordlist() {
    let config = setup_config_test();
    assert_eq!(config.wordlist, "/some/path");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_debug_log() {
    let config = setup_config_test();
    assert_eq!(config.debug_log, "/yet/anotherpath");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_status_codes() {
    let config = setup_config_test();
    assert_eq!(config.status_codes, vec![201, 301, 401]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_replay_codes() {
    let config = setup_config_test();
    assert_eq!(config.replay_codes, vec![201, 301]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_threads() {
    let config = setup_config_test();
    assert_eq!(config.threads, 40);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_depth() {
    let config = setup_config_test();
    assert_eq!(config.depth, 1);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_scan_limit() {
    let config = setup_config_test();
    assert_eq!(config.scan_limit, 6);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_parallel() {
    let config = setup_config_test();
    assert_eq!(config.parallel, 14);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_rate_limit() {
    let config = setup_config_test();
    assert_eq!(config.rate_limit, 250);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_timeout() {
    let config = setup_config_test();
    assert_eq!(config.timeout, 5);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_proxy() {
    let config = setup_config_test();
    assert_eq!(config.proxy, "http://127.0.0.1:8080");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_replay_proxy() {
    let config = setup_config_test();
    assert_eq!(config.replay_proxy, "http://127.0.0.1:8081");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_silent() {
    let config = setup_config_test();
    assert!(config.silent);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_force_recursion() {
    let config = setup_config_test();
    assert!(config.force_recursion);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_quiet() {
    let config = setup_config_test();
    assert!(config.quiet);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_json() {
    let config = setup_config_test();
    assert!(config.json);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_auto_bail() {
    let config = setup_config_test();
    assert!(config.auto_bail);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_auto_tune() {
    let config = setup_config_test();
    assert!(config.auto_tune);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_verbosity() {
    let config = setup_config_test();
    assert_eq!(config.verbosity, 1);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_limit_bars() {
    let config = setup_config_test();
    assert_eq!(config.limit_bars, 3);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_output() {
    let config = setup_config_test();
    assert_eq!(config.output, "/some/otherpath");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_redirects() {
    let config = setup_config_test();
    assert!(config.redirects);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_insecure() {
    let config = setup_config_test();
    assert!(config.insecure);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_no_recursion() {
    let config = setup_config_test();
    assert!(config.no_recursion);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_stdin() {
    let config = setup_config_test();
    assert!(config.stdin);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_dont_filter() {
    let config = setup_config_test();
    assert!(config.dont_filter);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_add_slash() {
    let config = setup_config_test();
    assert!(config.add_slash);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_extract_links() {
    let config = setup_config_test();
    assert!(!config.extract_links);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_collect_extensions() {
    let config = setup_config_test();
    assert!(config.collect_extensions);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_collect_backups() {
    let config = setup_config_test();
    assert!(config.collect_backups);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_collect_words() {
    let config = setup_config_test();
    assert!(config.collect_words);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_extensions() {
    let config = setup_config_test();
    assert_eq!(config.extensions, vec!["html", "php", "js"]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_dont_collect() {
    let config = setup_config_test();
    assert_eq!(config.dont_collect, vec!["png", "gif", "jpg", "jpeg"]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_methods() {
    let config = setup_config_test();
    assert_eq!(config.methods, vec!["GET", "PUT", "DELETE"]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_data() {
    let config = setup_config_test();
    assert_eq!(config.data, vec![31, 32, 33, 34]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_regex_denylist() {
    let config = setup_config_test();
    assert_eq!(
        config.regex_denylist[0].as_str(),
        Regex::new("/deny.*").unwrap().as_str()
    );
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_url_denylist() {
    let config = setup_config_test();
    assert_eq!(
        config.url_denylist,
        vec![
            Url::parse("http://dont-scan.me").unwrap(),
            Url::parse("https://also-not.me").unwrap(),
        ]
    );
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_filter_regex() {
    let config = setup_config_test();
    assert_eq!(config.filter_regex, vec!["^ignore me$"]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_filter_similar() {
    let config = setup_config_test();
    assert_eq!(config.filter_similar, vec!["https://somesite.com/soft404"]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_filter_size() {
    let config = setup_config_test();
    assert_eq!(config.filter_size, vec![4120]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_filter_word_count() {
    let config = setup_config_test();
    assert_eq!(config.filter_word_count, vec![994, 992]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_filter_line_count() {
    let config = setup_config_test();
    assert_eq!(config.filter_line_count, vec![34]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_filter_status() {
    let config = setup_config_test();
    assert_eq!(config.filter_status, vec![201]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_save_state() {
    let config = setup_config_test();
    assert!(!config.save_state);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_time_limit() {
    let config = setup_config_test();
    assert_eq!(config.time_limit, "10m");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_scan_dir_listings() {
    let config = setup_config_test();
    assert!(config.scan_dir_listings);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_protocol() {
    let config = setup_config_test();
    assert_eq!(config.protocol, "http");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_request_file() {
    let config = setup_config_test();
    assert_eq!(config.request_file, String::new());
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_resume_from() {
    let config = setup_config_test();
    assert_eq!(config.resume_from, "/some/state/file");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_server_certs() {
    let config = setup_config_test();
    assert_eq!(
        config.server_certs,
        ["/some/cert.pem", "/some/other/cert.pem"]
    );
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_backup_extensions() {
    let config = setup_config_test();
    assert_eq!(config.backup_extensions, [".save"]);
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_client_cert() {
    let config = setup_config_test();
    assert_eq!(config.client_cert, "/some/client/cert.pem");
}

#[test]
/// parse the test config and see that the value parsed is correct
fn config_reads_client_key() {
    let config = setup_config_test();
    assert_eq!(config.client_key, "/some/client/key.pem");
}

#[test]
/// parse the test config and see that the values parsed are correct
fn config_reads_headers() {
    let config = setup_config_test();
    let mut headers = HashMap::new();
    headers.insert("stuff".to_string(), "things".to_string());
    headers.insert("mostuff".to_string(), "mothings".to_string());
    assert_eq!(config.headers, headers);
}

#[test]
/// parse the test config and see that the values parsed are correct
fn config_reads_queries() {
    let config = setup_config_test();
    let queries = vec![
        ("name".to_string(), "value".to_string()),
        ("rick".to_string(), "astley".to_string()),
    ];
    assert_eq!(config.queries, queries);
}

#[test]
fn config_default_not_random_agent() {
    let config = setup_config_test();
    assert!(!config.random_agent);
}

#[test]
#[should_panic]
/// test that an error message is printed and panic is called when report_and_exit is called
fn config_report_and_exit_works() {
    report_and_exit("some message");
}

#[test]
/// test as_str method of Configuration
fn as_str_returns_string_with_newline() {
    let config = Configuration::new().unwrap();
    let config_str = config.as_str();
    println!("{config_str}");
    assert!(config_str.starts_with("Configuration {"));
    assert!(config_str.ends_with("}\n"));
    assert!(config_str.contains("replay_codes:"));
    assert!(config_str.contains("client: Client {"));
    assert!(config_str.contains("user_agent: \"feroxbuster"));
}

#[test]
/// test as_json method of Configuration
fn as_json_returns_json_representation_of_configuration_with_newline() {
    let mut config = Configuration::new().unwrap();
    config.timeout = 12;
    config.depth = 2;
    let config_str = config.as_json().unwrap();
    let json: Configuration = serde_json::from_str(&config_str).unwrap();
    assert_eq!(json.config, config.config);
    assert_eq!(json.wordlist, config.wordlist);
    assert_eq!(json.replay_codes, config.replay_codes);
    assert_eq!(json.timeout, config.timeout);
    assert_eq!(json.depth, config.depth);
}

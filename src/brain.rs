use uuid::Uuid;
use crate::scanner::{make_request, format_url};
use crate::config::CONFIGURATION;


/// todo document
pub async fn initialize(target_urls: &[String]) {
    for target_url in target_urls {
        let nonexistent = format_url(target_url, &unique_string(), None);
        let response = make_request(&CONFIGURATION.client, nonexistent.unwrap()).await.unwrap();
        println!("{:?}", response);
        if CONFIGURATION.statuscodes.contains(&response.status().as_u16()) {
            println!("found wildcard response");
        }
    }
}

/// Simple helper to return a uuid, formatted as lowercase without hyphens
fn unique_string() -> String {
    Uuid::new_v4().to_simple().to_string()
}

/// todo document
pub async fn smart_scan(target_urls: &[String]) {
    println!("{:?}", target_urls);


}


use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::{
    config::OutputLevel,
    event_handlers::Handles,
    scan_manager::{FeroxScans, ScanOrder},
};

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[should_panic]
/// try to hit struct field coverage of FileOutHandler
async fn get_scan_by_url_bails_on_unfound_url() {
    let sem = Semaphore::new(10);
    let urls = FeroxScans::new(OutputLevel::Default, 0);

    let scanner = FeroxScanner::new(
        "http://localhost",
        ScanOrder::Initial,
        Arc::new(Default::default()),
        Arc::new(sem),
        Arc::new(Handles::for_testing(Some(Arc::new(urls)), None).0),
    );
    scanner.scan_url().await.unwrap();
}

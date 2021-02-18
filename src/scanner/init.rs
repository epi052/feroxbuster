use crate::{
    event_handlers::{Command::AddToUsizeField, Handles},
    statistics::StatField::ExpectedPerScan,
};
use anyhow::Result;
use std::{convert::TryInto, sync::Arc};

/// Perform steps necessary to run scans that only need to be performed once (warming up the
/// engine, as it were)
pub async fn initialize(num_words: usize, handles: Arc<Handles>) -> Result<()> {
    log::trace!("enter: initialize({}, {:?})", num_words, handles);

    // number of requests only needs to be calculated once, and then can be reused
    let num_reqs_expected: u64 = if handles.config.extensions.is_empty() {
        num_words.try_into()?
    } else {
        let total = num_words * (handles.config.extensions.len() + 1);
        total.try_into()?
    };

    {
        // no real reason to keep the arc around beyond this call
        let scans = handles.ferox_scans()?;
        scans.set_bar_length(num_reqs_expected);
    }

    // tell Stats object about the number of expected requests
    handles
        .stats
        .send(AddToUsizeField(ExpectedPerScan, num_reqs_expected as usize))?;

    log::trace!("exit: initialize");
    Ok(())
}

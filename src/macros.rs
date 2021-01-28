#![macro_use]

#[macro_export]
/// wrapper to improve code readability
macro_rules! send_command {
    ($tx:expr, $value:expr) => {
        $tx.send($value).unwrap_or_default();
    };
}

#[macro_export]
macro_rules! skip_fail {
    ($res:expr) => {
        match $res {
            Ok(val) => val,
            Err(e) => {
                log::warn!("An error: {}; skipped.", e);
                continue;
            }
        }
    };
}

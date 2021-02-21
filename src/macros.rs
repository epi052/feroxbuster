#![macro_use]

#[macro_export]
/// wrapper to improve code readability
macro_rules! send_command {
    ($tx:expr, $value:expr) => {
        $tx.send($value).unwrap_or_default();
    };
}

#[macro_export]
/// while looping, check for a Result, if Ok return the value, if Err, continue
macro_rules! skip_fail {
    ($res:expr) => {
        match $res {
            Ok(val) => val,
            Err(e) => {
                log::warn!("{}", fmt_err(&format!("{}; skipping...", e)));
                continue;
            }
        }
    };
}

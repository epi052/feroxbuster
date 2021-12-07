#![macro_use]

/// Wrapper `Atomic*.fetch_add` to save me from writing Ordering::Relaxed a bajillion times
///
/// default is to increment by 1, second arg can be used to increment by a different value
#[macro_export]
macro_rules! atomic_increment {
    ($metric:expr) => {
        $metric.fetch_add(1, Ordering::Relaxed);
    };

    ($metric:expr, $value:expr) => {
        $metric.fetch_add($value, Ordering::Relaxed);
    };
}

/// Wrapper around `Atomic*.load` to save me from writing Ordering::Relaxed a bajillion times
#[macro_export]
macro_rules! atomic_load {
    ($metric:expr) => {
        $metric.load(Ordering::Relaxed)
    };
    ($metric:expr, $ordering:expr) => {
        $metric.load($ordering)
    };
}

/// Wrapper around `Atomic*.store` to save me from writing Ordering::Relaxed a bajillion times
#[macro_export]
macro_rules! atomic_store {
    ($metric:expr, $value:expr) => {
        $metric.store($value, Ordering::Relaxed);
    };
    ($metric:expr, $value:expr, $ordering:expr) => {
        $metric.store($value, $ordering);
    };
}

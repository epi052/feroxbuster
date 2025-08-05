//! Synchronization primitives for feroxbuster
//!
//! This module provides enhanced synchronization primitives that extend
//! the functionality of standard async synchronization tools to meet
//! feroxbuster's specific needs.

mod dynamic_semaphore;

pub use dynamic_semaphore::{DynamicSemaphore, DynamicSemaphorePermit};

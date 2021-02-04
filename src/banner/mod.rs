//! all logic related to building/printing the banner seen when scans start
mod container;
mod entry;

#[cfg(test)]
mod tests;

pub use self::container::{Banner, UPDATE_URL};

mod backend;
mod config;
mod download;
mod jobs;
mod pdf;
mod server;
#[cfg(windows)]
mod windows;

pub use config::LabelSettings;
pub use server::{serve, Pairing, PAIRING_PORT};

#[cfg(test)]
mod tests;

// src/api/mod.rs
pub mod auth;
pub mod uploads;
mod smart; // Private mod

// Re-export 'analyze' agar terlihat seolah-olah ada di bawah 'api'
pub use smart::normal_analyze;
pub use smart::deep_analyze;
pub use smart::fast_analyze;
//! Wizard Pages - Individual pages for the installation wizard
//!
//! Each page is a self-contained widget that handles its own logic
//! and emits signals for navigation.

mod welcome;
mod progress;
mod complete;
mod maintenance;
mod license;

pub use welcome::WelcomePage;
pub use progress::ProgressPage;
pub use complete::CompletePage;
pub use maintenance::MaintenancePage;
pub use license::LicensePage;


#[allow(clippy::module_inception)]
mod network_manager;

pub use network_manager::NetworkManagerProxy;
pub mod access_point;
pub mod device;
pub mod settings;
pub mod statistics;
pub mod wireless;

use anyhow::Result;
pub use config::{Endpoint, ProxyConfig};

pub mod config;

mod platform;

pub fn apply(config: &ProxyConfig) -> Result<()> {
  platform::apply(config)
}

pub fn clear() -> Result<()> {
  platform::clear()
}

pub fn show() -> Result<ProxyConfig> {
  platform::show()
}

//! The Opencord server: one community per instance.

pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod gateway;
pub mod guild;
pub mod handlers;
pub mod http;
pub mod media;
pub mod permissions;
pub mod presence;
pub mod random;
pub mod rate_limit;
pub mod server;
pub mod state;
pub mod tls;
pub mod visibility;
pub mod voice;

//! The functions JavaScript calls (design §5.2). Call `core_init` first,
//! then `events_attach` and `identity_load`.

pub mod chat;
pub mod files;
pub mod identity;
pub mod music;
pub mod servers;
pub mod setup;
pub mod soundboard;
pub mod voice;
pub mod voice_messages;

//! Protobuf types for the Opencord gateway protocol, generated from `proto/`.

pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/opencord.v1.rs"));
}

//! The wire: packet framing, compression, encryption and the readers and
//! writers of its types, and the text components that travel on it.

pub mod compression;
pub mod connection;
pub mod crypto;
pub mod id;
pub mod item;
pub mod packet;
pub mod reader;
pub mod varint;

pub mod component;

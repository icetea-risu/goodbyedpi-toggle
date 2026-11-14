//! Platform-independent logic. Everything here builds and is unit-tested on
//! Linux; Windows-only code lives in `crate::win`.

pub mod icon_art;
pub mod known;
pub mod pe_version;
pub mod upstream_script;

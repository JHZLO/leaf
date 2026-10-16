//! The rules behind leaf: the folder tree, keyboard moves, copied paths and file I/O.
//! Nothing here depends on GPUI, so these rules test in seconds.

pub mod fs;
pub mod fuzzy;
pub mod index;
pub mod markdown;
pub mod nav;
pub mod paths;
pub mod search;
pub mod session;
pub mod settings;
pub mod tree;
pub mod watch;

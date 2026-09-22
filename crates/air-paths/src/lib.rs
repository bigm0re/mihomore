extern crate self as air_paths;

pub mod roots;

pub use roots::{
    AppDirRoots, PORTABLE_CACHE_DIR, PORTABLE_CONFIG_DIR, PORTABLE_DATA_DIR, PathMode,
};

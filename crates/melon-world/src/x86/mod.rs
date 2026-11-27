//! The x86_64 `target_feature` bodies of the world's kernels, one file per
//! kernel, beside whose scalar code they run (`section_decode.rs`).

pub mod nibble_decode;
pub mod section_decode;

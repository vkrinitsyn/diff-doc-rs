//  original is https://github.com/ChrisRega/json-diff  https://github.com/ChrisRega/json-diff/blob/master/UNLICENSE


pub mod enums;
pub mod mismatch;
pub mod process;
pub mod sort;

pub type Result<T> = std::result::Result<T, enums::Error>;

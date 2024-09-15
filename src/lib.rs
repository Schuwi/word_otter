#[cfg(all(
    any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
    not(feature = "dashu")
))]
#[path = "bigint_rug.rs"]
mod bigint;
#[cfg(not(all(
    any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
    not(feature = "dashu")
)))]
#[path = "bigint_dashu.rs"]
mod bigint;

mod implementation;

pub use implementation::*;
pub use bigint::*;

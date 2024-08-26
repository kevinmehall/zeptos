#[cfg(feature="samd11")]
pub use atsamd11d as pac;

#[cfg(feature="samd21")]
pub use atsamd21j as pac;

pub mod gpio;


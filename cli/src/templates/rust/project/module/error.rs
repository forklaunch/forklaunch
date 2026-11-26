//! Startup errors. Request errors are rendered by the module's handlers in
//! their declared wire formats and never pass through this type.
use std::{error, result};

pub type Error = Box<dyn error::Error>;
pub type Result<T> = result::Result<T, Error>;

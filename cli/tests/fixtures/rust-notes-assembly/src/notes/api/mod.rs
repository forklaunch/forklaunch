//! API controllers and route registration.
mod context;
mod controllers;
mod errors;
pub(crate) mod routes;

#[cfg(test)]
#[path = "../__test__/api.rs"]
mod tests;

//! Database admission and restricted runtime pools. Payload encryption and RLS
//! remain independent safeguards; a TLS connection alone supplies neither.
use sqlx as driver;
include!("postgres_pool.rs");

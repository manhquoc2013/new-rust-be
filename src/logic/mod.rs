//! Business logic: handler receives messages from connections and processes (checkin, commit, rollback).

pub mod handler;

pub use handler::*;

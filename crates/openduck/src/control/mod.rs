//! Personal-agent control-plane primitives.
//!
//! This module contains storage and validation for the HTTP control plane.
//! Scheduler code must not import this module.

pub mod files;
pub mod git;
pub mod harness;
pub mod http;
pub mod insight;
pub mod jobs;
pub mod project_harness;
pub mod project_roots;
pub mod projects;
pub mod task_scheduler;
pub mod terminal;

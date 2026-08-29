//! Shared benchmark harness: the workload contract every ORM adapter
//! implements, the measurement loop, statistics, and report rendering.
//!
//! The harness owns everything that is not ORM-specific - fixtures, timing,
//! percentiles, process metrics - so that the only difference between two
//! numbers in a report is the ORM code that produced them.

pub mod adapter;
pub mod child;
pub mod fixture;
pub mod proc;
pub mod report;
pub mod scenario;
pub mod stats;
pub mod workload;

pub use adapter::{BenchError, BenchResult, NewPost, OrmAdapter, PostRow, UserRow};
pub use scenario::{Scenario, ScenarioConfig};
pub use stats::Stats;
pub use workload::{run_all, RunConfig};

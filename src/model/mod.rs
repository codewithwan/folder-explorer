//! Layer 2: the in-memory data model and the queries over it. No UI and no OS
//! calls beyond reading the filesystem during a scan.

pub mod chart;
pub mod item;
pub mod list;
pub mod query;
pub mod scan;

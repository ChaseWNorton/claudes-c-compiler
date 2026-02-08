//! Superoptimizer for the CCC i686 backend.
//!
//! Exhaustively searches for the shortest instruction sequence equivalent to
//! patterns harvested from real compiler output. Generates rewrite rules that
//! the peephole optimizer can apply at compile time.

pub mod ir;
pub mod cpu;
pub mod emulator;
pub mod sizing;
pub mod verify;
pub mod search;
pub mod table;
pub mod harvester;

//! The MyBarPool fuzz harness (build plan Step 8).
//!
//! Seeds are Mollusk fixtures ejected from the unit suite
//! (`EJECT_FUZZ_FIXTURES=$PWD/target/fuzz-fixtures cargo test -p mybarpool --features fuzz-fixtures`),
//! one per `process_instruction` call, so the fuzzer starts from every valid state the suite
//! already knows. This library is the pure part: [`model`] is a plain picture of an
//! instruction's inputs and effects, [`seeds`] names and groups instructions by their Anchor
//! discriminator, [`mutate`] is the proptest strategy over a seed's context, and [`oracle`]
//! is the PROGRAM §10 Money check every mutated run must satisfy. The binary (`src/main.rs`,
//! feature `runner`) loads the fixtures, replays them through Mollusk and drives the three.

pub mod model;
pub mod mutate;
pub mod oracle;
pub mod seeds;

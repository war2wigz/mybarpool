//! The PROGRAM §10 Money oracle; the checks land with the fuzzer.

use crate::model::{Context, Outcome};

/// A violated oracle line, for the failure report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub line: &'static str,
    pub detail: String,
}

/// Placeholder until the checks land: nothing is ever wrong.
pub fn check(_before: &Context, _after: &Outcome) -> Vec<Violation> {
    Vec::new()
}

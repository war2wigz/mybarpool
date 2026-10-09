//! Mutation strategies over a seed's [`Context`]. The kinds are named here; the strategies
//! land with the fuzzer.

use crate::model::Context;

/// The kinds of mutation the harness applies; each case picks one seed and one to three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Data,
    Signer,
    Alias,
    Lamports,
    DataPlant,
    Clock,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Data,
        Kind::Signer,
        Kind::Alias,
        Kind::Lamports,
        Kind::DataPlant,
        Kind::Clock,
    ];
}

/// A mutation applied to a context, for the failure report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub kind: Kind,
    pub detail: String,
}

/// Placeholder until the strategies land: the identity.
pub fn identity(ctx: &Context) -> (Context, Vec<Applied>) {
    (ctx.clone(), Vec::new())
}

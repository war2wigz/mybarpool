//! Mutation strategies over a seed's [`Context`] (build plan Step 8 › Fuzzer).
//!
//! A case is a seed and a *plan* of one to three [`Mutation`]s. The plan is plain data drawn by
//! proptest from the seed's [`Shape`] (how many metas and accounts it has, how long its data
//! is, whether it is a `buy`), so proptest can shrink it; [`apply`] turns a plan into a mutated
//! context and the list of what was applied, for the failure report. Indices are taken modulo
//! the shape so every plan is applicable.
//!
//! `missing_mut_constraint` (anchor-lints `d8116dd`) reads every access to a field of an Anchor
//! `#[account]` struct as a write to an account of the instruction being analysed; this
//! crate has no instructions and no accounts, it decodes the program's state to judge it, so
//! the lint is allowed for the module (listed in `AUDIT-READINESS.md`).
#![cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]

use anchor_lang::{AccountDeserialize, Discriminator};
use proptest::prelude::*;

use crate::model::{Account, Context, Key};

/// The kinds of mutation the harness applies; each case picks one seed and one to three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    pub fn name(self) -> &'static str {
        match self {
            Kind::Data => "data",
            Kind::Signer => "signer",
            Kind::Alias => "alias",
            Kind::Lamports => "lamports",
            Kind::DataPlant => "data-plant",
            Kind::Clock => "clock",
        }
    }
}

/// Sixty days in seconds, the `clock` mutation's range either way.
pub const CLOCK_RANGE: i64 = 60 * 86_400;

/// Which program-owned account a `data-plant` targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Pool,
    Game,
    Config,
}

/// One mutation, as plain data proptest can shrink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mutation {
    /// `data`: flip a byte after the discriminator.
    FlipByte { offset: usize, xor: u8 },
    /// `data`: truncate to `len` bytes (modulo the length).
    Truncate { len: usize },
    /// `data`: append 1–64 random bytes.
    Append(Vec<u8>),
    /// `data`: replace the `count` byte (the first after the discriminator) with 0, 26 or 255.
    Count(u8),
    /// `data` (`buy` only): replace the proof with a random-length vector of random entries.
    Proof(Vec<[u8; 32]>),
    /// `signer`: clear `is_signer` on the n-th meta that has it.
    ClearSigner(usize),
    /// `signer`: set `is_signer` on the n-th meta that lacks it.
    SetSigner(usize),
    /// `alias`: replace meta `from`'s key with meta `to`'s key (the duplicate-account attack).
    Alias { from: usize, to: usize },
    /// `alias`: replace one meta's key with a fresh key backed by an empty system account.
    Fresh { meta: usize, key: Key },
    /// `lamports`: set a non-vault account's lamports to `value` (below `u64::MAX / 2`).
    Lamports { account: usize, value: u64 },
    /// `lamports`: set the vault's lamports to `permille / 1000 × 2 ×` its recorded value.
    VaultLamports { permille: u16 },
    /// `data-plant`: write `byte` at `offset` of the target's data.
    Plant {
        target: Target,
        offset: usize,
        byte: u8,
    },
    /// `clock`: shift the `Clock` sysvar's `unix_timestamp` by `delta` seconds.
    Clock { delta: i64 },
}

impl Mutation {
    pub fn kind(&self) -> Kind {
        match self {
            Mutation::FlipByte { .. }
            | Mutation::Truncate { .. }
            | Mutation::Append(_)
            | Mutation::Count(_)
            | Mutation::Proof(_) => Kind::Data,
            Mutation::ClearSigner(_) | Mutation::SetSigner(_) => Kind::Signer,
            Mutation::Alias { .. } | Mutation::Fresh { .. } => Kind::Alias,
            Mutation::Lamports { .. } | Mutation::VaultLamports { .. } => Kind::Lamports,
            Mutation::Plant { .. } => Kind::DataPlant,
            Mutation::Clock { .. } => Kind::Clock,
        }
    }
}

/// What a plan needs to know about its seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub metas: usize,
    pub accounts: usize,
    pub data_len: usize,
    pub is_buy: bool,
}

impl Shape {
    pub fn of(ctx: &Context) -> Self {
        Self {
            metas: ctx.metas.len(),
            accounts: ctx.accounts.len(),
            data_len: ctx.data.len(),
            is_buy: ctx.data.len() >= 8
                && ctx.data[..8] == *<mybarpool::instruction::Buy as Discriminator>::DISCRIMINATOR,
        }
    }
}

/// One mutation drawn for `shape`.
pub fn mutation(shape: Shape) -> BoxedStrategy<Mutation> {
    let metas = shape.metas.max(1);
    let accounts = shape.accounts.max(1);
    let payload = shape.data_len.saturating_sub(8).max(1);
    let data = prop_oneof![
        (0..payload, any::<u8>()).prop_map(|(offset, xor)| Mutation::FlipByte { offset, xor }),
        (0..shape.data_len.max(1)).prop_map(|len| Mutation::Truncate { len }),
        prop::collection::vec(any::<u8>(), 1..=64).prop_map(Mutation::Append),
        prop::sample::select(vec![0u8, 26, 255]).prop_map(Mutation::Count),
    ];
    let data: BoxedStrategy<Mutation> = if shape.is_buy {
        prop_oneof![
            3 => data,
            1 => prop::collection::vec(any::<[u8; 32]>(), 0..=40).prop_map(Mutation::Proof),
        ]
        .boxed()
    } else {
        data.boxed()
    };
    prop_oneof![
        3 => data,
        2 => prop_oneof![
            (0..metas).prop_map(Mutation::ClearSigner),
            (0..metas).prop_map(Mutation::SetSigner),
        ],
        2 => prop_oneof![
            (0..metas, 0..metas).prop_map(|(from, to)| Mutation::Alias { from, to }),
            (0..metas, any::<[u8; 32]>()).prop_map(|(meta, key)| Mutation::Fresh { meta, key }),
        ],
        2 => prop_oneof![
            (0..accounts, 0..u64::MAX / 2)
                .prop_map(|(account, value)| Mutation::Lamports { account, value }),
            (0..=2000u16).prop_map(|permille| Mutation::VaultLamports { permille }),
        ],
        2 => (
            prop::sample::select(vec![Target::Pool, Target::Game, Target::Config]),
            0..2048usize,
            any::<u8>()
        )
            .prop_map(|(target, offset, byte)| Mutation::Plant { target, offset, byte }),
        1 => (-CLOCK_RANGE..=CLOCK_RANGE).prop_map(|delta| Mutation::Clock { delta }),
    ]
    .boxed()
}

/// A plan of one to three mutations for `shape`.
pub fn plan(shape: Shape) -> BoxedStrategy<Vec<Mutation>> {
    prop::collection::vec(mutation(shape), 1..=3).boxed()
}

/// A mutation applied to a context, for the failure report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub kind: Kind,
    pub detail: String,
}

/// The system program's id (the owner of a fresh empty account).
pub const SYSTEM_PROGRAM: Key = [0u8; 32];

/// The program-owned account of `target` in the context, by Anchor discriminator.
pub fn find_target(ctx: &Context, target: Target) -> Option<usize> {
    let disc: &[u8] = match target {
        Target::Pool => mybarpool::Pool::DISCRIMINATOR,
        Target::Game => mybarpool::GameRecord::DISCRIMINATOR,
        Target::Config => mybarpool::PlatformConfig::DISCRIMINATOR,
    };
    ctx.accounts.iter().position(|a| {
        a.owner == mybarpool::ID.to_bytes() && a.data.len() >= 8 && a.data[..8] == *disc
    })
}

/// The pool's vault, from the pool account in the context.
pub fn find_vault(ctx: &Context) -> Option<usize> {
    let pool = find_target(ctx, Target::Pool)?;
    let decoded = mybarpool::Pool::try_deserialize(&mut &ctx.accounts[pool].data[..]).ok()?;
    let vault = decoded.vault.to_bytes();
    ctx.accounts.iter().position(|a| a.key == vault)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn on_curve(key: &Key) -> bool {
    anchor_lang::prelude::Pubkey::new_from_array(*key).is_on_curve()
}

/// The nearest key on the ed25519 curve at or after `key` (last byte incremented, at most
/// 255 steps: about half of all keys are on the curve), so a fresh signer can exist.
fn nudge_on_curve(mut key: Key) -> Key {
    for _ in 0..=255 {
        if on_curve(&key) {
            return key;
        }
        key[31] = key[31].wrapping_add(1);
    }
    key
}

/// Why a mutated context is not a transaction the runtime could ever see: Mollusk trusts the
/// `is_signer` flags (signature verification is the transaction layer's), so a signer meta
/// whose key is off the ed25519 curve — a PDA, or about half of all random keys — would make
/// the SVM answer a question no transaction can ask. Only signers the *mutation* introduced
/// count: the unit suite's own signers are `Pubkey::new_unique()` keys, off the curve about
/// half the time, and Mollusk accepted them as the tests' premise. Such cases are counted and
/// skipped.
pub fn unrealistic(original: &Context, mutated: &Context) -> Option<String> {
    for (i, m) in mutated.metas.iter().enumerate() {
        if !m.is_signer || on_curve(&m.key) {
            continue;
        }
        let was_the_fixtures = original.metas.iter().any(|o| o.is_signer && o.key == m.key);
        if !was_the_fixtures {
            return Some(format!(
                "meta {i} signs with an off-curve key {}…",
                &hex(&m.key)[..16]
            ));
        }
    }
    None
}

/// Apply `plan` to a copy of `ctx`. Mutations that do not apply to this context (no vault, no
/// signer to clear, …) are recorded as skipped so the report still names them.
pub fn apply(ctx: &Context, plan: &[Mutation]) -> (Context, Vec<Applied>) {
    let mut c = ctx.clone();
    let mut applied = Vec::with_capacity(plan.len());
    for m in plan {
        let detail = apply_one(&mut c, m);
        applied.push(Applied {
            kind: m.kind(),
            detail,
        });
    }
    (c, applied)
}

fn apply_one(c: &mut Context, m: &Mutation) -> String {
    match m {
        Mutation::FlipByte { offset, xor } => {
            if c.data.len() <= 8 {
                return "flip-byte: skipped (no payload)".into();
            }
            let i = 8 + offset % (c.data.len() - 8);
            c.data[i] ^= xor;
            format!("flip-byte: data[{i}] ^= {xor:#04x}")
        }
        Mutation::Truncate { len } => {
            if c.data.is_empty() {
                return "truncate: skipped (empty)".into();
            }
            let n = len % c.data.len();
            c.data.truncate(n);
            format!("truncate: data to {n} bytes")
        }
        Mutation::Append(bytes) => {
            c.data.extend_from_slice(bytes);
            format!("append: {} bytes", bytes.len())
        }
        Mutation::Count(v) => {
            if c.data.len() <= 8 {
                return "count: skipped (no payload)".into();
            }
            c.data[8] = *v;
            format!("count: data[8] = {v}")
        }
        Mutation::Proof(entries) => {
            if c.data.len() < 9 {
                return "proof: skipped (no count byte)".into();
            }
            c.data.truncate(9);
            c.data.extend_from_slice(
                &(u32::try_from(entries.len()).unwrap_or(u32::MAX)).to_le_bytes(),
            );
            for e in entries {
                c.data.extend_from_slice(e);
            }
            format!("proof: {} entries", entries.len())
        }
        Mutation::ClearSigner(n) => {
            let signers: Vec<usize> = (0..c.metas.len())
                .filter(|&i| c.metas[i].is_signer)
                .collect();
            if signers.is_empty() {
                return "clear-signer: skipped (no signer)".into();
            }
            let i = signers[n % signers.len()];
            c.metas[i].is_signer = false;
            format!("clear-signer: meta {i}")
        }
        Mutation::SetSigner(n) => {
            // Only a key on the curve can sign; a PDA set as a signer is no transaction.
            let others: Vec<usize> = (0..c.metas.len())
                .filter(|&i| !c.metas[i].is_signer && on_curve(&c.metas[i].key))
                .collect();
            if others.is_empty() {
                return "set-signer: skipped (no on-curve non-signer)".into();
            }
            let i = others[n % others.len()];
            c.metas[i].is_signer = true;
            format!("set-signer: meta {i}")
        }
        Mutation::Alias { from, to } => {
            if c.metas.len() < 2 {
                return "alias: skipped (one meta)".into();
            }
            let f = from % c.metas.len();
            let n = c.metas.len();
            // Another meta's key; when `f` signs, one that can sign (on the curve).
            let candidates: Vec<usize> = (0..n)
                .filter(|&t| t != f && (!c.metas[f].is_signer || on_curve(&c.metas[t].key)))
                .collect();
            if candidates.is_empty() {
                return "alias: skipped (no usable key)".into();
            }
            let t = candidates[to % candidates.len()];
            c.metas[f].key = c.metas[t].key;
            format!("alias: meta {f} := meta {t}'s key")
        }
        Mutation::Fresh { meta, key } => {
            if c.metas.is_empty() {
                return "fresh: skipped (no metas)".into();
            }
            let i = meta % c.metas.len();
            let key = &if c.metas[i].is_signer {
                nudge_on_curve(*key)
            } else {
                *key
            };
            c.metas[i].key = *key;
            if c.account(key).is_none() {
                c.accounts.push(Account {
                    key: *key,
                    lamports: 0,
                    data: Vec::new(),
                    owner: SYSTEM_PROGRAM,
                    executable: false,
                });
            }
            format!("fresh: meta {i} := {}…", &hex(key)[..16])
        }
        Mutation::Lamports { account, value } => {
            let vault = find_vault(c);
            let candidates: Vec<usize> = (0..c.accounts.len())
                .filter(|&i| Some(i) != vault && !c.accounts[i].executable)
                .collect();
            if candidates.is_empty() {
                return "lamports: skipped (no candidate)".into();
            }
            let i = candidates[account % candidates.len()];
            c.accounts[i].lamports = *value;
            format!("lamports: account {i} := {value}")
        }
        Mutation::VaultLamports { permille } => {
            let Some(v) = find_vault(c) else {
                return "vault-lamports: skipped (no vault in context)".into();
            };
            let before = c.accounts[v].lamports;
            let value = (u128::from(before) * 2 * u128::from(*permille) / 1000)
                .min(u128::from(u64::MAX)) as u64;
            c.accounts[v].lamports = value;
            format!("vault-lamports: {before} → {value}")
        }
        Mutation::Plant {
            target,
            offset,
            byte,
        } => {
            let Some(i) = find_target(c, *target) else {
                return format!("plant: skipped (no {target:?} in context)");
            };
            let len = c.accounts[i].data.len();
            if len <= 8 {
                return format!("plant: skipped ({target:?} has no body)");
            }
            let o = 8 + offset % (len - 8);
            let was = c.accounts[i].data[o];
            c.accounts[i].data[o] = *byte;
            format!("plant: {target:?} data[{o}] {was:#04x} → {byte:#04x}")
        }
        Mutation::Clock { delta } => {
            c.unix_timestamp = c.unix_timestamp.saturating_add(*delta);
            format!("clock: {delta:+} s")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Meta;

    fn ctx() -> Context {
        Context {
            program_id: mybarpool::ID.to_bytes(),
            metas: vec![
                Meta {
                    key: [1; 32],
                    is_signer: true,
                    is_writable: true,
                },
                Meta {
                    key: [2; 32],
                    is_signer: false,
                    is_writable: false,
                },
            ],
            data: {
                let mut d = <mybarpool::instruction::Buy as Discriminator>::DISCRIMINATOR.to_vec();
                d.extend_from_slice(&[3, 0, 0, 0, 0]);
                d
            },
            accounts: vec![
                Account {
                    key: [1; 32],
                    lamports: 5,
                    data: vec![],
                    owner: SYSTEM_PROGRAM,
                    executable: false,
                },
                Account {
                    key: [2; 32],
                    lamports: 7,
                    data: vec![],
                    owner: SYSTEM_PROGRAM,
                    executable: false,
                },
            ],
            unix_timestamp: 1_800_000_000,
            compute_unit_limit: 200_000,
        }
    }

    #[test]
    fn every_mutation_applies_or_says_why() {
        let c = ctx();
        assert!(Shape::of(&c).is_buy);
        let plan = vec![
            Mutation::FlipByte { offset: 0, xor: 1 },
            Mutation::Count(26),
            Mutation::Proof(vec![[9; 32]; 2]),
            Mutation::ClearSigner(0),
            Mutation::SetSigner(0),
            Mutation::Alias { from: 0, to: 0 },
            Mutation::Fresh {
                meta: 1,
                key: [7; 32],
            },
            Mutation::Lamports {
                account: 1,
                value: 99,
            },
            Mutation::VaultLamports { permille: 500 },
            Mutation::Plant {
                target: Target::Pool,
                offset: 3,
                byte: 1,
            },
            Mutation::Clock { delta: -5 },
            Mutation::Truncate { len: 2 },
            Mutation::Append(vec![1, 2, 3]),
        ];
        let (m, applied) = apply(&c, &plan);
        assert_eq!(applied.len(), plan.len());
        assert_eq!(m.unix_timestamp, c.unix_timestamp - 5);
        assert_eq!(m.accounts.len(), 3); // the fresh key's empty account
        assert!(applied
            .iter()
            .any(|a| a.detail.starts_with("vault-lamports: skipped")));
        assert!(applied
            .iter()
            .any(|a| a.detail.starts_with("plant: skipped")));
        assert!(applied.iter().all(|a| !a.detail.is_empty()));
    }

    proptest! {
        #[test]
        fn plans_never_panic(p in plan(Shape { metas: 2, accounts: 2, data_len: 13, is_buy: true })) {
            let (_, applied) = apply(&ctx(), &p);
            prop_assert_eq!(applied.len(), p.len());
        }
    }
}

//! Mollusk glue (feature `runner`): fixtures on disk ↔ the plain model.

use std::path::{Path, PathBuf};

use mollusk_svm_fuzz_fixture::Fixture;
use mybarpool_fuzz::model::{Account, Context, Meta};

/// One seed: the fixture, its plain context, and the file it came from (named on failure).
#[allow(dead_code)] // the replay and the report (the fuzzer commit) read `path` and `fixture`
pub struct Seed {
    pub path: PathBuf,
    pub fixture: Fixture,
    pub context: Context,
}

/// Every `*.fix` file under `dir`, in path order.
pub fn load(dir: &Path) -> std::io::Result<Vec<Seed>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "fix"))
        .collect();
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|path| {
            let fixture = Fixture::load_from_blob_file(path.to_str().expect("utf-8 path"));
            let context = context_of(&fixture);
            Seed {
                path,
                fixture,
                context,
            }
        })
        .collect())
}

/// The fixture's input as the plain model.
pub fn context_of(f: &Fixture) -> Context {
    let input = &f.input;
    Context {
        program_id: input.program_id.to_bytes(),
        metas: input
            .instruction_accounts
            .iter()
            .map(|m| Meta {
                key: m.pubkey.to_bytes(),
                is_signer: m.is_signer,
                is_writable: m.is_writable,
            })
            .collect(),
        data: input.instruction_data.clone(),
        accounts: input
            .accounts
            .iter()
            .map(|(k, a)| Account {
                key: k.to_bytes(),
                lamports: a.lamports,
                data: a.data.clone(),
                owner: a.owner.to_bytes(),
                executable: a.executable,
            })
            .collect(),
        unix_timestamp: input.sysvars.clock.unix_timestamp,
        compute_unit_limit: input.compute_budget.compute_unit_limit,
    }
}

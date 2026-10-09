//! `mybarpool-fuzz --fixtures <dir> [--seconds <n> | --cases <n>] [--seed <u64>] [--filter buy|settle|all]`
//!
//! The skeleton loads the fixtures and reports the count per instruction; the fuzzer commit
//! adds the replay, the mutations and the oracle.

mod runner;

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut fixtures: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--fixtures" => fixtures = args.next().map(PathBuf::from),
            "--seconds" | "--cases" | "--seed" | "--filter" => {
                args.next();
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let Some(dir) = fixtures else {
        eprintln!(
            "usage: mybarpool-fuzz --fixtures <dir> [--seconds <n>] [--cases <n>] [--seed <u64>] [--filter buy|settle|all]"
        );
        std::process::exit(2);
    };
    let seeds = match runner::load(&dir) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            eprintln!("{}: {e}", dir.display());
            std::process::exit(1);
        }
    };
    let groups = mybarpool_fuzz::seeds::group(&seeds, |s| &s.context);
    println!(
        "{} fixtures in {} instruction groups",
        seeds.len(),
        groups.len()
    );
    for (name, group) in &groups {
        println!(
            "  {}: {}",
            name.unwrap_or("(other programs / unknown)"),
            group.len()
        );
    }
}

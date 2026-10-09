//! `mybarpool-fuzz --fixtures <dir> [--seconds <n> | --cases <n>] [--seed <u64>]
//! [--filter buy|settle|all] [--program <mybarpool.so>] [--entropy <entropy.so>]`
//!
//! Replays every selected seed first (a mismatch is a harness or determinism bug and stops the
//! run), then draws cases — one seed, one to three mutations — until the budget is spent,
//! checking the PROGRAM §10 oracle on each. Prints the proptest seed it used; `--seed`
//! reproduces a run exactly. On a failure, prints the fixture file, the mutations and the
//! minimal shrunk case, and exits 1.

mod runner;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use mybarpool_fuzz::mutate::{self, Kind, Mutation, Shape};
use mybarpool_fuzz::oracle;
use mybarpool_fuzz::seeds::{group, instruction_name};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

struct Args {
    fixtures: PathBuf,
    seconds: Option<u64>,
    cases: Option<u64>,
    seed: Option<u64>,
    filter: String,
    program: PathBuf,
    entropy: PathBuf,
    inspect: Option<PathBuf>,
}

fn usage() -> ! {
    eprintln!(
        "usage: mybarpool-fuzz --fixtures <dir> [--seconds <n>] [--cases <n>] [--seed <u64>] \
         [--filter buy|settle|all] [--program target/deploy/mybarpool.so] \
         [--entropy programs/mybarpool/tests/fixtures/entropy-486225b.so]\n       \
         mybarpool-fuzz --inspect <fixture.fix>"
    );
    std::process::exit(2)
}

fn parse() -> Args {
    let mut a = Args {
        fixtures: PathBuf::new(),
        seconds: None,
        cases: None,
        seed: None,
        filter: "all".into(),
        program: PathBuf::from("target/deploy/mybarpool.so"),
        entropy: PathBuf::from("programs/mybarpool/tests/fixtures/entropy-486225b.so"),
        inspect: None,
    };
    let mut it = std::env::args().skip(1);
    let mut have_fixtures = false;
    while let Some(k) = it.next() {
        let Some(val) = it.next() else { usage() };
        match k.as_str() {
            "--fixtures" => {
                a.fixtures = PathBuf::from(val);
                have_fixtures = true;
            }
            "--seconds" => a.seconds = Some(val.parse().unwrap_or_else(|_| usage())),
            "--cases" => a.cases = Some(val.parse().unwrap_or_else(|_| usage())),
            "--seed" => a.seed = Some(val.parse().unwrap_or_else(|_| usage())),
            "--filter" => a.filter = val,
            "--program" => a.program = PathBuf::from(val),
            "--entropy" => a.entropy = PathBuf::from(val),
            "--inspect" => a.inspect = Some(PathBuf::from(val)),
            _ => usage(),
        }
    }
    if a.inspect.is_some() {
        return a;
    }
    if !have_fixtures || !matches!(a.filter.as_str(), "buy" | "settle" | "all") {
        usage();
    }
    if a.seconds.is_none() && a.cases.is_none() {
        a.cases = Some(1_000);
    }
    a
}

fn main() {
    let args = parse();
    if let Some(path) = &args.inspect {
        runner::inspect(path);
        return;
    }
    let seeds = runner::load(&args.fixtures).unwrap_or_else(|e| {
        eprintln!("{}: {e}", args.fixtures.display());
        std::process::exit(1)
    });
    let groups = group(&seeds, |s| &s.context);
    println!("{} fixtures in {} groups", seeds.len(), groups.len());
    for (name, g) in &groups {
        println!(
            "  {}: {}",
            name.unwrap_or("(other programs / unknown)"),
            g.len()
        );
    }
    let selected: Vec<&runner::Seed> = seeds
        .iter()
        .filter(
            |s| match (args.filter.as_str(), instruction_name(&s.context)) {
                ("all", Some(_)) => true,
                (f, Some(name)) => name == f,
                (_, None) => false,
            },
        )
        .collect();
    println!("filter {}: {} seeds", args.filter, selected.len());
    if selected.is_empty() {
        eprintln!("nothing to fuzz");
        std::process::exit(1);
    }

    let program = std::fs::read(&args.program).unwrap_or_else(|e| {
        eprintln!(
            "{}: {e} (run `anchor build --arch v3` first)",
            args.program.display()
        );
        std::process::exit(1)
    });
    let entropy = std::fs::read(&args.entropy).unwrap_or_else(|e| {
        eprintln!("{}: {e}", args.entropy.display());
        std::process::exit(1)
    });
    let mut m = runner::mollusk(&program, &entropy);

    // Replay first.
    let t0 = Instant::now();
    for s in &selected {
        if let Err(e) = runner::replay(&mut m, s) {
            eprintln!("REPLAY MISMATCH: {e}");
            std::process::exit(1);
        }
    }
    println!(
        "replayed {} seeds unchanged in {:.1?}",
        selected.len(),
        t0.elapsed()
    );

    // The proptest seed: given, or fresh and printed.
    let seed_u64 = args.seed.unwrap_or_else(|| {
        let mut b = [0u8; 8];
        getrandom(&mut b);
        u64::from_le_bytes(b)
    });
    println!("proptest seed: {seed_u64} (reproduce with --seed {seed_u64})");
    let mut rng_seed = [0u8; 32];
    rng_seed[..8].copy_from_slice(&seed_u64.to_le_bytes());
    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::from_seed(RngAlgorithm::ChaCha, &rng_seed),
    );
    let shapes: Vec<Shape> = selected.iter().map(|s| Shape::of(&s.context)).collect();
    let n = selected.len();
    let strategy =
        (0..n).prop_flat_map(move |i| (proptest::strategy::Just(i), mutate::plan(shapes[i])));

    let deadline = args
        .seconds
        .map(|s| Instant::now() + Duration::from_secs(s));
    let max_cases = args.cases.unwrap_or(u64::MAX);
    let mut cases = 0u64;
    let mut by_kind: BTreeMap<Kind, u64> = BTreeMap::new();
    let mut by_result: BTreeMap<String, u64> = BTreeMap::new();
    let start = Instant::now();
    let mut report_at = start + Duration::from_secs(60);

    let mut unrealistic = 0u64;
    let evaluate = |m: &mut mollusk_svm::Mollusk, (i, plan): &(usize, Vec<Mutation>)| {
        let seed = selected[*i];
        let (ctx, applied) = mutate::apply(&seed.context, plan);
        if let Some(why) = mutate::unrealistic(&seed.context, &ctx) {
            return (applied, None, Vec::new(), Some(why));
        }
        let outcome = runner::run(m, seed, &ctx);
        let violations = oracle::check(&ctx, &outcome);
        (applied, Some(outcome), violations, None)
    };
    let mut o4 = (0u64, 0u64); // (held, not applicable)

    while cases < max_cases && deadline.is_none_or(|d| Instant::now() < d) {
        let mut tree = strategy.new_tree(&mut runner).expect("strategy");
        let (applied, outcome, violations, skipped) = evaluate(&mut m, &tree.current());
        cases += 1;
        for a in &applied {
            *by_kind.entry(a.kind).or_default() += 1;
        }
        if skipped.is_some() {
            unrealistic += 1;
            continue;
        }
        let outcome = outcome.expect("an outcome when not skipped");
        *by_result.entry(result_label(&outcome.result)).or_default() += 1;
        if outcome.result == mybarpool_fuzz::model::Result::Success {
            let (ctx, _) = mutate::apply(&selected[tree.current().0].context, &tree.current().1);
            for c in oracle::o4_vaults(&ctx, &outcome) {
                match c {
                    oracle::VaultCheck::Holds => o4.0 += 1,
                    oracle::VaultCheck::NotApplicable(_) => o4.1 += 1,
                    oracle::VaultCheck::Broken(_) => {}
                }
            }
        }
        if Instant::now() >= report_at {
            println!("… {cases} cases in {:.0?}", start.elapsed());
            report_at += Duration::from_secs(60);
        }
        if violations.is_empty() {
            continue;
        }
        // Shrink: simplify while it still fails; complicate back when it stops failing.
        let mut best = (tree.current().clone(), applied, outcome, violations);
        let mut steps = 0;
        let mut direction_simplify = true;
        while steps < 2_000 {
            let moved = if direction_simplify {
                tree.simplify()
            } else {
                tree.complicate()
            };
            if !moved {
                break;
            }
            steps += 1;
            let (a, o, vs, _) = evaluate(&mut m, &tree.current());
            match o {
                Some(o) if !vs.is_empty() => {
                    best = (tree.current().clone(), a, o, vs);
                    direction_simplify = true;
                }
                _ => direction_simplify = false,
            }
        }
        let (case, applied, outcome, violations) = best;
        let seed = selected[case.0];
        eprintln!("\nORACLE VIOLATION after {cases} cases (proptest seed {seed_u64})");
        eprintln!("fixture: {}", seed.path.display());
        eprintln!(
            "instruction: {}",
            instruction_name(&seed.context).unwrap_or("?")
        );
        eprintln!("mutations ({} after {steps} shrink steps):", applied.len());
        for a in &applied {
            eprintln!("  [{}] {}", a.kind.name(), a.detail);
        }
        eprintln!("plan: {:?}", case.1);
        eprintln!(
            "result: {:?} ({} CU)",
            outcome.result, outcome.compute_units
        );
        for vio in &violations {
            eprintln!("  {}: {}", vio.line, vio.detail);
        }
        std::process::exit(1);
    }

    println!(
        "clean: {cases} cases in {:.1?} over {n} seeds (proptest seed {seed_u64}); {unrealistic} \
         skipped as no possible transaction (a signer meta with an off-curve key)",
        start.elapsed()
    );
    println!(
        "O4 vault invariant: held on {} successful pool results; not applicable on {} (the input \
         was already off §3.4 or its pool broke a §3.3 relation)",
        o4.0, o4.1
    );
    println!("mutations by kind:");
    for k in Kind::ALL {
        println!("  {}: {}", k.name(), by_kind.get(&k).copied().unwrap_or(0));
    }
    println!("results:");
    for (label, count) in &by_result {
        println!("  {label}: {count}");
    }
}

fn result_label(r: &mybarpool_fuzz::model::Result) -> String {
    use mybarpool_fuzz::model::Result as R;
    match r {
        R::Success => "success".into(),
        R::Custom(c) if (6000..7000).contains(c) => format!("custom {c} (program)"),
        R::Custom(c) if (2000..4000).contains(c) => format!("custom {c} (anchor)"),
        R::Custom(c) => format!("custom {c}"),
        R::Runtime(s) => format!("runtime {s}"),
        R::FailedToComplete => "FAILED TO COMPLETE".into(),
    }
}

/// Eight random bytes from the OS, without a dependency (`/dev/urandom`, else the clock).
fn getrandom(out: &mut [u8; 8]) {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(out).is_ok() {
            return;
        }
    }
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    *out = t.to_le_bytes();
}

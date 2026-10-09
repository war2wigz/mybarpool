//! Schema snapshot (PROGRAM §10 Layout, build plan Step 8): the committed IDL against
//! `tests/fixtures/schema-snapshot.json`. The snapshot holds every account's fields in order
//! with its size, every event's fields in order, every instruction's discriminator, arguments
//! and account list, and every error's name and code. Anything in the snapshot must be in the
//! IDL unchanged and in the same position; the IDL may add only what §10 allows: a new
//! instruction, a new event, a new trailing event field, a new error with the next code, and a
//! new account field immediately before `reserved` with `reserved` shorter by its size and
//! the account's size unchanged. Any other difference fails naming the item.
//!
//! `UPDATE_SCHEMA_SNAPSHOT=1 cargo test -p mybarpool --test schema` rewrites the file. The
//! test fails if the file is missing, so a fresh clone cannot silently pass.
//!
//! Sizes come from the program's `SIZE` constants (`layout.rs` keeps the byte offsets); the
//! field types are rendered the IDL's way (the `type` value as JSON).

use std::collections::BTreeMap;
use std::path::PathBuf;

use mybarpool::{CreatorCounter, GameRecord, PlatformConfig, Pool, Sponsorship, WalletOverride};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn idl_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../idl/mybarpool.json")
}

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema-snapshot.json")
}

fn load_idl() -> Value {
    let text = std::fs::read_to_string(idl_path()).expect("idl/mybarpool.json");
    serde_json::from_str(&text).expect("valid IDL JSON")
}

// ---------------------------------------------------------------------------
// The snapshot's shape
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    /// The IDL's `type` value, verbatim (`"u64"`, `{"array": ["u8", 32]}`, `{"defined": …}`).
    #[serde(rename = "type")]
    pub ty: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSchema {
    pub name: String,
    /// The program's `SIZE` constant (discriminator included).
    pub size: usize,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSchema {
    pub name: String,
    pub discriminator: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMetaSchema {
    pub name: String,
    pub writable: bool,
    pub signer: bool,
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionSchema {
    pub name: String,
    /// The 8-byte discriminator as lowercase hex.
    pub discriminator: String,
    pub args: Vec<Field>,
    pub accounts: Vec<AccountMetaSchema>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorSchema {
    pub name: String,
    pub code: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub spec: String,
    pub accounts: Vec<AccountSchema>,
    pub events: Vec<EventSchema>,
    pub instructions: Vec<InstructionSchema>,
    pub errors: Vec<ErrorSchema>,
}

fn hex(bytes: &[Value]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b.as_u64().expect("byte")))
        .collect()
}

fn fields_of(type_def: &Value) -> Vec<Field> {
    type_def["type"]["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("{} has struct fields", type_def["name"]))
        .iter()
        .map(|f| Field {
            name: f["name"].as_str().expect("field name").to_string(),
            ty: f["type"].clone(),
        })
        .collect()
}

/// The program's `SIZE` for each account type (the IDL has no sizes; `layout.rs` pins the
/// offsets these imply).
pub fn account_size(name: &str) -> usize {
    match name {
        "CreatorCounter" => CreatorCounter::SIZE,
        "GameRecord" => GameRecord::SIZE,
        "PlatformConfig" => PlatformConfig::SIZE,
        "Pool" => Pool::SIZE,
        "Sponsorship" => Sponsorship::SIZE,
        "WalletOverride" => WalletOverride::SIZE,
        other => panic!("no SIZE known for account type {other}; add it here and to layout.rs"),
    }
}

/// Build the snapshot from an IDL value.
pub fn snapshot_of(idl: &Value) -> Snapshot {
    let types: BTreeMap<&str, &Value> = idl["types"]
        .as_array()
        .expect("types")
        .iter()
        .map(|t| (t["name"].as_str().expect("type name"), t))
        .collect();
    let accounts = idl["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|a| {
            let name = a["name"].as_str().expect("account name");
            AccountSchema {
                name: name.to_string(),
                size: account_size(name),
                fields: fields_of(types[name]),
            }
        })
        .collect();
    let events = idl["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|e| {
            let name = e["name"].as_str().expect("event name");
            EventSchema {
                name: name.to_string(),
                discriminator: hex(e["discriminator"].as_array().expect("disc")),
                fields: fields_of(types[name]),
            }
        })
        .collect();
    let instructions = idl["instructions"]
        .as_array()
        .expect("instructions")
        .iter()
        .map(|i| InstructionSchema {
            name: i["name"].as_str().expect("ix name").to_string(),
            discriminator: hex(i["discriminator"].as_array().expect("disc")),
            args: i["args"]
                .as_array()
                .expect("args")
                .iter()
                .map(|a| Field {
                    name: a["name"].as_str().expect("arg name").to_string(),
                    ty: a["type"].clone(),
                })
                .collect(),
            accounts: i["accounts"]
                .as_array()
                .expect("ix accounts")
                .iter()
                .map(|a| AccountMetaSchema {
                    name: a["name"].as_str().expect("meta name").to_string(),
                    writable: a["writable"].as_bool().unwrap_or(false),
                    signer: a["signer"].as_bool().unwrap_or(false),
                    optional: a["optional"].as_bool().unwrap_or(false),
                })
                .collect(),
        })
        .collect();
    let errors = idl["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|e| ErrorSchema {
            name: e["name"].as_str().expect("error name").to_string(),
            code: u32::try_from(e["code"].as_u64().expect("code")).expect("u32"),
        })
        .collect();
    Snapshot {
        spec: "PROGRAM §10 Layout: account fields and sizes, event fields, instruction \
               discriminators, arguments and accounts, error names and codes; additive \
               changes only"
            .to_string(),
        accounts,
        events,
        instructions,
        errors,
    }
}

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// Byte size of an IDL field type, for the `reserved`-shrink rule. `None` for a type the rule
/// does not need to size (a `defined` enum's one byte is the only indirect case handled).
fn type_size(ty: &Value) -> Option<usize> {
    match ty {
        Value::String(s) => match s.as_str() {
            "bool" | "u8" | "i8" => Some(1),
            "u16" | "i16" => Some(2),
            "u32" | "i32" | "f32" => Some(4),
            "u64" | "i64" | "f64" => Some(8),
            "u128" | "i128" => Some(16),
            "pubkey" => Some(32),
            _ => None,
        },
        Value::Object(o) => {
            if let Some(arr) = o.get("array") {
                let inner = type_size(&arr[0])?;
                let n = usize::try_from(arr[1].as_u64()?).ok()?;
                Some(inner * n)
            } else if o.get("defined").is_some() {
                // Every `defined` field in an account is a one-byte enum here (PoolStatus,
                // AccessType, PayoutPreset, GameStatus); a struct would need the IDL types.
                Some(1)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn reserved_len(f: &Field) -> Option<usize> {
    if f.name != "reserved" {
        return None;
    }
    let arr = f.ty.get("array")?;
    (arr[0] == "u8").then(|| usize::try_from(arr[1].as_u64()?).ok())?
}

/// Compare the committed snapshot with the IDL's; `Ok` when the IDL is the snapshot plus
/// additive changes only, else the first difference, naming the item.
pub fn compare(snapshot: &Snapshot, idl: &Snapshot) -> Result<(), String> {
    // Accounts: same set in the same order (an account type is never added or removed
    // without a plan); fields identical, or one new field immediately before `reserved`
    // with `reserved` shorter by its size and the size unchanged.
    let names = |xs: &[AccountSchema]| xs.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
    if names(&snapshot.accounts) != names(&idl.accounts) {
        return Err(format!(
            "accounts: {:?} in the snapshot, {:?} in the IDL",
            names(&snapshot.accounts),
            names(&idl.accounts)
        ));
    }
    for (s, i) in snapshot.accounts.iter().zip(&idl.accounts) {
        if s.size != i.size {
            return Err(format!("account {}: size {} → {}", s.name, s.size, i.size));
        }
        compare_account_fields(&s.name, &s.fields, &i.fields)?;
    }

    // Events: every snapshot event present unchanged (new trailing fields allowed); new
    // events allowed after.
    for s in &snapshot.events {
        let Some(i) = idl.events.iter().find(|e| e.name == s.name) else {
            return Err(format!("event {} removed", s.name));
        };
        if s.discriminator != i.discriminator {
            return Err(format!("event {}: discriminator changed", s.name));
        }
        if i.fields.len() < s.fields.len() {
            return Err(format!("event {}: field removed", s.name));
        }
        for (k, (a, b)) in s.fields.iter().zip(&i.fields).enumerate() {
            if a != b {
                return Err(format!(
                    "event {}: field {k} is {} ({}) in the snapshot, {} ({}) in the IDL",
                    s.name, a.name, a.ty, b.name, b.ty
                ));
            }
        }
    }

    // Instructions: every snapshot instruction identical; new ones allowed.
    for s in &snapshot.instructions {
        let Some(i) = idl.instructions.iter().find(|x| x.name == s.name) else {
            return Err(format!("instruction {} removed", s.name));
        };
        if s != i {
            let what = if s.discriminator != i.discriminator {
                "discriminator"
            } else if s.args != i.args {
                "arguments"
            } else {
                "account list"
            };
            return Err(format!("instruction {}: {what} changed", s.name));
        }
    }

    // Errors: the snapshot's prefix unchanged; new ones take the next codes in order.
    if idl.errors.len() < snapshot.errors.len() {
        return Err("errors: one removed".to_string());
    }
    for (k, (a, b)) in snapshot.errors.iter().zip(&idl.errors).enumerate() {
        if a != b {
            return Err(format!(
                "error {k}: {} ({}) in the snapshot, {} ({}) in the IDL",
                a.name, a.code, b.name, b.code
            ));
        }
    }
    let mut next = snapshot.errors.last().map_or(6000, |e| e.code + 1);
    for e in &idl.errors[snapshot.errors.len()..] {
        if e.code != next {
            return Err(format!(
                "error {}: code {} but the next free code is {next}",
                e.name, e.code
            ));
        }
        next += 1;
    }
    Ok(())
}

fn compare_account_fields(account: &str, s: &[Field], i: &[Field]) -> Result<(), String> {
    if s == i {
        return Ok(());
    }
    // The one allowed shape: `i` is `s` with one field inserted immediately before `reserved`
    // and `reserved` shrunk by exactly its size.
    let Some(r) = s.iter().position(|f| f.name == "reserved") else {
        return Err(format!(
            "account {account}: fields changed (no reserved padding)"
        ));
    };
    if i.len() != s.len() + 1 || r != s.len() - 1 {
        return Err(first_field_difference(account, s, i));
    }
    if s[..r] != i[..r] {
        return Err(first_field_difference(account, s, i));
    }
    let added = &i[r];
    let (Some(before), Some(after)) = (reserved_len(&s[r]), reserved_len(&i[r + 1])) else {
        return Err(format!("account {account}: reserved is not a [u8; N]"));
    };
    let Some(size) = type_size(&added.ty) else {
        return Err(format!(
            "account {account}: cannot size the new field {} ({})",
            added.name, added.ty
        ));
    };
    if before != after + size {
        return Err(format!(
            "account {account}: new field {} is {size} bytes but reserved went {before} → {after}",
            added.name
        ));
    }
    Ok(())
}

fn first_field_difference(account: &str, s: &[Field], i: &[Field]) -> String {
    for (k, (a, b)) in s.iter().zip(i).enumerate() {
        if a != b {
            return format!(
                "account {account}: field {k} is {} ({}) in the snapshot, {} ({}) in the IDL",
                a.name, a.ty, b.name, b.ty
            );
        }
    }
    format!(
        "account {account}: {} fields in the snapshot, {} in the IDL",
        s.len(),
        i.len()
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn the_committed_idl_matches_the_schema_snapshot_or_extends_it_additively() {
    let current = snapshot_of(&load_idl());
    let path = snapshot_path();
    if std::env::var("UPDATE_SCHEMA_SNAPSHOT").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&current).unwrap() + "\n",
        )
        .unwrap();
        println!("wrote {}", path.display());
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} is missing ({e}); run UPDATE_SCHEMA_SNAPSHOT=1 cargo test -p mybarpool --test schema",
            path.display()
        )
    });
    let committed: Snapshot = serde_json::from_str(&text).expect("snapshot JSON");
    compare(&committed, &current).unwrap_or_else(|e| panic!("PROGRAM §10 Layout: {e}"));
    // The Step 8 freeze: 26 / 6 / 24 / 65.
    assert_eq!(
        (
            current.instructions.len(),
            current.accounts.len(),
            current.events.len(),
            current.errors.len()
        ),
        (26, 6, 24, 65)
    );
    assert_eq!(
        current
            .accounts
            .iter()
            .find(|a| a.name == "Pool")
            .unwrap()
            .size,
        1_442
    );
}

fn base() -> Snapshot {
    snapshot_of(&load_idl())
}

#[test]
fn a_reordered_account_field_is_named() {
    let committed = base();
    let mut mutated = committed.clone();
    let pool = mutated
        .accounts
        .iter_mut()
        .find(|a| a.name == "Pool")
        .unwrap();
    pool.fields.swap(0, 1); // game ↔ creator
    let err = compare(&committed, &mutated).unwrap_err();
    assert!(
        err.contains("account Pool") && err.contains("field 0"),
        "{err}"
    );
    assert!(err.contains("game") && err.contains("creator"), "{err}");
}

#[test]
fn a_removed_event_field_is_named() {
    let committed = base();
    let mut mutated = committed.clone();
    let ev = mutated
        .events
        .iter_mut()
        .find(|e| e.name == "BoxesBought")
        .unwrap();
    ev.fields.pop();
    let err = compare(&committed, &mutated).unwrap_err();
    assert_eq!(err, "event BoxesBought: field removed");
}

#[test]
fn a_renumbered_error_is_named() {
    let committed = base();
    let mut mutated = committed.clone();
    let e = mutated
        .errors
        .iter_mut()
        .find(|e| e.name == "GateKeyNotSigner")
        .unwrap();
    e.code = 6999;
    let err = compare(&committed, &mutated).unwrap_err();
    assert!(
        err.starts_with("error 17:") && err.contains("GateKeyNotSigner (6017)"),
        "{err}"
    );
}

#[test]
fn the_additive_changes_pass_and_everything_else_fails() {
    let committed = base();

    // A new instruction, a new event, a new trailing event field, a new error: fine.
    let mut more = committed.clone();
    more.instructions.push(InstructionSchema {
        name: "something_new".into(),
        discriminator: "00".repeat(8),
        args: vec![],
        accounts: vec![],
    });
    more.events.push(EventSchema {
        name: "SomethingHappened".into(),
        discriminator: "ff".repeat(8),
        fields: vec![],
    });
    more.events[0].fields.push(Field {
        name: "extra".into(),
        ty: Value::String("u8".into()),
    });
    more.errors.push(ErrorSchema {
        name: "SomethingNew".into(),
        code: 6065,
    });
    compare(&committed, &more).unwrap();

    // A new error that skips a code: not fine.
    let mut skip = committed.clone();
    skip.errors.push(ErrorSchema {
        name: "SomethingNew".into(),
        code: 6070,
    });
    assert!(compare(&committed, &skip)
        .unwrap_err()
        .contains("next free code is 6065"));

    // A new Pool field before reserved, reserved shrunk by its size: fine (the Step 5b shape).
    let mut grown = committed.clone();
    let pool = grown
        .accounts
        .iter_mut()
        .find(|a| a.name == "Pool")
        .unwrap();
    let r = pool.fields.len() - 1;
    assert_eq!(pool.fields[r].name, "reserved");
    pool.fields.insert(
        r,
        Field {
            name: "new_commit".into(),
            ty: serde_json::json!({ "array": ["u8", 32] }),
        },
    );
    pool.fields[r + 1].ty = serde_json::json!({ "array": ["u8", 64] });
    compare(&committed, &grown).unwrap();

    // The same with reserved left alone: the size would have changed.
    let mut wrong = committed.clone();
    let pool = wrong
        .accounts
        .iter_mut()
        .find(|a| a.name == "Pool")
        .unwrap();
    pool.fields.insert(
        r,
        Field {
            name: "new_commit".into(),
            ty: serde_json::json!({ "array": ["u8", 32] }),
        },
    );
    assert!(compare(&committed, &wrong)
        .unwrap_err()
        .contains("reserved went 96 → 96"));

    // An instruction's account list changing (a Step 8 `buy` without `gate_key`): not fine.
    let mut ix = committed.clone();
    let buy = ix
        .instructions
        .iter_mut()
        .find(|i| i.name == "buy")
        .unwrap();
    buy.accounts.retain(|a| a.name != "gate_key");
    assert_eq!(
        compare(&committed, &ix).unwrap_err(),
        "instruction buy: account list changed"
    );

    // The Pool size changing: not fine.
    let mut size = committed.clone();
    size.accounts
        .iter_mut()
        .find(|a| a.name == "Pool")
        .unwrap()
        .size += 1;
    assert_eq!(
        compare(&committed, &size).unwrap_err(),
        "account Pool: size 1442 → 1443"
    );
}

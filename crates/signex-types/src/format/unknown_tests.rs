//! Forward-compatibility guards: keys a newer Signex wrote, which this
//! build does not recognise, must survive a load -> save cycle on the
//! in-memory model (not just on the discarded codec wrapper).

use super::tests::{empty_sheet, sample_symbol};
use super::*;
use crate::schematic::SchematicSheet;

const SYMBOL_UUID: &str = "0192a8c0-0001-7000-8000-000000000001";

/// A sheet holding one all-default symbol, so its `[extras]` tree is
/// empty until a test injects foreign keys.
fn plain_sheet() -> SchematicSheet {
    let mut sheet = empty_sheet();
    let mut symbol = sample_symbol();
    symbol.footprint.clear();
    sheet.symbols.push(symbol);
    sheet
}

/// Parse `text`, panicking with the codec error on failure.
fn load(text: &str) -> SchematicSheet {
    SnxSchematic::parse(text).unwrap().sheet
}

/// Save a bare sheet.
fn save(sheet: &SchematicSheet) -> String {
    SnxSchematic::new(sheet.clone()).write_string().unwrap()
}

/// A plain file with `vendor_annotation` prepended to the manifest.
fn with_manifest_key() -> String {
    format!(
        "vendor_annotation = \"important-data\"\n{}",
        save(&plain_sheet())
    )
}

#[test]
fn unknown_manifest_key_survives() {
    let sheet = load(&with_manifest_key());
    assert_eq!(sheet.unknown.manifest.len(), 1);
    assert!(save(&sheet).contains("vendor_annotation = \"important-data\""));
}

#[test]
fn unknown_manifest_table_survives() {
    let text = format!(
        "{}\n[vendor]\nk = \"v\"\n[vendor.deep]\nn = 1\n",
        save(&plain_sheet())
    );
    let sheet = load(&text);
    assert!(sheet.unknown.manifest.contains_key("vendor"));
    let again = load(&save(&sheet));
    assert_eq!(again.unknown, sheet.unknown);
}

#[test]
fn unknown_symbol_extras_key_survives_and_is_not_elided() {
    // The symbol is otherwise all-default, so without the is_default()
    // guard its extras entry would be dropped on save.
    let text = format!(
        "{}\n[extras.symbols.{SYMBOL_UUID}]\nfuture_symbol_key = 42\n\n[extras.symbols.{SYMBOL_UUID}.vendor]\nnote = \"n\"\n",
        save(&plain_sheet())
    );
    let sheet = load(&text);
    assert_eq!(sheet.symbols[0].unknown.len(), 2);
    let out = save(&sheet);
    assert!(out.contains("future_symbol_key = 42"));
    assert!(out.contains("note = \"n\""));
    assert_eq!(load(&out).symbols[0].unknown, sheet.symbols[0].unknown);
}

#[test]
fn unknown_sheet_extras_key_survives_and_stays_apart_from_manifest() {
    let text = format!(
        "same_name = \"manifest\"\n{}\n[extras.sheet]\nsame_name = \"sheet\"\n[extras.sheet.vendor]\nz = 1\n",
        save(&plain_sheet())
    );
    let sheet = load(&text);
    assert_eq!(sheet.unknown.manifest.len(), 1);
    assert_eq!(sheet.unknown.extras.len(), 2);
    let again = load(&save(&sheet));
    assert_eq!(again.unknown, sheet.unknown);
}

#[test]
fn load_save_without_edit_is_byte_stable() {
    let text = format!(
        "{}\n[extras.sheet]\nfuture = 1\n[extras.symbols.{SYMBOL_UUID}]\nfuture_symbol_key = 2\n",
        with_manifest_key()
    );
    let first = save(&load(&text));
    let second = save(&load(&first));
    assert_eq!(first, second);
    assert!(first.contains("vendor_annotation"));
    assert!(first.contains("future = 1"));
    assert!(first.contains("future_symbol_key = 2"));
}

#[test]
fn unknowns_survive_editing_an_unrelated_symbol() {
    let mut sheet = load(&with_manifest_key());
    let mut other = sample_symbol();
    other.uuid = Uuid::parse_str("0192a8c0-0001-7000-8000-000000000002").unwrap();
    other.reference = "R9".into();
    sheet.symbols.push(other);
    sheet.symbols[0].reference = "U7".into();
    let out = save(&sheet);
    assert!(out.contains("vendor_annotation = \"important-data\""));
    assert_eq!(load(&out).symbols.len(), 2);
}

#[test]
fn file_without_unknowns_has_empty_bags_and_unchanged_json() {
    let sheet = load(&save(&plain_sheet()));
    assert!(sheet.unknown.is_empty());
    assert!(sheet.symbols[0].unknown.is_empty());
    let json = serde_json::to_string(&sheet).unwrap();
    assert!(!json.contains("\"unknown\""));
}

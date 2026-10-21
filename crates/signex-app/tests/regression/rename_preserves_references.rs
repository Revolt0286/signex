//! #145 — renaming a project-tree file keeps its native extension and keeps
//! hierarchical sheet references pointing at it.
//!
//! Real temp project on disk: `root.snxsch` references `child.snxsch`. The
//! rename goes through the same `RenameMsg::Submit` dispatcher the dialog uses.

use signex_app::app::{LoadedProject, Message, RenameDialogState, RenameMsg, Signex};
use signex_types::project::{ProjectData, SheetEntry};
use signex_types::schematic::{ChildSheet, FillType, Point, SchematicSheet};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn sheet(children: &[&str]) -> SchematicSheet {
    SchematicSheet {
        uuid: uuid::Uuid::new_v4(),
        version: 0,
        generator: String::new(),
        generator_version: String::new(),
        paper_size: "A4".to_string(),
        root_sheet_page: "1".to_string(),
        symbols: Vec::new(),
        wires: Vec::new(),
        junctions: Vec::new(),
        labels: Vec::new(),
        child_sheets: children
            .iter()
            .map(|f| ChildSheet {
                uuid: uuid::Uuid::new_v4(),
                name: (*f).to_string(),
                filename: (*f).to_string(),
                position: Point::new(0.0, 0.0),
                size: (10.0, 10.0),
                stroke_width: 0.0,
                fill: FillType::None,
                stroke_color: None,
                fill_color: None,
                fields_autoplaced: false,
                pins: Vec::new(),
                instances: Vec::new(),
            })
            .collect(),
        no_connects: Vec::new(),
        text_notes: Vec::new(),
        buses: Vec::new(),
        bus_entries: Vec::new(),
        drawings: Vec::new(),
        no_erc_directives: Vec::new(),
        title_block: HashMap::new(),
        lib_symbols: HashMap::new(),
    }
}

fn write_sheet(path: &Path, s: SchematicSheet) {
    let text = signex_types::format::SnxSchematic::new(s)
        .write_string()
        .expect("serialise sheet");
    fs::write(path, text).expect("write sheet");
}

struct Fixture {
    app: Signex,
    _tmp: TempDir,
    root: PathBuf,
    child: PathBuf,
    prj: PathBuf,
}

/// `root.snxsch` (references `child.snxsch`) + `child.snxsch`, both listed in
/// a loaded project at tree index 0.
fn fixture() -> Fixture {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    let root = dir.join("root.snxsch");
    let child = dir.join("child.snxsch");
    let prj = dir.join("proj.snxprj");
    write_sheet(&root, sheet(&["child.snxsch"]));
    write_sheet(&child, sheet(&[]));
    fs::write(&prj, b"{}").expect("write prj");

    let entry = |f: &str| SheetEntry {
        name: f.to_string(),
        filename: f.to_string(),
        symbols_count: 0,
        wires_count: 0,
        labels_count: 0,
    };
    let (mut app, _task) = Signex::new();
    let id = app.document_state.mint_project_id();
    app.document_state.projects.push(LoadedProject {
        id,
        path: prj.clone(),
        data: ProjectData {
            name: "proj".to_string(),
            dir: dir.to_string_lossy().into_owned(),
            schematic_root: Some("root.snxsch".to_string()),
            pcb_file: None,
            sheets: vec![entry("root.snxsch"), entry("child.snxsch")],
            variant_definitions: Vec::new(),
            active_variant: None,
            libraries: Vec::new(),
            enable_git: false,
        },
        pending_libraries: HashMap::new(),
    });
    app.document_state.active_project = Some(id);
    Fixture {
        app,
        _tmp: tmp,
        root,
        child,
        prj,
    }
}

fn submit_rename(app: &mut Signex, target: &Path, buffer: &str) {
    app.ui_state.rename_dialog = Some(RenameDialogState {
        target_path: target.to_path_buf(),
        tree_path: vec![0, 1],
        buffer: buffer.to_string(),
        error: None,
        is_project_rename: false,
    });
    let _ = app.update(Message::Rename(RenameMsg::Submit));
}

fn child_refs(app: &Signex, parent: &Path) -> Vec<String> {
    app.document_state
        .engines
        .get(parent)
        .expect("parent engine")
        .document()
        .child_sheets
        .iter()
        .map(|c| c.filename.clone())
        .collect()
}

#[test]
fn renaming_a_child_sheet_updates_the_unopened_parents_reference() {
    let mut f = fixture();
    let root_before = fs::read(&f.root).expect("read root");

    submit_rename(&mut f.app, &f.child, "renamed.snxsch");

    assert!(f.app.ui_state.rename_dialog.is_none(), "rename succeeded");
    assert!(!f.child.exists());
    assert!(f.child.with_file_name("renamed.snxsch").exists());
    assert_eq!(child_refs(&f.app, &f.root), vec!["renamed.snxsch"]);
    assert!(
        f.app.document_state.dirty_paths.contains(&f.root),
        "the parent must be marked dirty for the user to save"
    );
    assert_eq!(
        fs::read(&f.root).expect("read root"),
        root_before,
        "the parent is NOT silently written"
    );
    assert!(
        f.app.document_state.dirty_paths.contains(&f.prj),
        "the project's sheet list changed in memory"
    );
    let data = &f.app.document_state.projects[0].data;
    assert!(data.sheets.iter().any(|s| s.filename == "renamed.snxsch"));
    assert!(!data.sheets.iter().any(|s| s.filename == "child.snxsch"));
}

#[test]
fn renaming_a_child_sheet_updates_an_open_parents_reference() {
    let mut f = fixture();
    let text = fs::read_to_string(&f.root).expect("read root");
    let parsed = signex_types::format::SnxSchematic::parse(&text).expect("parse root");
    let engine =
        signex_engine::Engine::new_with_path(parsed.sheet, Some(f.root.clone())).expect("engine");
    f.app.document_state.engines.insert(f.root.clone(), engine);
    f.app.document_state.tabs.push(signex_app::app::TabInfo {
        title: "root".to_string(),
        path: f.root.clone(),
        cached_document: None,
        dirty: false,
        project_id: None,
        kind: signex_app::app::TabKind::Schematic,
    });

    submit_rename(&mut f.app, &f.child, "renamed.snxsch");

    assert_eq!(child_refs(&f.app, &f.root), vec!["renamed.snxsch"]);
    assert!(f.app.document_state.tabs[0].dirty, "open tab shows dirty");
    assert!(f.app.document_state.dirty_paths.contains(&f.root));
}

#[test]
fn rename_cannot_drop_or_change_the_native_extension() {
    for bad in ["renamed", "renamed.txt", "renamed.snxpcb", "renamed."] {
        let mut f = fixture();
        submit_rename(&mut f.app, &f.child, bad);

        let dialog = f
            .app
            .ui_state
            .rename_dialog
            .as_ref()
            .unwrap_or_else(|| panic!("`{bad}` must keep the dialog open"));
        let msg = dialog.error.as_deref().expect("visible validation message");
        assert!(
            msg.contains(".snxsch"),
            "message names the extension: {msg}"
        );
        assert!(f.child.exists(), "`{bad}`: file must not move");
        assert!(
            !f.app.document_state.engines.contains_key(&f.root),
            "`{bad}`: no references touched"
        );
        assert!(f.app.document_state.dirty_paths.is_empty());
    }
}

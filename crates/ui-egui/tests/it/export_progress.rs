//! Export textures › exporting (texturecraft #673): LIST OF EXPORTS as a tree, the console, the
//! progress bar with Cancel Export, and the window staying open.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::{ConsoleLine, ExportJob};

/// The Export textures window writing into a scratch folder of its own for `name`
fn harness(name: &str) -> (Harness<'static, TextureCraftApp>, std::path::PathBuf) {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    let dir = std::env::temp_dir().join(format!("tc-export-progress-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    h.state_mut().ui.export.dir = dir.display().to_string();
    h.state_mut().ui.windows.export = true;
    h.run_steps(4);
    (h, dir)
}

fn click(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    h.get_all_by_label(label).last().unwrap_or_else(|| panic!("no {label}")).click();
    h.run_steps(1);
}

fn console(h: &Harness<'_, TextureCraftApp>) -> Vec<String> {
    h.state()
        .ui
        .export
        .console
        .iter()
        .map(|l| match l {
            ConsoleLine::Step(t) | ConsoleLine::Done(t) => t.clone(),
            ConsoleLine::Set(name) => format!("Exporting texture set {name}..."),
        })
        .collect()
}

#[test]
fn export_shows_progress_logs_the_steps_and_keeps_the_window_open() {
    let (mut h, dir) = harness("run");
    let sets: Vec<String> = h.state().session.doc.project.texture_sets.iter().map(|t| t.name.clone()).collect();
    click(&mut h, "Export");
    assert_eq!(h.state().ui.export.tab, 2, "Export switches to LIST OF EXPORTS");
    h.run_steps(1);
    assert!(h.query_by_label("Cancel Export").is_some());
    assert!(h.query_by_label("Cancel").is_none(), "the progress bar replaces the buttons");
    assert_eq!(console(&h).first().map(String::as_str), Some("Preparing export..."));

    for _ in 0..64 {
        if h.state().ui.export.job.is_none() {
            break;
        }
        h.run_steps(1);
    }
    h.run_steps(2);
    assert!(h.state().ui.windows.export, "the window stays open");
    assert!(h.query_by_label("Cancel").is_some() && h.query_by_label("Cancel Export").is_none());
    let lines = console(&h);
    let set = format!("Exporting texture set {}...", sets[0]);
    let start = lines.iter().position(|l| *l == set).unwrap();
    assert_eq!(&lines[start..start + 5], [set.clone(), set.clone(), "Computing textures results".into(), set.clone(), "Preparing textures writing".into()]);
    assert!(lines[start + 6].starts_with("Writing ") && lines[start + 6].ends_with(" texture on disk"), "{lines:?}");
    let written: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let finished = lines.iter().position(|l| l == "Export successfully finished").unwrap();
    assert_eq!(&lines[finished - 3..finished], ["Finalizing textures writing", "Bitmaps write finalization...", "Finalizing export..."]);
    assert_eq!(lines[finished + 1], "Successfully exported:");
    assert_eq!(lines.len() - finished - 2, written.len(), "every written file is listed");
    assert!(matches!(h.state().ui.export.console.last(), Some(ConsoleLine::Done(_))), "in green");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancel_export_stops_the_export() {
    let (mut h, dir) = harness("cancel");

    // A long export: the texture sets several times over
    let sets: Vec<usize> = (0..h.state().session.set_count()).collect();
    let pending: Vec<usize> = sets.iter().copied().cycle().take(sets.len() * 8).collect();
    let e = &mut h.state_mut().ui.export;
    e.tab = 2;
    e.job = Some(ExportJob { total: pending.len(), pending, ..ExportJob::default() });
    h.run_steps(2);
    assert!(h.query_by_label("Cancel Export").is_some());
    assert!(h.query_all_by_label_contains("%").next().is_some(), "the bar shows the percent");
    click(&mut h, "Cancel Export");
    h.run_steps(3);
    assert!(h.state().ui.export.job.is_none());
    assert!(h.state().ui.windows.export);
    let lines = console(&h);
    assert!(!lines.iter().any(|l| l == "Finalizing export..." || l == "Export successfully finished"), "the export did not finish: {lines:?}");
    assert!(h.query_by_label("Cancel").is_some(), "the buttons are back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_list_is_a_tree_per_texture_set_with_the_console_beside_it() {
    let (mut h, dir) = harness("tree");
    h.state_mut().ui.export.tab = 2;
    h.run_steps(4);
    let set = h.state().session.doc.project.texture_sets[0].name.clone();
    assert!(h.query_by_label(&set).is_some());
    assert!(h.query_all_by_label("256x256 (Texture Set size)").next().is_some());
    assert!(h.query_all_by_label("png").next().is_some() && h.query_all_by_label("16 bits").next().is_some());
    assert!(h.query_by_label("Include all").is_none() && h.query_by_label("Reset all").is_none());
    assert!(h.query_by_label("CONSOLE").is_some());

    // A row's tooltip is the full path
    let preset = h.state().ui.export.preset.clone();
    let plan = texturecraft_engine::execute(&mut h.state_mut().session, "export.plan", &serde_json::json!({"preset": preset, "ts": 0})).unwrap();
    let file = plan[0]["file"].as_str().unwrap().to_string();
    let at = h.get_by_label(&file).rect().center();
    h.hover_at(at);
    h.run_steps(30);
    assert!(h.query_by_label(&dir.join(&file).display().to_string()).is_some());

    // The console's toggle hides it
    let toggle = h.get_by_label(egui_phosphor::regular::CARET_DOUBLE_RIGHT).rect().center();
    h.hover_at(toggle);
    h.run_steps(30);
    assert!(h.query_by_label("Hide Console").is_some());
    click(&mut h, egui_phosphor::regular::CARET_DOUBLE_RIGHT);
    h.run_steps(2);
    assert!(h.state().ui.export.console_hidden && h.query_by_label("CONSOLE").is_none());
}

#[test]
fn open_output_directory_asks_the_system_to_show_the_folder() {
    let (mut h, dir) = harness("open");
    h.state_mut().ui.export.tab = 2;
    h.run_steps(4);
    h.get_by_label("Open output directory").click();
    h.run_steps(2);

    // The test harness has no window: the folder is recorded instead of shown
    assert_eq!(h.state().file_dialogs.shown_folders(), std::slice::from_ref(&dir));
    assert!(dir.is_dir(), "a missing folder is created first");
    let _ = std::fs::remove_dir_all(&dir);
}

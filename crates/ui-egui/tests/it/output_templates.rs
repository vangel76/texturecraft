//! Export textures › OUTPUT TEMPLATES (texturecraft #674): the reference's layout, names and the
//! preset list's right-click menu and F2.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use serde_json::json;
use texturecraft_ui_egui::TextureCraftApp;

/// A user template library folder removed with its owner
struct Library(std::path::PathBuf);

impl Library {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("texturecraft-output-templates-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// OUTPUT TEMPLATES with the user template "Mine" selected
fn harness(library: &Library) -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    h.state_mut().session.load_export_presets(library.0.clone());
    let base = texturecraft_engine::formats::builtin_preset("Unreal Engine (Packed)").unwrap();
    let mine = texturecraft_engine::formats::ExportPreset { name: "Mine".into(), ..base };
    h.state_mut().run("export.preset.save", json!({ "preset": mine })).unwrap();
    let ui = &mut h.state_mut().ui;
    ui.windows.export = true;
    ui.export.tab = 1;
    ui.export.preset = "Mine".into();
    h.run_steps(4);
    h
}

fn click(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    h.get_all_by_label(label).last().unwrap_or_else(|| panic!("no {label}")).click();
    h.run_steps(4);
}

/// Right-click the Presets list's `label` (listed before the selected one's name in the middle)
fn right_click(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    h.get_all_by_label(label).next().unwrap_or_else(|| panic!("no {label}")).click_secondary();
    h.run_steps(4);
}

#[test]
fn the_tab_has_the_reference_layout_and_texts() {
    let library = Library::new("layout");
    let mut h = harness(&library);
    for text in ["Presets", "Naming pattern", "Current preset:", "Output maps", "Create:", "Drag and drop maps on output's channels to setup your export."] {
        assert!(h.query_by_label(text).is_some(), "{text}");
    }
    assert!(h.get_all_by_role(egui::Role::ComboBox).any(|c| c.value().as_deref() == Some("OpenPBR")));
    let maps = h.state().session.export_preset("Mine").unwrap().maps.len();
    assert!(h.query_by_label(&format!("{maps} output maps will be exported.")).is_some());
    for create in ["Gray", "RGB", "R+G+B", "RGB+A", "R+G+B+A"] {
        assert!(h.query_by_label(create).is_some(), "{create}");
    }
    click(&mut h, "R+G+B+A");
    assert_eq!(h.state().session.export_preset("Mine").unwrap().maps.len(), maps + 1, "Create adds an output map");

    let info = h.get_by_label(egui_phosphor::regular::INFO).rect().center();
    h.hover_at(info);
    h.run_steps(30);
    assert!(h.query_by_label("Select the naming pattern used to display the input maps on this page.").is_some());
    assert!(h.query_by_label("Some input maps (channels) may be named differently depending on the shader used as a Texture Set.").is_some());
}

#[test]
fn the_presets_list_holds_the_sorted_templates_without_the_first_group() {
    let library = Library::new("list");
    let h = harness(&library);
    assert!(h.query_by_label("glTF PBR Metal Roughness").is_none());
    assert!(h.query_by_label("Document channels + Normal + AO (No Alpha)").is_none());
    let names = ["Blender (Principled BSDF)", "Mine", "Unreal Engine (Packed)"];
    let tops: Vec<f32> = names.iter().map(|n| h.get_all_by_label(n).next().unwrap().rect().top()).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "sorted by name: {tops:?}");
}

#[test]
fn new_and_duplicate_use_the_reference_names() {
    let library = Library::new("names");
    let mut h = harness(&library);
    click(&mut h, "New template");
    assert_eq!(h.state().ui.export.preset, "new_export_preset");
    right_click(&mut h, "Mine");
    click(&mut h, "Duplicate");
    assert_eq!(h.state().ui.export.preset, "Mine_copy");
    assert!(h.state().session.export_preset("Mine_copy").is_some());
}

#[test]
fn the_right_click_menu_renames_and_removes_user_templates_only() {
    let library = Library::new("menu");
    let mut h = harness(&library);
    right_click(&mut h, "Mine");
    for item in ["Rename", "Duplicate", "Remove"] {
        assert!(!h.get_by_label(item).accesskit_node().is_disabled(), "{item}");
    }
    click(&mut h, "Rename");
    assert_eq!(h.state().ui.export.renaming.as_deref(), Some("Mine"));
    h.key_press(egui::Key::Escape);
    h.run_steps(4);

    right_click(&mut h, "Mine");
    click(&mut h, "Remove");
    assert!(matches!(&h.state().ui.export.ask, Some(texturecraft_ui_egui::export_templates::Ask::Delete(n)) if n == "Mine"));
    h.state_mut().ui.export.ask = None;
    h.run_steps(2);

    right_click(&mut h, "Unreal Engine (Packed)");
    assert!(h.get_by_label("Rename").accesskit_node().is_disabled(), "built-in templates are read-only");
    assert!(h.get_by_label("Remove").accesskit_node().is_disabled());
    assert!(!h.get_by_label("Duplicate").accesskit_node().is_disabled());
}

#[test]
fn f2_renames_the_selected_template_and_leaves_the_views_alone() {
    let library = Library::new("f2");
    let mut h = harness(&library);
    let layout = h.state().ui.layout;
    h.key_press(egui::Key::F2);
    h.run_steps(4);
    assert_eq!(h.state().ui.export.renaming.as_deref(), Some("Mine"));
    assert_eq!(h.state().ui.layout, layout, "F2 does not switch to the 3D view here");
}

//! Export textures › a texture set's page (texturecraft #669): each set can override the output
//! template, file type, size and padding of the global settings.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::SetExport;

/// The Export textures window open on the first texture set's page
fn harness() -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    h.state_mut().ui.windows.export = true;
    h.run_steps(4);
    let name = h.state().session.doc.project.texture_sets[0].name.clone();
    h.get_all_by_label(&name).last().unwrap().click();
    h.run_steps(4);
    h
}

fn click(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    h.get_all_by_label(label).last().unwrap_or_else(|| panic!("no {label}")).click();
    h.run_steps(4);
}

/// Open the General Export Parameters menu
fn page_menu(h: &mut Harness<'_, TextureCraftApp>) {
    h.get_all_by_label(egui_phosphor::regular::LIST_CHECKS).nth(1).unwrap().click();
    h.run_steps(4);
}

/// The `row`th "Enable override" button (Output template, File type, Size, Padding)
fn toggle(h: &mut Harness<'_, TextureCraftApp>, row: usize) {
    h.get_all_by_label("Enable override").nth(row).unwrap().click();
    h.run_steps(4);
}

fn overrides(h: &Harness<'_, TextureCraftApp>) -> SetExport {
    h.state().ui.export.sets.get(&0).cloned().unwrap_or_default()
}

/// A scratch folder of its own for `name`
fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tc-export-overrides-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Width of a PNG file from its header
fn png_width(path: &std::path::Path) -> u32 {
    let bytes = std::fs::read(path).unwrap();
    u32::from_be_bytes(bytes[16..20].try_into().unwrap())
}

#[test]
fn a_texture_set_page_shows_what_it_inherits() {
    let h = harness();
    assert_eq!(h.state().ui.export.page, Some(0));
    assert!(h.get_all_by_label("Enable override").count() >= 4, "four rows, then the output maps");
    let own = h.state().session.doc.project.texture_sets[0].resolution;
    for text in ["Document channels + Normal + AO (No Alpha)", "Based on output template", &format!("{own}x{own} (Texture Set size)"), "Dilation infinite"] {
        assert!(h.query_by_label(text).is_some(), "{text}");
    }
    assert!(h.query_by_label("*").is_none(), "no override yet");
}

#[test]
fn an_override_turns_the_row_into_a_list_and_marks_the_set() {
    let mut h = harness();
    let own = h.state().session.doc.project.texture_sets[0].resolution;
    toggle(&mut h, 2);
    assert_eq!(overrides(&h).size, Some(own), "the override starts at the inherited size");
    let star = h.get_by_label("*").rect().center();
    h.hover_at(star);
    h.run_steps(30);
    assert!(h.query_by_label("Override(s) active").is_some());

    let label = format!("{own}x{own} (Texture Set size)");
    h.get_all_by_role(egui::Role::ComboBox).find(|c| c.value().as_deref() == Some(label.as_str())).unwrap().click();
    h.run_steps(4);
    for size in ["128x128", "8192x8192"] {
        assert!(h.query_by_label(size).is_some(), "{size}");
    }
    click(&mut h, "128x128");
    assert_eq!(overrides(&h).size, Some(128));

    toggle(&mut h, 2);
    assert!(!h.state().ui.export.sets.contains_key(&0), "switching the only override off clears the mark");
}

#[test]
fn the_header_menu_switches_every_override() {
    let mut h = harness();
    // The set list has the same menu button; the page header's is the second
    page_menu(&mut h);
    click(&mut h, "Enable override for all items");
    let o = overrides(&h);
    assert!(o.preset.is_some() && o.file_type.is_some() && o.size.is_some() && o.padding.is_some(), "{o:?}");
    assert_eq!(o.file_type.unwrap().format, "png");
    page_menu(&mut h);
    click(&mut h, "Disable override for all items");
    assert!(h.state().ui.export.sets.is_empty());
}

#[test]
fn a_set_cannot_override_to_the_templates_the_reference_leaves_out() {
    let mut h = harness();
    toggle(&mut h, 0);
    h.get_all_by_role(egui::Role::ComboBox).find(|c| c.value().as_deref() == Some("Document channels + Normal + AO (No Alpha)")).unwrap().click();
    h.run_steps(4);
    assert!(h.query_by_label("2D View").is_some());
    assert!(h.query_by_label("glTF PBR Metal Roughness").is_none());
    assert!(h.query_by_label("Sketchfab").is_none());
}

#[test]
fn export_writes_each_set_with_its_overrides_and_cancel_drops_them() {
    let mut h = harness();
    let dir = folder("size");
    h.state_mut().ui.export.dir = dir.display().to_string();
    h.state_mut().ui.export.sets.insert(0, SetExport { size: Some(128), ..SetExport::default() });
    h.run_steps(3);
    click(&mut h, "Export");

    // The export writes a texture set per frame
    for _ in 0..64 {
        if h.state().ui.export.job.is_none() {
            break;
        }
        h.run_steps(1);
    }
    h.run_steps(2);
    let sets: Vec<String> = h.state().session.doc.project.texture_sets.iter().map(|t| format!("_{}_", t.name)).collect();
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
    let of = |set: &str| files.iter().filter(|f| f.file_name().unwrap().to_string_lossy().contains(set)).collect::<Vec<_>>();
    assert!(!of(&sets[0]).is_empty());
    for f in of(&sets[0]) {
        assert_eq!(png_width(f), 128, "{}", f.display());
    }

    // The other sets keep their own size
    for set in &sets[1..] {
        for f in of(set) {
            assert_eq!(png_width(f), 256, "{}", f.display());
        }
    }
    let _ = std::fs::remove_dir_all(&dir);

    // The window stays open after an export; Save settings keeps the overrides
    click(&mut h, "Save settings");
    h.state_mut().ui.windows.export = true;
    h.run_steps(4);
    h.state_mut().ui.export.sets.insert(0, SetExport { size: Some(512), ..SetExport::default() });
    h.run_steps(3);
    click(&mut h, "Cancel");
    assert_eq!(overrides(&h).size, Some(128), "Cancel goes back to the overrides the window opened with");
}

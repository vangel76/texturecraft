//! File ▸ Export textures: the SETTINGS, OUTPUT TEMPLATES and LIST OF EXPORTS tabs. The window
//! edits `app.ui.export` live; Cancel (and closing the window) goes back to the settings it
//! opened with, Save settings keeps them.

use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::formats::{ExportPreset, FileFormat};

use crate::TextureCraftApp;
#[cfg(not(target_arch = "wasm32"))]
use crate::file_dialogs::{Bound, FileDialog};
use crate::i18n::{tr, trf};
use crate::state::{ExportUi, FileType, SIZE_OF_LARGEST_SET, SetExport};
use crate::theme::{Tokens, caption};
use crate::unavailable::NOT_AVAILABLE;
use crate::widgets::{icon_button, primary_button};

/// Height of the window's tab body, above its buttons
const BODY_HEIGHT: f32 = 440.0;
/// Width of the texture set list
const LIST_WIDTH: f32 = 180.0;
const ICON: f32 = 20.0;
/// Width of the value column of the parameter rows
const VALUE_WIDTH: f32 = 360.0;

/// The output sizes the Size list offers, in texels per side
pub(crate) const SIZES: [u32; 7] = [128, 256, 512, 1024, 2048, 4096, 8192];

/// Templates the reference lists above the separator, in its order (one more carries a vendor
/// name and is left out). TextureCraft has some of them; the others are listed greyed.
const FIRST_GROUP: [&str; 7] = [
    "2D View",
    "Document channels + Normal + AO (No Alpha)",
    "Document channels + Normal + AO (With Alpha)",
    "Sketchfab",
    "USDz (Apple AR)",
    "glTF PBR Metal Roughness",
    "glTF PBR Metal Roughness + Displacement texture (experimental)",
];

/// First-group templates a texture set cannot override to (the reference leaves them out there)
const NOT_PER_SET: [&str; 4] = ["Sketchfab", "USDz (Apple AR)", "glTF PBR Metal Roughness", "glTF PBR Metal Roughness + Displacement texture (experimental)"];

/// The file types of the File type list, in the reference order
pub(crate) const FILE_TYPES: [FileFormat; 8] =
    [FileFormat::Bmp, FileFormat::Jpeg, FileFormat::Png, FileFormat::Tga, FileFormat::Tiff, FileFormat::Hdr, FileFormat::Exr, FileFormat::Webp];

/// The padding types, in the reference order: (`file.exportTextures` value, label)
pub(crate) const PADDINGS: [(&str, &str); 4] = [
    ("none", "No padding (passthrough)"),
    ("infinite", "Dilation infinite"),
    ("dilation", "Dilation + transparent"),
    ("dilationDefault", "Dilation + default background color"),
];

pub(crate) fn padding_label(padding: &str) -> &'static str {
    PADDINGS.iter().find(|(v, _)| *v == padding).map_or("Dilation infinite", |(_, l)| l)
}

/// Paddings with a width: the others ignore the dilation
pub(crate) fn has_width(padding: &str) -> bool {
    matches!(padding, "dilation" | "dilationDefault")
}

/// The name a stored file type is listed under ("" when the template decides)
fn file_type_name(format: &str) -> String {
    FileFormat::from_extension(format).map_or_else(|| format.to_string(), |f| f.name().to_string())
}

/// The template list: the first group (name, whether TextureCraft has it), then the others
/// (built-in and user templates) sorted by name.
pub(crate) fn template_groups(app: &TextureCraftApp, builtins: &[ExportPreset]) -> (Vec<(&'static str, bool)>, Vec<String>) {
    let first = FIRST_GROUP.iter().map(|name| (*name, builtins.iter().any(|p| p.name == *name))).collect();
    let mut rest: Vec<String> = builtins.iter().map(|p| p.name.clone()).filter(|n| !FIRST_GROUP.contains(&n.as_str())).collect();
    rest.extend(app.session.export_presets.list.iter().map(|p| p.name.clone()));
    rest.sort();
    rest.dedup();
    (first, rest)
}

/// A template dropdown over `first` and `rest`; true when `current` changed
fn template_combo(ui: &mut Ui, id: &str, current: &mut String, first: &[(&str, bool)], rest: &[String]) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).width(VALUE_WIDTH).selected_text(current.clone()).show_ui(ui, |ui| {
        for (name, have) in first {
            if *have {
                changed |= ui.selectable_value(current, (*name).to_string(), *name).on_hover_text(*name).changed();
            } else {
                ui.add_enabled(false, egui::Button::selectable(false, *name)).on_disabled_hover_text(tr(NOT_AVAILABLE));
            }
        }
        ui.separator().on_hover_text(tr("This preset is not available for the current project"));
        for name in rest {
            changed |= ui.selectable_value(current, name.clone(), name).on_hover_text(name).changed();
        }
    });
    changed
}

/// The number shown for an output size
fn size_label(size: u32) -> String {
    match size {
        0 => tr("Based on each Texture Set's size").to_string(),
        SIZE_OF_LARGEST_SET => tr("Maximum size for all Texture Sets").to_string(),
        n => n.to_string(),
    }
}

/// A size as a texture set page shows it: "1024x1024", marked when it is the set's own
fn set_size_label(size: u32, own: u32) -> String {
    if size == own { trf("{n}x{n} (Texture Set size)", &[("n", &size)]) } else { format!("{size}x{size}") }
}

/// The bit depth a file type is written with, as listed
fn bits_label(bit_depth: u8) -> String {
    trf("{n} bits", &[("n", &bit_depth)])
}

/// The largest texture set's resolution
fn largest_set(app: &TextureCraftApp) -> u32 {
    app.session.doc.project.texture_sets.iter().map(|t| t.block_resolution()).max().unwrap_or(1024)
}

/// Texture sets ticked in the export dialog.
fn export_selection(app: &TextureCraftApp) -> Vec<usize> {
    (0..app.session.set_count()).filter(|i| !app.ui.export.skip.contains(i)).collect()
}

/// Export the selected texture sets; false when a command failed (its error is already shown).
#[cfg(not(target_arch = "wasm32"))]
fn export_sets(app: &mut TextureCraftApp) -> bool {
    let mut written = 0usize;
    for ts in export_selection(app) {
        let mut p = set_params(app, ts);
        p["dir"] = json!(app.ui.export.dir);
        match app.run("file.exportTextures", p) {
            Some(r) => written += r.get("files").and_then(|f| f.as_array()).map(Vec::len).unwrap_or(0),
            None => return false,
        }
    }
    app.toast(trf("Exported {n} textures to {folder}", &[("n", &written), ("folder", &app.ui.export.dir)]), false);
    true
}

/// In the browser the textures download (one zip for several files).
#[cfg(target_arch = "wasm32")]
fn export_sets(app: &mut TextureCraftApp) -> bool {
    let jobs: Vec<(usize, serde_json::Value)> = export_selection(app).into_iter().map(|ts| (ts, set_params(app, ts))).collect();
    crate::web_files::export_textures(app, &jobs);
    true
}

/// The `file.exportTextures` / `export.plan` parameters of the window's global settings
pub(crate) fn export_params(app: &TextureCraftApp) -> serde_json::Value {
    let e = &app.ui.export;
    let mut p = json!({"preset": e.preset, "padding": e.padding, "dilation": e.dilation});
    if !e.format.is_empty() {
        p["format"] = json!(e.format);
    }
    if e.bit_depth > 0 {
        p["bitDepth"] = json!(e.bit_depth);
    }
    match e.size {
        0 => {}
        SIZE_OF_LARGEST_SET => p["size"] = json!(largest_set(app)),
        n => p["size"] = json!(n),
    }
    if !e.normal_format.is_empty() {
        p["normalFormat"] = json!(e.normal_format);
    }
    if !e.color_space.is_empty() {
        p["colorSpace"] = json!(e.color_space);
    }
    p
}

/// The parameters exporting texture set `ts`: the global settings with the set's overrides
pub(crate) fn set_params(app: &TextureCraftApp, ts: usize) -> serde_json::Value {
    let mut p = export_params(app);
    p["ts"] = json!(ts);
    let Some(o) = app.ui.export.sets.get(&ts) else { return p };
    if let Some(preset) = &o.preset {
        p["preset"] = json!(preset);
    }
    if let Some(t) = &o.file_type {
        p["format"] = json!(t.format);
        p["bitDepth"] = json!(t.bit_depth);
    }
    if let Some(size) = o.size {
        p["size"] = json!(size);
    }
    if let Some((padding, dilation)) = &o.padding {
        p["padding"] = json!(padding);
        p["dilation"] = json!(dilation);
    }
    p
}

/// The files the export writes for the ticked texture sets, set by set (an error stops the list)
fn planned_files(app: &mut TextureCraftApp) -> Result<Vec<serde_json::Value>, String> {
    let mut files = Vec::new();
    for ts in export_selection(app) {
        let params = set_params(app, ts);
        let plan = texturecraft_engine::execute(&mut app.session, "export.plan", &params).map_err(|e| e.to_string())?;
        files.extend(plan.as_array().into_iter().flatten().cloned());
    }
    Ok(files)
}

/// A header with a menu or tool at its right end
fn header(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(caption(title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

/// The menu button of a list header
fn list_menu(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.menu_button(RichText::new(ic::LIST_CHECKS).size(15.0), add);
}

/// Check all, Uncheck all, Invert all over the texture sets
fn set_checks_menu(app: &mut TextureCraftApp, ui: &mut Ui) {
    let sets = app.session.set_count();
    if ui.button(tr("Check all")).clicked() {
        app.ui.export.skip.clear();
    }
    if ui.button(tr("Uncheck all")).clicked() {
        app.ui.export.skip = (0..sets).collect();
    }
    if ui.button(tr("Invert all")).clicked() {
        let skipped = std::mem::take(&mut app.ui.export.skip);
        app.ui.export.skip = (0..sets).filter(|i| !skipped.contains(i)).collect();
    }
    ui.separator();
}

/// Output directory: the path as a button that opens the folder picker, and a reset
#[cfg(not(target_arch = "wasm32"))]
fn output_directory(app: &mut TextureCraftApp, ui: &mut Ui) {
    ui.label(tr("Output directory"));
    ui.horizontal(|ui| {
        let dir = app.ui.export.dir.clone();
        let button = egui::Button::new(dir.as_str()).truncate();
        if ui.add_sized(vec2(VALUE_WIDTH - 24.0, 20.0), button).on_hover_text(dir.as_str()).clicked() {
            app.ask_file(FileDialog::folder().directory(dir), Bound::App, |app, paths| {
                if let Some(d) = paths.first() {
                    app.ui.export.dir = d.display().to_string();
                }
            });
        }
        if ui.small_button("R").on_hover_text(tr("Reset to default")).clicked() {
            app.ui.export.dir = ExportUi::default().dir;
        }
    });
    ui.end_row();
}

/// The dilation width: a button showing it, with a slider in its menu (only for paddings that
/// have a width)
fn dilation_button(ui: &mut Ui, padding: &str, dilation: &mut u32) {
    let enabled = has_width(padding);
    // The menu stays open while the slider is used
    let config = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let r = ui.add_enabled_ui(enabled, |ui| {
        egui::containers::menu::MenuButton::new(dilation.to_string()).config(config).ui(ui, |ui| {
            ui.label(tr("Dilation (pixels)"));
            ui.add(egui::Slider::new(dilation, 0..=256).step_by(1.0));
        })
    });
    if enabled {
        r.response.on_hover_text(tr("Dilation in pixels"));
    } else {
        r.response.on_disabled_hover_text(tr("Dilation cannot be applied to selected padding type"));
    }
}

/// A greyed reference checkbox with its tooltip and the "not available" hint
fn unavailable_checkbox(ui: &mut Ui, label: &str, tooltip: &str) {
    let mut off = false;
    ui.add_enabled(false, egui::Checkbox::new(&mut off, label)).on_disabled_hover_ui(|ui| {
        ui.label(tooltip);
        ui.label(tr(NOT_AVAILABLE));
    });
}

/// The Global settings page
fn global_page(app: &mut TextureCraftApp, ui: &mut Ui, builtins: &[ExportPreset]) {
    let mut reset = false;
    header(ui, tr("General Export Parameters"), |ui| {
        reset = icon_button(ui, ic::ARROW_COUNTER_CLOCKWISE, tr("Resets all settings to default values"), false, ICON).clicked();
    });
    if reset {
        let d = ExportUi::default();
        let e = &mut app.ui.export;
        (e.dir, e.format, e.bit_depth, e.size, e.padding, e.dilation, e.normal_format, e.color_space) =
            (d.dir, d.format, d.bit_depth, d.size, d.padding, d.dilation, d.normal_format, d.color_space);
        crate::export_templates::select(app, d.preset);
    }
    let (first, rest) = template_groups(app, builtins);
    egui::Grid::new("exp_grid").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        // The browser downloads the files instead
        #[cfg(not(target_arch = "wasm32"))]
        output_directory(app, ui);
        ui.label(tr("Output template"));
        let mut picked = app.ui.export.preset.clone();
        if template_combo(ui, "exp_preset", &mut picked, &first, &rest) {
            crate::export_templates::select(app, picked);
        }
        ui.end_row();
        ui.label(tr("File type"));
        ui.horizontal(|ui| {
            let e = &mut app.ui.export;
            let label = if e.format.is_empty() { tr("Based on output template").to_string() } else { file_type_name(&e.format) };
            egui::ComboBox::from_id_salt("exp_fmt").selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut e.format, String::new(), tr("Based on output template"));
                ui.separator();
                for f in FILE_TYPES {
                    ui.selectable_value(&mut e.format, f.name().to_string(), f.name());
                }
            });
            let bd = if e.bit_depth == 0 { tr("Default").to_string() } else { trf("{n} bits", &[("n", &e.bit_depth)]) };
            egui::ComboBox::from_id_salt("exp_bits").selected_text(bd).show_ui(ui, |ui| {
                for (v, l) in [
                    (0u8, tr("Default").to_string()),
                    (8, trf("{n} bits", &[("n", &8)])),
                    (16, trf("{n} bits", &[("n", &16)])),
                    (32, tr("32 bits (float)").to_string()),
                ] {
                    ui.selectable_value(&mut e.bit_depth, v, l);
                }
            });
        });
        ui.end_row();
        ui.label(tr("Size"));
        let e = &mut app.ui.export;
        egui::ComboBox::from_id_salt("exp_size").width(VALUE_WIDTH).selected_text(size_label(e.size)).show_ui(ui, |ui| {
            ui.selectable_value(&mut e.size, 0, size_label(0));
            ui.separator();
            ui.selectable_value(&mut e.size, SIZE_OF_LARGEST_SET, size_label(SIZE_OF_LARGEST_SET));
            for r in SIZES {
                ui.selectable_value(&mut e.size, r, size_label(r));
            }
        });
        ui.end_row();
        ui.label(tr("Padding"));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("exp_pad").width(260.0).selected_text(tr(padding_label(&e.padding))).show_ui(ui, |ui| {
                for (v, l) in PADDINGS {
                    ui.selectable_value(&mut e.padding, v.to_string(), tr(l));
                }
            });
            dilation_button(ui, &e.padding, &mut e.dilation);
        });
        ui.end_row();
    });

    // The reference's two switches; TextureCraft's own normal map and colour space overrides follow them
    unavailable_checkbox(
        ui,
        tr("Export USD asset"),
        tr("Export USD files hierarchy with material and textures. References the original mesh if possible, exports the mesh otherwise"),
    );
    unavailable_checkbox(ui, tr("Export shaders parameters"), tr("Exports a JSON file with the settings for all shader instances"));
    ui.add_space(8.0);
    egui::Grid::new("exp_grid_more").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        let e = &mut app.ui.export;
        ui.label(tr("Normal map format"));
        let nf = match e.normal_format.as_str() {
            "openGl" => "OpenGL",
            "directX" => "DirectX",
            _ => tr("From template / project"),
        };
        egui::ComboBox::from_id_salt("exp_nf").selected_text(nf).show_ui(ui, |ui| {
            for (v, l) in [("", tr("From template / project")), ("openGl", "OpenGL"), ("directX", "DirectX")] {
                ui.selectable_value(&mut e.normal_format, v.to_string(), l);
            }
        });
        ui.end_row();
        ui.label(tr("Color space"));
        let shown = if e.color_space.is_empty() { tr("Default") } else { e.color_space.as_str() };
        egui::ComboBox::from_id_salt("exp_cs").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut e.color_space, String::new(), tr("Default"));
            for name in ["sRGB", "Linear Rec.709", "ACEScg", "ACES2065-1"] {
                ui.selectable_value(&mut e.color_space, name.to_string(), name);
            }
        });
        ui.end_row();
    });
}

/// The left column: Global settings, then a row per texture set with its export tick; a click
/// on a row shows the set's page, "*" marks a set with overrides
fn set_list(app: &mut TextureCraftApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| list_menu(ui, |ui| set_checks_menu(app, ui)));
    });
    if ui.selectable_label(app.ui.export.page.is_none(), RichText::new(tr("Global settings")).strong()).clicked() {
        app.ui.export.page = None;
    }
    for i in 0..app.session.set_count() {
        let name = app.session.doc.project.texture_sets.get(i).map(|t| t.name.clone()).unwrap_or_default();
        let mut on = !app.ui.export.skip.contains(&i);
        ui.horizontal(|ui| {
            if ui.checkbox(&mut on, "").changed() {
                if on {
                    app.ui.export.skip.retain(|x| *x != i);
                } else {
                    app.ui.export.skip.push(i);
                }
            }
            if ui.selectable_label(app.ui.export.page == Some(i), name).clicked() {
                app.ui.export.page = Some(i);
            }
            if app.ui.export.sets.get(&i).is_some_and(SetExport::any) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("*").on_hover_text(tr("Override(s) active"));
                });
            }
        });
    }
}

/// A texture set page row: the label, the override toggle, then the inherited value as text or,
/// overridden, `edit`; true when the toggle was clicked
fn override_row(ui: &mut Ui, label: &str, on: bool, inherited: &str, edit: impl FnOnce(&mut Ui)) -> bool {
    ui.label(label);
    let mut toggled = false;
    ui.horizontal(|ui| {
        toggled = icon_button(ui, ic::PENCIL_SIMPLE, tr("Enable override"), on, ICON).clicked();
        if on {
            edit(ui);
        } else {
            ui.label(inherited);
        }
    });
    ui.end_row();
    toggled
}

/// What texture set `ts` inherits from the global settings, as override starting points
struct Inherited {
    preset: String,
    file_type: FileType,
    size: u32,
    padding: (String, u32),
}

fn inherited(app: &TextureCraftApp, ts: usize) -> Inherited {
    let e = &app.ui.export;
    let own = app.session.doc.project.texture_sets.get(ts).map_or(1024, |t| t.resolution);
    let format = if e.format.is_empty() { FileFormat::Png.name().to_string() } else { file_type_name(&e.format) };
    let bit_depth = FileFormat::from_extension(&format).map_or(8, |f| f.depth(if e.bit_depth == 0 { 8 } else { e.bit_depth }));
    let size = match e.size {
        0 => own,
        SIZE_OF_LARGEST_SET => largest_set(app),
        n => n,
    };
    Inherited { preset: e.preset.clone(), file_type: FileType { format, bit_depth }, size, padding: (e.padding.clone(), e.dilation) }
}

/// Switch one override of `o` on (from what the set inherits) or off
fn toggle(o: &mut SetExport, row: usize, from: &Inherited) {
    match row {
        0 => o.preset = if o.preset.is_some() { None } else { Some(from.preset.clone()) },
        1 => o.file_type = if o.file_type.is_some() { None } else { Some(from.file_type.clone()) },
        2 => o.size = if o.size.is_some() { None } else { Some(from.size) },
        _ => o.padding = if o.padding.is_some() { None } else { Some(from.padding.clone()) },
    }
}

/// A file type dropdown (without "Based on output template") and its bit depth
fn file_type_combo(ui: &mut Ui, id: (&str, usize, usize), t: &mut FileType) {
    egui::ComboBox::from_id_salt(("type", id)).width(90.0).selected_text(t.format.clone()).show_ui(ui, |ui| {
        for f in FILE_TYPES {
            if ui.selectable_label(t.format == f.name(), f.name()).clicked() {
                t.format = f.name().to_string();
                t.bit_depth = f.depth(t.bit_depth);
            }
        }
    });
    let depths = FileFormat::from_extension(&t.format).map_or(&[8u8][..], FileFormat::bit_depths);
    egui::ComboBox::from_id_salt(("bits", id)).width(90.0).selected_text(bits_label(t.bit_depth)).show_ui(ui, |ui| {
        for &d in depths {
            ui.selectable_value(&mut t.bit_depth, d, bits_label(d));
        }
    });
}

/// A texture set's page: its overrides of the global settings
fn set_page(app: &mut TextureCraftApp, ui: &mut Ui, ts: usize, builtins: &[ExportPreset]) {
    let from = inherited(app, ts);
    let own = app.session.doc.project.texture_sets.get(ts).map_or(1024, |t| t.resolution);
    let (first, rest) = template_groups(app, builtins);
    let first: Vec<(&str, bool)> = first.into_iter().filter(|(n, _)| !NOT_PER_SET.contains(n)).collect();
    let global = &app.ui.export;
    let inherited_file_type = if global.format.is_empty() {
        tr("Based on output template").to_string()
    } else if global.bit_depth == 0 {
        file_type_name(&global.format)
    } else {
        format!("{}   {}", file_type_name(&global.format), bits_label(global.bit_depth))
    };
    let inherited_size = match global.size {
        0 => set_size_label(own, own),
        _ => set_size_label(from.size, own),
    };
    let inherited_padding = tr(padding_label(&global.padding)).to_string();

    let o = app.ui.export.sets.entry(ts).or_default();
    let mut all = None;
    header(ui, tr("General Export Parameters"), |ui| {
        list_menu(ui, |ui| {
            if ui.button(tr("Enable override for all items")).clicked() {
                all = Some(true);
            }
            if ui.button(tr("Disable override for all items")).clicked() {
                all = Some(false);
            }
            ui.separator();
        });
    });
    let mut toggled = None;
    egui::Grid::new(("exp_set_grid", ts)).num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        let on = o.preset.is_some();
        if override_row(ui, tr("Output template"), on, &from.preset, |ui| {
            if let Some(p) = &mut o.preset {
                template_combo(ui, "exp_set_preset", p, &first, &rest);
            }
        }) {
            toggled = Some(0);
        }
        let on = o.file_type.is_some();
        if override_row(ui, tr("File type"), on, &inherited_file_type, |ui| {
            if let Some(t) = &mut o.file_type {
                file_type_combo(ui, ("exp_set", ts, usize::MAX), t);
            }
        }) {
            toggled = Some(1);
        }
        let on = o.size.is_some();
        if override_row(ui, tr("Size"), on, &inherited_size, |ui| {
            if let Some(size) = &mut o.size {
                egui::ComboBox::from_id_salt("exp_set_size").width(VALUE_WIDTH).selected_text(set_size_label(*size, own)).show_ui(ui, |ui| {
                    for r in SIZES {
                        ui.selectable_value(size, r, set_size_label(r, own));
                    }
                });
            }
        }) {
            toggled = Some(2);
        }
        let on = o.padding.is_some();
        if override_row(ui, tr("Padding"), on, &inherited_padding, |ui| {
            if let Some((padding, dilation)) = &mut o.padding {
                egui::ComboBox::from_id_salt("exp_set_pad").width(260.0).selected_text(tr(padding_label(padding))).show_ui(ui, |ui| {
                    for (v, l) in PADDINGS {
                        ui.selectable_value(padding, v.to_string(), tr(l));
                    }
                });
                dilation_button(ui, padding, dilation);
            }
        }) {
            toggled = Some(3);
        }
    });
    if let Some(row) = toggled {
        toggle(o, row, &from);
    }
    match all {
        Some(true) => {
            for row in 0..4 {
                let on = [o.preset.is_some(), o.file_type.is_some(), o.size.is_some(), o.padding.is_some()];
                if !on.get(row).copied().unwrap_or(true) {
                    toggle(o, row, &from);
                }
            }
        }
        Some(false) => *o = SetExport::default(),
        None => {}
    }
    if !o.any() {
        app.ui.export.sets.remove(&ts);
    }
}

/// The LIST OF EXPORTS tab: the files the export writes
fn list_tab(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    match planned_files(app) {
        Ok(files) => {
            egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                for f in files {
                    ui.label(RichText::new(f.get("file").and_then(|x| x.as_str()).unwrap_or_default()).monospace());
                }
            });
        }
        Err(e) => {
            ui.label(RichText::new(e).color(tk.danger));
        }
    }
}

/// How many files the export writes for the ticked texture sets
fn selected_texture_count(app: &mut TextureCraftApp) -> usize {
    planned_files(app).map_or(0, |files| files.len())
}

/// Go back to the settings the window opened with
fn cancel(app: &mut TextureCraftApp) {
    if let Some(before) = app.ui.export.opened_with.take() {
        let mut before = *before;
        before.draft = None;
        before.renaming = None;
        before.ask = None;

        // A template deleted since keeps the default instead
        if app.session.export_preset(&before.preset).is_none() {
            before.preset = ExportUi::default().preset;
        }
        app.ui.export = before;
    }
    app.ui.windows.export = false;
}

pub fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    if !app.ui.windows.export {
        // The window opens on SETTINGS again
        let e = &mut app.ui.export;
        e.ask = None;
        e.opened_with = None;
        e.tab = 0;
        e.page = None;
        return;
    }

    // Settings saved under the default template's old name
    if let Some(b) = texturecraft_engine::formats::builtin_preset(&app.ui.export.preset)
        && b.name != app.ui.export.preset
    {
        app.ui.export.preset = b.name;
    }
    if app.ui.export.opened_with.is_none() {
        let mut now = app.ui.export.clone();
        now.draft = None;
        app.ui.export.opened_with = Some(Box::new(now));
    }

    let mut open = true;
    let mut do_export = false;
    let mut canceled = false;
    let presets = texturecraft_engine::formats::builtin_presets();
    crate::windows::dialog("export_textures", tr("Export textures"), &mut open).fixed_size(vec2(900.0, 520.0)).show(ctx, |ui| {
        // Nothing to export without a project
        if !app.session.project_open {
            ui.label(tr("No project is open"));
            return;
        }

        ui.horizontal(|ui| {
            for (i, t) in [tr("Settings"), tr("Output templates"), tr("List of exports")].iter().enumerate() {
                if ui.selectable_label(app.ui.export.tab == i as u8, caption(&t.to_uppercase())).clicked() {
                    app.ui.export.tab = i as u8;
                }
            }

            // Tick every texture set again
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Include all")).on_hover_text(tr("Includes all Texture Sets, UV Tiles and output maps.")).clicked() {
                    app.ui.export.skip.clear();
                }
            });
        });
        ui.separator();
        ui.allocate_ui_with_layout(vec2(ui.available_width(), BODY_HEIGHT), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.set_height(BODY_HEIGHT);
            if app.ui.export.tab == 1 {
                crate::export_templates::tab(app, ui, &presets);
                return;
            }

            ui.vertical(|ui| {
                ui.set_width(LIST_WIDTH);
                set_list(app, ui);
            });
            ui.separator();
            ui.vertical(|ui| match (app.ui.export.tab, app.ui.export.page) {
                (0, Some(ts)) if ts < app.session.set_count() => set_page(app, ui, ts, &presets),
                (0, _) => global_page(app, ui, &presets),
                _ => list_tab(app, ui),
            });
        });
        ui.add_space(8.0);
        ui.separator();
        let selected = selected_texture_count(app);
        ui.horizontal(|ui| {
            if ui.button(tr("Reset all")).on_hover_text(tr("Resets all settings to their default values, including all textures.")).clicked() {
                let opened_with = app.ui.export.opened_with.take();
                app.ui.export = ExportUi { opened_with, ..ExportUi::default() };
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button(ui, tr("Export"))).on_hover_text(tr("Exports all selected textures and additional files")).clicked() {
                    do_export = true;
                }

                // The settings are kept with the interface, so saving them closes the window without exporting
                if ui.button(tr("Save settings")).on_hover_text(tr("Closes the export window and saves changes")).clicked() {
                    app.ui.windows.export = false;
                }
                if ui.button(tr("Cancel")).on_hover_text(tr("Closes the export window and undoes all recent changes")).clicked() {
                    canceled = true;
                }
                ui.label(trf("{n} textures selected for export", &[("n", &selected)]));
            });
        });
    });
    crate::export_templates::ask(app, ctx);
    if do_export && export_sets(app) {
        app.ui.windows.export = false;
    }

    // Closing the window is Cancel
    if canceled || !open {
        cancel(app);
    }
}

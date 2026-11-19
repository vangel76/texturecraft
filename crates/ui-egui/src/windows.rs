//! Floating windows and the dockable History panel

use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as ic;
use serde_json::json;

use crate::TextureCraftApp;
#[cfg(not(target_arch = "wasm32"))]
use crate::file_dialogs::{Bound, FileDialog};
use crate::i18n::{tr, tr_history, trf};
#[cfg(not(target_arch = "wasm32"))]
use crate::state::Mode;
use crate::theme::{Tokens, caption};
use crate::widgets::primary_button;

pub fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    crate::new_project_advanced::show(app, ctx);
    about(app, ctx);
    crate::settings_window::show(app, ctx);
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::project_config::show(app, ctx);
        crate::import_resources::show(app, ctx);
    }
    crate::keymap::window(app, ctx);
    crate::windows::export(app, ctx);
    crate::mesh_export_options::show(app, ctx);
    #[cfg(target_arch = "wasm32")]
    export_mesh(app, ctx);
    crate::pbr_import::window(app, ctx);
    unsaved(app, ctx);
    reset_ui(app, ctx);
    crate::named_layouts::show(app, ctx);
}

/// Window ▸ Reset UI asks first: Escape closes the question, Enter answers Reset.
pub(crate) fn reset_ui_keys(app: &mut TextureCraftApp, input: &mut egui::InputState, nothing_focused: bool) {
    if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
        app.ui.windows.confirm_reset_ui = false;
    } else if nothing_focused && input.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
        app.ui.adopt_layout(crate::UiState::default());
        app.ui.windows.confirm_reset_ui = false;
    }
}

fn reset_ui(app: &mut TextureCraftApp, ctx: &egui::Context) {
    if !app.ui.windows.confirm_reset_ui {
        return;
    }

    let mut open = true;
    let (mut reset, mut cancel) = (false, false);
    dialog("reset_ui", tr("Reset UI"), &mut open).show(ctx, |ui| {
        ui.label(tr("Reset the panels, view layout, zoom and every other view setting to their defaults? Settings and shortcuts stay. To reset only the panels, use Reset panel layout."));
        ui.add_space(8.0);

        // One row keeps the window to its contents; a bare right-to-left layout fills the screen height
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                reset = ui.add(primary_button(ui, tr("Reset"))).clicked();
                cancel = ui.button(tr("Cancel")).clicked();
            });
        });
    });

    if reset {
        app.ui.adopt_layout(crate::UiState::default());
    }
    if reset || cancel || !open {
        app.ui.windows.confirm_reset_ui = false;
    }
}

/// Hold `then` (quitting, opening or closing a project) behind the unsaved-changes dialog when
/// the project has unsaved changes; false when it can go ahead now.
pub(crate) fn ask_to_save(app: &mut TextureCraftApp, then: crate::state::Unsaved) -> bool {
    if app.session.modified {
        app.ui.windows.unsaved = Some(then);
    }
    app.session.modified
}

/// The close decision, once per frame in `logic` (which also runs while the window is minimized):
/// a close with unsaved changes waits on the dialog, in a window brought back so it can be
/// answered. A close nothing holds open quits through the platform's own path (craft-quit).
pub(crate) fn close_request(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let closing = ctx.input(|i| i.viewport().close_requested());
    // A stroke still open at the close, or released in this frame, is a change to ask about.
    if closing
        && app.session.stroke_active()
        && let Err(error) = app.session.end_stroke()
    {
        app.toast(error.to_string(), true);
    }
    if closing && ask_to_save(app, crate::state::Unsaved::Quit) {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }
    craft_quit::on_close_request(ctx);
}

/// Quitting, opening or closing a project with unsaved changes asks whether to save them first
/// (in the browser, Save downloads the project).
fn unsaved(app: &mut TextureCraftApp, ctx: &egui::Context) {
    use crate::state::Unsaved;

    let Some(pending) = app.ui.windows.unsaved.clone() else { return };

    #[cfg(not(target_arch = "wasm32"))]
    let (waiting_for_save, saved_by_dialog) = (app.saving_for_prompt, std::mem::take(&mut app.prompt_saved));
    #[cfg(target_arch = "wasm32")]
    let (waiting_for_save, saved_by_dialog) = (false, false);

    let mut open = true;
    let mut save_first = None;
    let mut cancel = false;
    let question_args: [(&str, &dyn std::fmt::Display); 1] = [("name", &app.session.doc.project.name)];
    let question = if pending == Unsaved::Quit {
        trf("Save the changes to {name} before closing?", &question_args)
    } else {
        trf("Save the changes to {name} first?", &question_args)
    };
    dialog("unsaved", tr("Unsaved changes"), &mut open).show(ctx, |ui| {
        ui.label(question);
        ui.add_space(8.0);

        // One row keeps the window to its contents; a bare right-to-left layout fills the screen height
        ui.horizontal(|ui| {
            // The answers wait while the Save dialog is open
            ui.add_enabled_ui(!waiting_for_save, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(primary_button(ui, tr("Save"))).clicked() {
                        save_first = Some(true);
                    }
                    if ui.button(tr("Don't save")).clicked() {
                        save_first = Some(false);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        });
    });
    if !open || cancel {
        app.ui.windows.unsaved = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            app.saving_for_prompt = false;
        }
    }

    let discard_after_create = matches!(pending, Unsaved::CreateProject(_)) && save_first == Some(false) && app.session.modified;
    match save_first {
        Some(true) => {
            // An untitled project is saved from a dialog that answers on a later frame
            #[cfg(not(target_arch = "wasm32"))]
            {
                app.saving_for_prompt = app.session.path.is_none();
            }
            file_action(app, ctx, "file.save");
        }
        // Going ahead anyway: the changes are let go
        Some(false) => app.session.modified = false,
        None if saved_by_dialog => {}
        None => return,
    }

    // A canceled or failed save keeps the project and this dialog open
    if app.session.modified {
        return;
    }

    app.ui.windows.unsaved = None;
    match pending {
        Unsaved::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        Unsaved::Command(id) => crate::menus::invoke(app, ctx, &id),
        Unsaved::CreateProject(opening_id) => {
            if !crate::new_project_advanced::finish_create(app, opening_id) && discard_after_create {
                app.session.modified = true;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Unsaved::Open(path) => open_path(app, &path),
        Unsaved::Load(name, bytes) => crate::web_files::load(app, &name, std::sync::Arc::unwrap_or_clone(bytes)),
    }
}

/// A titled tool window; `id` keeps its place across UI languages.
pub(crate) fn window<'a>(id: &str, title: &str, open: &'a mut bool) -> egui::Window<'a> {
    egui::Window::new(RichText::new(title).font(crate::theme::semibold(13.0)).strong())
        .id(egui::Id::unique(id))
        .open(open)
        .resizable(true)
        .collapsible(false)
        .default_width(300.0)
}

/// A centered dialog; `id` keeps its place across UI languages.
pub(crate) fn dialog<'a>(id: &str, title: &str, open: &'a mut bool) -> egui::Window<'a> {
    egui::Window::new(title.to_owned())
        .id(egui::Id::unique(id))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
}

/// The History panel body (a dock tab)
pub(crate) fn history(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    let (done, undone) = (app.session.history_labels(), app.session.redo_labels());

    // A click on a step goes back (or forward) to it: steps to undo, or to redo when negative
    let mut jump = 0i64;

    // Bottom up: the Undo and Redo row keeps its place under a list that fills the rest of the panel
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(app.session.can_undo(), egui::Button::new(format!("{} {}", ic::ARROW_COUNTER_CLOCKWISE, tr("Undo")))).clicked() {
                app.run("edit.undo", json!({}));
            }
            if ui.add_enabled(app.session.can_redo(), egui::Button::new(format!("{} {}", ic::ARROW_CLOCKWISE, tr("Redo")))).clicked() {
                app.run("edit.redo", json!({}));
            }
        });

        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let done_count = done.len() as i64;
                let rows = std::iter::once(RichText::new(tr("Project opened")).color(tokens.text_dim))
                    .chain(done.iter().map(|label| RichText::new(tr_history(label))))
                    .chain(undone.iter().map(|label| RichText::new(trf("{action} (undone)", &[("action", &tr_history(label))])).color(tokens.text_dim)));
                for (row, text) in rows.enumerate() {
                    // Each row is the state after that many steps
                    let steps = done_count - row as i64;
                    if ui.selectable_label(steps == 0, text).on_hover_text(tr("Return to this step")).clicked() {
                        // Point history is consumed before the document rows shown here
                        jump = if steps >= 0 {
                            app.session.history_undo_steps(steps as usize) as i64
                        } else {
                            -(app.session.history_redo_steps((-steps) as usize) as i64)
                        };
                    }
                }
            });
        });
    });

    if jump > 0 {
        app.run("edit.undo", json!({ "steps": jump }));
    } else if jump < 0 {
        app.run("edit.redo", json!({ "steps": -jump }));
    }
}

/// Browser File › Export Mesh: pick the download format
#[cfg(target_arch = "wasm32")]
fn export_mesh(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let formats = [
        ("glb", "glTF binary (.glb)", tr("Textures embedded; for game engines and most viewers")),
        ("usdz", "USDZ (.usdz)", tr("Textures packaged; for USD tools and AR Quick Look")),
        ("obj", "Wavefront OBJ (.obj)", tr("Geometry and UVs only")),
    ];
    if !app.ui.windows.export_mesh {
        return;
    }

    let mut open = true;
    let mut picked = None;
    dialog("export_mesh", tr("Export mesh"), &mut open).show(ctx, |ui| {
        ui.label(caption(tr("Download as")));
        for (extension, label, hint) in formats {
            if ui.add_sized(vec2(220.0, ui.spacing().interact_size.y), egui::Button::new(label)).on_hover_text(hint).clicked() {
                picked = Some(extension);
            }
        }
    });

    if let Some(extension) = picked {
        crate::web_files::export_mesh(app, extension);
        open = false;
    }
    app.ui.windows.export_mesh = open;
}

fn about(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let tk = Tokens::get(ctx);
    let mut open = app.ui.windows.about;
    dialog("about", tr("About TextureCraft"), &mut open).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("TextureCraft").size(22.0).strong().color(tk.text_strong));
            ui.label(trf("Version {version}", &[("version", &texturecraft_engine::VERSION)]));
            ui.add_space(6.0);
            ui.label(tr("3D texture painting: layers, masks, smart materials, mesh map baking and export."));
            ui.label(tr("Free and open source (MIT OR Apache-2.0), written in Rust."));
            ui.add_space(6.0);
            ui.hyperlink_to(tr("Join the ArtCraft community on Discord"), "https://discord.gg/artcraft");
            ui.hyperlink_to("getartcraft.com/apps/texturecraft", "https://getartcraft.com/apps/texturecraft");
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Made by the ArtCraft team and community.")).color(tk.text_dim));
        });
    });
    app.ui.windows.about = open;
}

/// The text Help ▸ Export log... saves: a header with the export time, then every log line with
/// its level and source. The session log keeps no time of its own, so every line gets the export
/// time.
pub(crate) fn log_text(lines: &[texturecraft_engine::log::LogEntry], exported_at: &str) -> String {
    let mut out = format!("TextureCraft log, exported {exported_at}\n");
    for line in lines {
        out.push_str(&format!("[{exported_at}] {:<5} {}\n", line.level.tag(), line.line()));
    }
    out
}

/// The current UTC time as `YYYY-MM-DD HH:MM:SS`.
#[cfg(not(target_arch = "wasm32"))]
fn now_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    utc_text(secs)
}

#[cfg(target_arch = "wasm32")]
fn now_utc() -> String {
    utc_text((js_sys::Date::now() / 1000.0) as i64)
}

/// Unix seconds as `YYYY-MM-DD HH:MM:SS` (UTC).
fn utc_text(secs: i64) -> String {
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from a day count (proleptic Gregorian)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// Help ▸ Export log...: saves the whole log to a text file chosen in a save dialog.
pub(crate) fn export_log(app: &mut TextureCraftApp) {
    let text = log_text(&app.session.log, &now_utc());
    #[cfg(not(target_arch = "wasm32"))]
    app.ask_file(FileDialog::save().filter(tr("Text file"), &["txt"]).file_name("texturecraft-log.txt"), Bound::App, move |app, paths| {
        let Some(path) = paths.first() else { return };
        match std::fs::write(path, &text) {
            Ok(()) => app.toast(trf("Exported the log to {path}", &[("path", &path.display().to_string())]), false),
            Err(e) => app.toast(trf("Could not export the log: {error}", &[("error", &e.to_string())]), true),
        }
    });
    #[cfg(target_arch = "wasm32")]
    match crate::web_files::download("texturecraft-log.txt", text.as_bytes()) {
        Ok(()) => app.toast(tr("Exported the log"), false),
        Err(e) => app.toast(trf("Could not export the log: {error}", &[("error", &e)]), true),
    }
}

// ---------------------------------------------------------------------------------------------
// Files (wired to texturecraft-formats once it lands)

pub fn file_dialogs_available() -> bool {
    cfg!(not(target_arch = "wasm32"))
}

/// Remember a project path in File ▸ Recent projects.
pub fn remember(app: &mut TextureCraftApp, path: &str) {
    app.ui.recent.retain(|p| p != path);
    app.ui.recent.insert(0, path.to_string());
    app.ui.recent.truncate(10);
}

/// Window › Save UI Layout › Painting / Rendering / Baking mode: the panels, windows and view
/// settings as that mode shows them, as a JSON file.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_layout(app: &mut TextureCraftApp, mode: crate::state::Mode) {
    use crate::state::Mode;
    let dir = crate::user_dir("layouts");
    let name = match mode {
        Mode::Painting => "painting-mode.json",
        Mode::Rendering => "rendering-mode.json",
        Mode::Baking => "baking-mode.json",
    };
    let dialog = FileDialog::save().file_name(name).filter(tr("UI layout"), &["json"]).directory(dir.clone());
    app.ask_file(dialog, Bound::App, move |app, paths| {
        let Some(path) = paths.first() else { return };

        // The file carries both docks; the mode's own is the one in use when it is loaded
        let mut layout = crate::UiState { recent: Vec::new(), named_layouts: Vec::new(), mode, ..app.ui.clone() };
        layout.sync_mode_dock();
        let result = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| serde_json::to_vec_pretty(&layout).map_err(|e| e.to_string()))
            .and_then(|bytes| std::fs::write(path, bytes).map_err(|e| e.to_string()))
            .map(|_| trf("Saved UI layout {path}", &[("path", &path.display())]));
        match result {
            Ok(msg) => app.toast(msg, false),
            Err(e) => app.toast(trf("UI layout: {error}", &[("error", &e)]), true),
        }
    });
}

/// Window › Load UI Layout: read a layout file saved by Save UI Layout.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_layout(app: &mut TextureCraftApp) {
    let dir = crate::user_dir("layouts");
    let dialog = FileDialog::open().title(tr("Select a UI layout file.")).filter(tr("UI layout"), &["json"]).directory(dir);
    app.ask_file(dialog, Bound::App, move |app, paths| {
        let Some(path) = paths.first() else { return };
        let result = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice::<crate::UiState>(&bytes).map_err(|e| e.to_string()))
            .map(|layout| {
                app.ui.adopt_layout(layout);
                trf("Loaded UI layout {path}", &[("path", &path.display())])
            });
        match result {
            Ok(msg) => app.toast(msg, false),
            Err(e) => app.toast(trf("UI layout: {error}", &[("error", &e)]), true),
        }
    });
}

/// File > More save options > Save as template...: the project's settings as a template in the
/// user templates folder. It saves without a confirmation and shows up in New project at once.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_as_template(app: &mut TextureCraftApp) {
    let dialog = FileDialog::save().title(tr("Save template")).filter(tr("Project template"), &["json"]).directory(crate::user_dir("templates"));
    app.ask_file(dialog, Bound::Project, |app, paths| {
        if let Some(name) = paths.first().and_then(|path| path.file_stem()).and_then(|stem| stem.to_str()) {
            app.run("project.template.save", json!({"name": name}));
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub fn save_as_template(app: &mut TextureCraftApp) {
    app.toast(tr("Saving templates to files is available in the desktop app"), false);
}

/// The browser has no file paths; layouts persist with the rest of the UI state.
#[cfg(target_arch = "wasm32")]
pub fn save_layout(app: &mut TextureCraftApp, _mode: crate::state::Mode) {
    app.toast(tr("Saving UI layouts to files is available in the desktop app"), false);
}

#[cfg(target_arch = "wasm32")]
pub fn load_layout(app: &mut TextureCraftApp) {
    app.toast(tr("Saving UI layouts to files is available in the desktop app"), false);
}

/// File menu actions that need dialogs. Returns true when handled.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_action(app: &mut TextureCraftApp, _ctx: &egui::Context, id: &str) -> bool {
    match id {
        "file.open" => {
            app.ask_file(FileDialog::open().filter(tr("TextureCraft project or mesh"), crate::web_files::OPEN_EXTS), Bound::App, |app, paths| {
                if let Some(path) = paths.first() {
                    open_path(app, &path.display().to_string());
                }
            });
            true
        }
        "file.save" if app.session.path.is_some() => {
            if let Some(result) = app.run("file.save", json!({})) {
                let saved_path = result["path"].as_str().unwrap_or_default();
                app.toast(trf("Saved {path}", &[("path", &saved_path)]), false);
            }
            true
        }
        "file.save" | "file.saveAs" => {
            ask_save_project(app, "file.saveAs");
            true
        }
        "file.saveCopy" => {
            ask_save_project(app, "file.saveCopy");
            true
        }
        "material.load" => {
            app.ask_file(FileDialog::open_many().filter(tr("Materials"), crate::web_files::MATERIAL_EXTS), Bound::Project, |app, paths| {
                for path in paths {
                    app.run("material.load", json!({"path": path.display().to_string()}));
                }
            });
            true
        }
        "file.importResources" | "resource.import" => {
            crate::import_resources::open(app);
            true
        }
        "file.importMaterialX" => {
            app.ask_file(FileDialog::open().filter(tr("MaterialX materials"), &["mtlx"]), Bound::Project, |app, paths| {
                if let Some(path) = paths.first()
                    && let Some(result) = app.run("file.importMaterialX", json!({"path": path.display().to_string()}))
                {
                    let name = result["material"].as_str().unwrap_or_default();
                    for warning in result["warnings"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str) {
                        app.session.log.warn(texturecraft_engine::log::LIBRARY_SOURCE, format!("MaterialX {name}: {warning}"));
                    }
                    app.toast(trf("Imported {name}", &[("name", &name)]), false);
                    app.ui.dock.focus(crate::docking::Panel::Layers);
                }
            });
            true
        }
        "file.importPbrFolder" => {
            crate::pbr_import::pick(app);
            true
        }
        "file.exportMesh" => {
            let name = format!("{}.obj", texturecraft_engine::formats::export::sanitize_file_stem(&app.session.doc.project.name));
            let dialog = FileDialog::save()
                .filter(tr("Wavefront OBJ with MTL, textures alongside"), &["obj"])
                .filter(tr("glTF binary with textures"), &["glb"])
                .filter(tr("USDZ with textures"), &["usdz"])
                .filter(tr("USD text, textures alongside"), &["usda"])
                .filter(tr("USD binary, textures alongside"), &["usdc"])
                .file_name(name);
            app.ask_file(dialog, Bound::Project, |app, paths| {
                if let Some(path) = paths.first()
                    && let Some(result) = app.run("file.exportMesh", json!({"path": path.display().to_string()}))
                {
                    let saved_path = result["path"].as_str().unwrap_or_default();
                    app.toast(trf("Exported mesh to {path}", &[("path", &saved_path)]), false);
                }
            });
            true
        }
        "file.reimportMesh" => {
            // Ask for a replacement only when the mesh source is missing
            if let Some(path) = app.session.remembered_mesh_path().map(str::to_string) {
                reimport_mesh(app, path);
                return true;
            }

            let dialog = FileDialog::open().filter(tr("3D meshes"), &texturecraft_engine::formats::mesh_import::MESH_EXTENSIONS);
            app.ask_file(dialog, Bound::Project, |app, paths| {
                if let Some(path) = paths.first() {
                    reimport_mesh(app, path.display().to_string());
                }
            });
            true
        }
        _ => false,
    }
}

/// Asks where to save the project and runs `cmd` with the answer. Each caller is its own dialog
/// request, so Save As and Save a Copy queue separately.
#[cfg(not(target_arch = "wasm32"))]
#[track_caller]
fn ask_save_project(app: &mut TextureCraftApp, cmd: &'static str) {
    let name = crate::web_files::project_file_name(app);
    let dialog = FileDialog::save().filter(tr("TextureCraft project"), &[texturecraft_engine::core::PROJECT_EXT]).file_name(name);
    // Save a Copy leaves the project unsaved, so it never answers the unsaved-changes prompt.
    let for_prompt = app.saving_for_prompt && cmd == "file.saveAs";
    let asked = app.ask_file(dialog, Bound::Project, move |app, paths| {
        let mut saved = false;
        if let Some(p) = paths.first() {
            let path = p.display().to_string();
            saved = app.run(cmd, json!({"path": path})).is_some();
            if saved {
                remember(app, &path);
                app.toast(trf("Saved {path}", &[("path", &path)]), false);
            }
        }
        // The answer ends the wait. A canceled or failed save leaves the prompt open for an answer.
        if for_prompt {
            app.saving_for_prompt = false;
            app.prompt_saved = saved && !app.session.modified;
        }
    });
    // A Save As already on its way answers for itself: the prompt's buttons stay usable.
    if for_prompt && !asked {
        app.saving_for_prompt = false;
    }
}

/// Reimport the mesh; painted strokes follow the surface when the new mesh's UVs differ.
#[cfg(not(target_arch = "wasm32"))]
fn reimport_mesh(app: &mut TextureCraftApp, path: String) {
    app.run("file.reimportMesh", json!({"path": path, "preserveStrokes": true}));
}

/// Open a project or mesh by path (menus, Recent projects, drag and drop).
#[cfg(not(target_arch = "wasm32"))]
pub fn open_path(app: &mut TextureCraftApp, path: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    if ask_to_save(app, crate::state::Unsaved::Open(path.into())) {
        return;
    }
    if app.run("file.open", json!({"path": path})).is_some() {
        remember(app, path);
        app.ui.mode = Mode::Painting;
    }
}

/// In the browser, Open and Import use the file picker and Save downloads the project.
#[cfg(target_arch = "wasm32")]
pub fn file_action(app: &mut TextureCraftApp, ctx: &egui::Context, id: &str) -> bool {
    match id {
        "file.open" => crate::web_files::open(ctx),
        "file.importResources" | "resource.import" => crate::web_files::import_images(ctx),
        "material.load" => crate::web_files::import_material(ctx),
        "file.save" | "file.saveAs" | "file.saveCopy" => crate::web_files::save_project(app),
        "file.exportMesh" => app.ui.windows.export_mesh = true,
        _ => return false,
    }
    true
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
        let mut p = export_params(app);
        p["dir"] = json!(app.ui.export.dir);
        p["ts"] = json!(ts);
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
    let sets = export_selection(app);
    crate::web_files::export_textures(app, &export_params(app), &sets);
    true
}

pub(crate) fn export_params(app: &TextureCraftApp) -> serde_json::Value {
    let e = &app.ui.export;
    let mut p = json!({"preset": e.preset, "padding": e.padding, "dilation": e.dilation});
    if !e.format.is_empty() {
        p["format"] = json!(e.format);
    }
    if e.bit_depth > 0 {
        p["bitDepth"] = json!(e.bit_depth);
    }
    if e.size > 0 {
        p["size"] = json!(e.size);
    }
    if !e.normal_format.is_empty() {
        p["normalFormat"] = json!(e.normal_format);
    }
    if !e.color_space.is_empty() {
        p["colorSpace"] = json!(e.color_space);
    }
    p
}

fn export_settings_tab(app: &mut TextureCraftApp, ui: &mut Ui, presets: &[texturecraft_engine::formats::ExportPreset]) {
    ui.label(caption(tr("General Export Parameters")));
    egui::Grid::new("exp_grid").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        // The browser downloads the files instead
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.label(tr("Output directory"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut app.ui.export.dir).desired_width(360.0));
                if ui.button("...").clicked() {
                    app.ask_file(FileDialog::folder().directory(app.ui.export.dir.clone()), Bound::App, |app, paths| {
                        if let Some(d) = paths.first() {
                            app.ui.export.dir = d.display().to_string();
                        }
                    });
                }
            });
            ui.end_row();
        }
        ui.label(tr("Output template"));
        let before = app.ui.export.preset.clone();
        egui::ComboBox::from_id_salt("exp_preset").width(360.0).selected_text(app.ui.export.preset.clone()).show_ui(ui, |ui| {
            for p in presets {
                ui.selectable_value(&mut app.ui.export.preset, p.name.clone(), &p.name);
            }
            if !app.session.export_presets.list.is_empty() {
                ui.separator();
            }
            for p in &app.session.export_presets.list {
                ui.selectable_value(&mut app.ui.export.preset, p.name.clone(), &p.name);
            }
        });
        if app.ui.export.preset != before {
            let picked = std::mem::replace(&mut app.ui.export.preset, before);
            crate::export_templates::select(app, picked);
        }
        ui.end_row();
        ui.label(tr("File type"));
        ui.horizontal(|ui| {
            let label = if app.ui.export.format.is_empty() { tr("From template").to_string() } else { app.ui.export.format.clone() };
            egui::ComboBox::from_id_salt("exp_fmt").selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut app.ui.export.format, String::new(), tr("From template"));
                for f in ["png", "jpeg", "tga", "tiff", "bmp", "webp", "exr", "hdr"] {
                    ui.selectable_value(&mut app.ui.export.format, f.to_string(), f);
                }
            });
            let bd = if app.ui.export.bit_depth == 0 { tr("Default").to_string() } else { trf("{n} bits", &[("n", &app.ui.export.bit_depth)]) };
            egui::ComboBox::from_id_salt("exp_bits").selected_text(bd).show_ui(ui, |ui| {
                for (v, l) in [
                    (0u8, tr("Default").to_string()),
                    (8, trf("{n} bits", &[("n", &8)])),
                    (16, trf("{n} bits", &[("n", &16)])),
                    (32, tr("32 bits (float)").to_string()),
                ] {
                    ui.selectable_value(&mut app.ui.export.bit_depth, v, l);
                }
            });
        });
        ui.end_row();
        ui.label(tr("Size"));
        let sz = if app.ui.export.size == 0 { tr("Based on each Texture Set's size").to_string() } else { format!("{0} x {0}", app.ui.export.size) };
        egui::ComboBox::from_id_salt("exp_size").width(360.0).selected_text(sz).show_ui(ui, |ui| {
            ui.selectable_value(&mut app.ui.export.size, 0, tr("Based on each Texture Set's size"));
            for r in [256u32, 512, 1024, 2048, 4096, 8192] {
                ui.selectable_value(&mut app.ui.export.size, r, format!("{r} x {r}"));
            }
        });
        ui.end_row();
        ui.label(tr("Padding"));
        ui.horizontal(|ui| {
            let label = match app.ui.export.padding.as_str() {
                "none" => tr("No padding (passthrough)"),
                "dilation" => tr("Dilation + transparent"),
                "dilationDefault" => tr("Dilation + default background color"),
                _ => tr("Dilation infinite"),
            };
            egui::ComboBox::from_id_salt("exp_pad").width(260.0).selected_text(label).show_ui(ui, |ui| {
                for (v, l) in [
                    ("infinite", tr("Dilation infinite")),
                    ("dilation", tr("Dilation + transparent")),
                    ("dilationDefault", tr("Dilation + default background color")),
                    ("none", tr("No padding (passthrough)")),
                ] {
                    ui.selectable_value(&mut app.ui.export.padding, v.to_string(), l);
                }
            });
            if app.ui.export.padding.starts_with("dilation") {
                ui.add(egui::DragValue::new(&mut app.ui.export.dilation).range(0..=256).suffix(" px"));
            }
        });
        ui.end_row();
    });

    // The reference's two switches; TextureCraft's own normal map and colour space overrides follow them
    crate::unavailable::checkbox(ui, false, tr("Export USD asset"));
    crate::unavailable::checkbox(ui, false, tr("Export shaders parameters"));
    ui.add_space(8.0);
    egui::Grid::new("exp_grid_more").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        ui.label(tr("Normal map format"));
        let nf = match app.ui.export.normal_format.as_str() {
            "openGl" => "OpenGL",
            "directX" => "DirectX",
            _ => tr("From template / project"),
        };
        egui::ComboBox::from_id_salt("exp_nf").selected_text(nf).show_ui(ui, |ui| {
            for (v, l) in [("", tr("From template / project")), ("openGl", "OpenGL"), ("directX", "DirectX")] {
                ui.selectable_value(&mut app.ui.export.normal_format, v.to_string(), l);
            }
        });
        ui.end_row();
        ui.label(tr("Color space"));
        let shown = if app.ui.export.color_space.is_empty() { tr("Default") } else { app.ui.export.color_space.as_str() };
        egui::ComboBox::from_id_salt("exp_cs").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut app.ui.export.color_space, String::new(), tr("Default"));
            for name in ["sRGB", "Linear Rec.709", "ACEScg", "ACES2065-1"] {
                ui.selectable_value(&mut app.ui.export.color_space, name.to_string(), name);
            }
        });
        ui.end_row();
    });
}

/// Height of the Export textures window's tab body, above its buttons
const EXPORT_BODY_HEIGHT: f32 = 440.0;

/// How many files the export writes for the ticked texture sets
fn selected_texture_count(app: &mut TextureCraftApp) -> usize {
    let mut params = export_params(app);
    params["ts"] = json!("all");
    let Ok(plan) = texturecraft_engine::execute(&mut app.session, "export.plan", &params) else { return 0 };
    let skipped = |file: &serde_json::Value| file.get("ts").and_then(serde_json::Value::as_u64).is_some_and(|ts| app.ui.export.skip.contains(&(ts as usize)));
    plan.as_array().into_iter().flatten().filter(|file| !skipped(file)).count()
}

pub fn export(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let tk = Tokens::get(ctx);
    if !app.ui.windows.export {
        app.ui.export.ask = None;
        return;
    }

    let mut open = true;
    let mut do_export = false;
    let presets = texturecraft_engine::formats::builtin_presets();
    dialog("export_textures", tr("Export textures"), &mut open).fixed_size(vec2(900.0, 520.0)).show(ctx, |ui| {
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
                if ui.button(tr("Include all")).clicked() {
                    app.ui.export.skip.clear();
                }
            });
        });
        ui.separator();
        ui.allocate_ui_with_layout(vec2(ui.available_width(), EXPORT_BODY_HEIGHT), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.set_height(EXPORT_BODY_HEIGHT);
            if app.ui.export.tab == 1 {
                crate::export_templates::tab(app, ui, &presets);
                return;
            }

            ui.vertical(|ui| {
                ui.set_width(180.0);

                // The settings apply to every texture set; there are no per-set settings yet
                let _ = ui.selectable_label(true, RichText::new(tr("Global settings")).strong());
                for i in 0..app.session.set_count() {
                    let name = app.session.doc.project.texture_sets.get(i).map(|t| t.name.clone()).unwrap_or_default();
                    let mut on = !app.ui.export.skip.contains(&i);
                    if ui.checkbox(&mut on, name).changed() {
                        if on {
                            app.ui.export.skip.retain(|x| *x != i);
                        } else {
                            app.ui.export.skip.push(i);
                        }
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| match app.ui.export.tab {
                0 => export_settings_tab(app, ui, &presets),
                _ => {
                    let mut params = export_params(app);
                    params["ts"] = json!("all");
                    match texturecraft_engine::execute(&mut app.session, "export.plan", &params) {
                        Ok(v) => {
                            egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                                for f in v.as_array().into_iter().flatten() {
                                    let ts = f.get("ts").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                                    if !app.ui.export.skip.contains(&ts) {
                                        ui.label(RichText::new(f.get("file").and_then(|x| x.as_str()).unwrap_or_default()).monospace());
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            ui.label(RichText::new(e.to_string()).color(tk.danger));
                        }
                    }
                }
            });
        });
        ui.add_space(8.0);
        ui.separator();
        let selected = selected_texture_count(app);
        ui.horizontal(|ui| {
            if ui.button(tr("Reset all")).clicked() {
                app.ui.export = crate::state::ExportUi::default();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button(ui, tr("Export"))).clicked() {
                    do_export = true;
                }

                // The settings are kept with the interface, so saving them closes the window without exporting
                if ui.button(tr("Save settings")).on_hover_text(tr("Keep these settings for the next export")).clicked() {
                    app.ui.windows.export = false;
                }
                if ui.button(tr("Cancel")).clicked() {
                    app.ui.windows.export = false;
                }
                ui.label(trf("{n} textures selected for export", &[("n", &selected)]));
            });
        });
    });
    crate::export_templates::ask(app, ctx);
    if do_export && export_sets(app) {
        app.ui.windows.export = false;
    }
    if !open {
        app.ui.windows.export = false;
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    use super::*;
    use crate::state::Unsaved;

    fn harness() -> Harness<'static, TextureCraftApp> {
        let session = texturecraft_engine::sample_session(256, 64).unwrap();
        let mut h = Harness::builder().with_size(vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
        // These dialog tests query English labels, independently of the system locale.
        h.state_mut().ui.prefs.language = "en".into();
        h.run_steps(2);
        h
    }

    #[test]
    fn a_canceled_save_dialog_does_not_answer_the_prompt() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut h = harness();
        let app = h.state_mut();
        app.file_dialogs.set_override(Some(Box::new(|_| crate::file_dialogs::Answer::Cancelled)));
        app.ui.windows.unsaved = Some(Unsaved::Quit);
        // Saved elsewhere, by a script, while the dialog was open.
        app.session.modified = false;
        app.saving_for_prompt = true;
        ask_save_project(app, "file.saveAs");
        h.run_steps(3);
        assert_eq!(h.state().ui.windows.unsaved, Some(Unsaved::Quit), "the prompt still waits for an answer");
    }

    #[test]
    fn the_prompts_buttons_wait_for_its_save_dialog() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut h = harness();
        let app = h.state_mut();
        app.ui.windows.unsaved = Some(Unsaved::Quit);
        app.session.modified = true;
        h.run_steps(2);
        for label in ["Save", "Don't save", "Cancel"] {
            assert!(!h.get_by_label(label).accesskit_node().is_disabled(), "{label} answers while no dialog is open");
        }
        h.state_mut().saving_for_prompt = true;
        h.run_steps(2);
        for label in ["Save", "Don't save", "Cancel"] {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label} waits for the Save dialog");
        }
    }

    #[test]
    fn a_save_a_copy_answer_does_not_end_the_prompts_wait() {
        // The harness draws the UI, and the i18n tests switch the process-wide language to ones its fonts cannot draw
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut h = harness();
        let app = h.state_mut();
        app.file_dialogs.set_override(Some(Box::new(|_| crate::file_dialogs::Answer::Cancelled)));
        app.saving_for_prompt = true;
        ask_save_project(app, "file.saveCopy");
        h.run_steps(3);
        assert!(h.state().saving_for_prompt, "only the prompt's own Save As ends the wait");
    }

    #[test]
    fn a_save_as_already_on_its_way_ends_the_prompts_wait() {
        // The harness draws the UI, and the i18n tests switch the process-wide language to ones its fonts cannot draw
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut h = harness();
        let app = h.state_mut();

        // One call site, as File › Save As and the prompt's Save share one
        for waiting in [false, true] {
            app.saving_for_prompt = waiting;
            ask_save_project(app, "file.saveAs");
        }
        assert_eq!(app.file_dialogs.pending(), 1);
        assert!(!app.saving_for_prompt, "the prompt's buttons work again");
    }

    /// A change that lands after the Save dialog saved (a bake finishing that frame) keeps the
    /// question open with buttons that still answer.
    #[test]
    fn a_change_after_the_dialog_saved_leaves_the_prompt_usable() {
        // The harness keeps the process-wide language at English while this asserts on labels
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut h = harness();
        let app = h.state_mut();
        app.ui.windows.unsaved = Some(Unsaved::Quit);
        app.session.modified = true;
        app.prompt_saved = true;
        h.run_steps(2);
        assert_eq!(h.state().ui.windows.unsaved, Some(Unsaved::Quit), "the project has unsaved changes again");
        assert!(!h.state().saving_for_prompt);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(h.state().ui.windows.unsaved.is_none());
    }

    /// The History panel runs each step name through the translator (skipped when this build
    /// embeds no catalog that translates "Remove layer"; `i18n` tests cover the template matching)
    #[test]
    fn history_rows_are_shown_in_the_interface_language() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(code) = crate::i18n::LANGUAGES.iter().map(|l| l.code).find(|code| {
            crate::i18n::set_language(code);
            *code != "en" && crate::i18n::tr("Remove layer") != "Remove layer"
        }) else {
            crate::i18n::set_language("en");
            return;
        };

        let translated = crate::i18n::tr("Remove layer").to_string();
        let mut h = harness();
        {
            let app = h.state_mut();
            let layer = app.session.selected_layer().unwrap();
            texturecraft_engine::execute(&mut app.session, "layer.remove", &json!({"layer": layer.0})).unwrap();
            app.ui.dock.focus(crate::docking::Panel::History);
            app.ui.prefs.language = code.into();
        }
        h.run_steps(4);
        let shown = h.query_all_by_label(&translated).count();
        let untranslated = h.query_all_by_label("Remove layer").count();
        h.state_mut().ui.prefs.language = "en".into();
        h.run_steps(4);
        assert!(shown > 0 && untranslated == 0, "{code}: translated rows {shown}, English rows {untranslated}");
        assert!(h.query_all_by_label("Remove layer").count() > 0);
    }
}

#[cfg(test)]
mod log_export_tests {
    use super::*;

    #[test]
    fn utc_text_formats_known_instants() {
        assert_eq!(utc_text(0), "1970-01-01 00:00:00");
        assert_eq!(utc_text(1_709_164_799), "2024-02-28 23:59:59");
        assert_eq!(utc_text(1_709_164_800), "2024-02-29 00:00:00");
    }
}

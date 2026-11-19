//! The Display Settings panel: the view mode, then environment, camera, post effect and viewer settings on one scrolling page

use egui::{RichText, Ui};
use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::ViewMode;
use texturecraft_engine::camera_cmds::DEFAULT_CAMERA;
use texturecraft_engine::core::ToneMapping;
use texturecraft_engine::core::project::{DisplaySettings, EnvAlignment, ShadowMode};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::section_page::{self, PageSection};
use crate::theme::Tokens;
use crate::unavailable;
use crate::widgets::{combo, icon_button, section_with_reset, slider};

/// The page's sections, in the reference's tab order
pub(crate) const SECTIONS: [PageSection; 5] = [
    PageSection { icon: ic::MOUNTAINS, title: "Environment settings" },
    PageSection { icon: ic::VIDEO_CAMERA, title: "Camera settings" },
    PageSection { icon: ic::FUNCTION, title: "Post effects" },
    PageSection { icon: ic::CUBE, title: "Render settings" },
    PageSection { icon: ic::CHECKERBOARD, title: "Visualization settings" },
];

/// Indent of the rows under a sub-heading (Environment, Shadows)
const ROW_INDENT: f32 = 12.0;

/// The Display Settings panel body (a dock tab or rail popup)
pub(crate) fn body(app: &mut TextureCraftApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(tr("View mode"));
        crate::viewport::view_mode_picker(app, ui, "display_view_mode", ui.available_width());
    });

    let mut display = app.session.doc.project.display.clone();
    let before = display.clone();
    let mut import_hdr = false;
    section_page::show(ui, "display_settings", &SECTIONS, |ui, index| match index {
        0 => import_hdr = environment(app, ui, &mut display),
        1 => camera(app, ui, &mut display),
        2 => post_effects(ui, &mut display),
        3 => render_settings(ui, &mut display),
        _ => visualization(app, ui, &mut display),
    });

    if display != before {
        if display.fov != before.fov {
            app.session.camera.fov = display.fov;
        }
        app.session.doc.project.display = display;
    }
    if import_hdr {
        let ctx = ui.ctx().clone();
        crate::web_files::import_environment(app, &ctx);
    }
}

/// A sub-heading inside a section, as the reference's "Environment" and "Shadows"
fn sub_heading(ui: &mut Ui, text: &str) {
    let tokens = Tokens::get(ui.ctx());
    ui.add_space(4.0);
    ui.label(RichText::new(text).color(tokens.text_strong));
}

/// Rows indented under a sub-heading
fn indented(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(ROW_INDENT);
        ui.vertical(add);
    });
}

/// A percentage slider over a value kept as a fraction (0..1)
fn percent_slider(ui: &mut Ui, label: &str, fraction: &mut f32) {
    let mut percent = *fraction * 100.0;
    if slider(ui, label, &mut percent, 0.0..=100.0, 0).changed() {
        *fraction = (percent / 100.0).clamp(0.0, 1.0);
    }
}

/// Environment and Shadows, greyed while a mesh map shows (it ignores lighting); true when Import HDR was picked
fn environment(app: &mut TextureCraftApp, ui: &mut Ui, display: &mut DisplaySettings) -> bool {
    let mesh_map_shown = matches!(app.session.view, ViewMode::MeshMap(_));
    ui.add_enabled_ui(!mesh_map_shown, |ui| {
        let mut import_hdr = false;
        sub_heading(ui, tr("Environment"));
        indented(ui, |ui| {
            import_hdr = crate::environment_picker::show(app, ui, display);
            unavailable::checkbox(ui, false, tr("Override environment map color space"));
            unavailable::greyed(ui, |ui| {
                egui::ComboBox::from_id_salt("env_color_space").width(ui.available_width()).selected_text(tr("Auto")).show_ui(ui, |_| {})
            });
            percent_slider(ui, tr("Environment Opacity"), &mut display.env_opacity);
            slider(ui, tr("Environment Exposure (EV)"), &mut display.env_exposure, -10.0..=10.0, 2);
            percent_slider(ui, tr("Environment Blur"), &mut display.env_blur);
            slider(ui, tr("Environment Rotation"), &mut display.env_rotation, 0.0..=360.0, 0);
            combo(ui, tr("Environment Alignment"), tr(display.env_alignment.label()), |ui| {
                for alignment in EnvAlignment::ALL {
                    ui.selectable_value(&mut display.env_alignment, alignment, tr(alignment.label()));
                }
            });
        });

        sub_heading(ui, tr("Shadows"));
        indented(ui, |ui| {
            ui.checkbox(&mut display.shadows, tr("Shadows")).on_hover_text(tr("Key light (shadow-casting light, direct only)"));
            // The shadow settings work only while shadows are on
            ui.add_enabled_ui(display.shadows, |ui| {
                combo(ui, tr("Computation mode"), tr(display.shadow_mode.label()), |ui| {
                    for mode in ShadowMode::ALL {
                        ui.selectable_value(&mut display.shadow_mode, mode, tr(mode.label()));
                    }
                });
                percent_slider(ui, tr("Shadows opacity"), &mut display.shadow_opacity);
            });
        });
        import_hdr
    })
    .inner
}

/// The camera preset and its restore, the lens, the saved cameras and depth of field
fn camera(app: &mut TextureCraftApp, ui: &mut Ui, display: &mut DisplaySettings) {
    let current = texturecraft_engine::camera_cmds::current(&app.session);
    let names: Vec<String> = app.session.doc.project.cameras.iter().map(|camera| camera.name.clone()).collect();
    let shown = current.and_then(|index| names.get(index)).map_or(tr(DEFAULT_CAMERA), String::as_str).to_string();
    ui.horizontal(|ui| {
        ui.label(tr("Preset"));
        let restore_width = ui.spacing().interact_size.x * 1.5;
        egui::ComboBox::from_id_salt("display_camera_preset").width((ui.available_width() - restore_width).max(60.0)).selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), tr(DEFAULT_CAMERA)).clicked() {
                app.run("camera.select", json!({}));
            }
            for (index, name) in names.iter().enumerate() {
                if ui.selectable_label(current == Some(index), name).clicked() {
                    app.run("camera.select", json!({"index": index}));
                }
            }
        });

        // Back to the preset's own view, after orbiting away from it
        let restore = ui.button(tr("Restore")).on_hover_text(tr("Return to the preset's view"));
        if restore.clicked() {
            let params = current.map_or_else(|| json!({}), |index| json!({"index": index}));
            app.run("camera.select", params);
        }
    });

    slider(ui, tr("Field of view (deg)"), &mut display.fov, 5.0..=120.0, 0);
    let mut ortho = app.session.camera.ortho;
    if ui.checkbox(&mut ortho, tr("Orthographic")).changed() {
        app.run("camera.set", json!({"ortho": ortho}));
    }
    let tone_mapping_name = |tone_mapping: ToneMapping| match tone_mapping {
        ToneMapping::Linear => tr("Linear"),
        ToneMapping::AcesFilmic => tr("ACES filmic"),
    };
    combo(ui, tr("Tone mapping"), tone_mapping_name(display.tone_mapping), |ui| {
        for tone_mapping in [ToneMapping::Linear, ToneMapping::AcesFilmic] {
            ui.selectable_value(&mut display.tone_mapping, tone_mapping, tone_mapping_name(tone_mapping));
        }
    });
    cameras(app, ui);

    sub_heading(ui, tr("Depth of field"));
    ui.checkbox(&mut display.dof.enabled, tr("Depth of field (viewport and Rendering mode)"));
    ui.add_enabled_ui(display.dof.enabled, |ui| {
        let far = (app.session.camera.distance * 4.0).max(display.dof.focus_distance);
        slider(ui, tr("Focus distance"), &mut display.dof.focus_distance, 0.0..=far, 2)
            .on_hover_text(tr("Depth of the sharp plane along the view (0: the orbit center)"));
        slider(ui, tr("Aperture"), &mut display.dof.aperture, 0.0..=1.0, 3)
            .on_hover_text(tr("Lens size: how quickly things blur away from the focus distance"));
        let picking = crate::viewport::focus_picking(ui.ctx());
        if ui.selectable_label(picking, format!("{}  {}", ic::CROSSHAIR_SIMPLE, tr("Pick focus in the viewport"))).clicked() {
            crate::viewport::set_focus_picking(ui.ctx(), !picking);
        }
    });
}

/// The camera list: rename (edit the name), view or delete saved and imported cameras, save the
/// current view.
fn cameras(app: &mut TextureCraftApp, ui: &mut Ui) {
    let current = texturecraft_engine::camera_cmds::current(&app.session);
    let names: Vec<String> = app.session.doc.project.cameras.iter().map(|c| c.name.clone()).collect();
    for (i, name) in names.iter().enumerate() {
        ui.horizontal(|ui| {
            if icon_button(ui, ic::VIDEO_CAMERA, tr("View through this camera"), current == Some(i), 20.0).clicked() {
                app.run("camera.select", json!({"index": i}));
            }
            // The name being typed lives in egui's memory until the field loses focus.
            let id = ui.scope_id().with(("camera_name", i));
            let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| name.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width() - 26.0));
            if r.changed() {
                ui.data_mut(|d| d.insert_temp(id, text.clone()));
            }
            if r.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(id));
                if text.trim() != name && !text.trim().is_empty() {
                    app.run("camera.rename", json!({"index": i, "to": text.trim()}));
                }
            }
            if icon_button(ui, ic::TRASH, tr("Delete this camera"), false, 20.0).clicked() {
                app.run("camera.remove", json!({"index": i}));
            }
        });
    }
    if ui.button(format!("{}  {}", ic::PLUS, tr("Save view as camera"))).clicked() {
        app.run("camera.add", json!({}));
    }
}

/// The post effects switch and amounts, with a reset on their header
fn post_effects(ui: &mut Ui, display: &mut DisplaySettings) {
    // The switch gates all four amounts (the engine has one switch for the group); each amount 0 leaves that effect out
    let post_effect_fields = |d: &DisplaySettings| (d.post_effects, d.bloom, d.vignette, d.sharpen, d.grain);
    let post_effect_defaults = post_effect_fields(&DisplaySettings::default());
    if section_with_reset(ui, "sec_post_effects", tr("Post effects"), false, post_effect_fields(display) == post_effect_defaults, |ui| {
        ui.checkbox(&mut display.post_effects, tr("Post effects (viewport and Rendering mode)"));
        ui.add_enabled_ui(display.post_effects, |ui| {
            slider(ui, tr("Glow"), &mut display.bloom, 0.0..=1.0, 2).on_hover_text(tr("Bright areas bleed light into their surroundings"));
            slider(ui, tr("Vignette"), &mut display.vignette, 0.0..=1.0, 2).on_hover_text(tr("Darkens the corners of the view"));
            slider(ui, tr("Sharpen"), &mut display.sharpen, 0.0..=1.0, 2).on_hover_text(tr("Brings out fine detail"));
            slider(ui, tr("Film grain"), &mut display.grain, 0.0..=1.0, 2).on_hover_text(tr("Adds fine noise over the view"));
        });
    }) {
        (display.post_effects, display.bloom, display.vignette, display.sharpen, display.grain) = post_effect_defaults;
    }
}

/// Temporal anti-aliasing
fn render_settings(ui: &mut Ui, display: &mut DisplaySettings) {
    ui.checkbox(&mut display.taa, tr("Temporal anti-aliasing (refines the 3D view while it is still)"));
}

/// The wireframes and the single-channel views
fn visualization(app: &mut TextureCraftApp, ui: &mut Ui, display: &mut DisplaySettings) {
    ui.checkbox(&mut app.ui.wireframe_3d, tr("Show wireframe in the 3D view"));
    ui.checkbox(&mut app.ui.uv_wireframe, tr("Show UV wireframe in the 2D view"));
    ui.checkbox(&mut display.solo_unlit, tr("Display solo view without lighting (unlit)"));
}

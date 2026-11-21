use std::{borrow::Cow, collections::BTreeMap};

use super::{ANCHOR_PUBLISH_LAYER, ANCHOR_SOURCE, COLOR_SELECTION_OPCODE, CompositeProgram, FIRST_EXTENDED_PATTERN, FIRST_GENERATOR, TILE_GENERATOR_OPCODE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Features {
    patterns: bool,
    // Zero selects the generic pattern shader on other backends.
    pattern_cases: u128,
    tile_generator: bool,
    authored_images: bool,
    mask_preview: bool,
    extended_blend_modes: bool,
    generators: bool,
    curvature: bool,
    sampled_generator_inputs: bool,
    spatial: bool,
    mask_effects: bool,
    content_effects: bool,
    height_mask: bool,
    anchors: bool,
    material_anchors: bool,
    color_selection: bool,
}

impl Features {
    fn of(program: &CompositeProgram, specialize_patterns: bool) -> Self {
        let mut features = Self {
            authored_images: !program.material_images.is_empty(),
            mask_preview: program.is_mask_preview(),
            spatial: !program.filters.stages.is_empty(),
            ..Self::default()
        };
        features.material_anchors = program.filters.stages.iter().any(|stage| stage.anchor.is_some());
        for op in program.operations.iter().take(program.layer_count as usize).filter(|op| op.ids[1] != ANCHOR_PUBLISH_LAYER) {
            features.mask_effects |= op.effects[1] != 0;
            features.content_effects |= op.effects[3] != 0;
            features.height_mask |= op.constraints[2] != 0;
        }
        // Inspect the compiled operations, including sources referenced by
        // generator overrides and fills in the mask preview.
        for op in &program.operations {
            let kind = op.ids[1];
            features.extended_blend_modes |= op.balance[2] > 8.0;
            features.anchors |= kind == ANCHOR_SOURCE;
            features.color_selection |= kind == COLOR_SELECTION_OPCODE;
            if matches!(kind, 1..=9 | 11..=13 | FIRST_EXTENDED_PATTERN..=TILE_GENERATOR_OPCODE) {
                features.patterns = true;
                if specialize_patterns {
                    features.pattern_cases |= 1u128.checked_shl(kind).unwrap_or(u128::MAX);
                }
            }
            features.tile_generator |= kind == TILE_GENERATOR_OPCODE;
            if (FIRST_GENERATOR..FIRST_EXTENDED_PATTERN).contains(&kind) {
                features.generators = true;
                features.curvature |= op.uv[1] != 0.0;
                if let Some(source) = op.effects[3].checked_sub(1).and_then(|index| program.operations.get(index as usize)) {
                    features.sampled_generator_inputs |= source.ids[1] != 0;
                }
            }
        }
        features
    }

    fn source(self, supports_curvature: bool, supports_id: bool) -> String {
        fn include<'a>(enabled: bool, full: &'a str, stub: &'a str) -> &'a str {
            if enabled { full } else { stub }
        }

        let extra = pattern_source(include_str!("shaders/procedural_extra.wgsl"), self.pattern_cases);
        let patterns = pattern_source(include_str!("shaders/composite_patterns.wgsl"), self.pattern_cases);

        // Naga substitutes overrides on GL, but still emits the inactive
        // function bodies. Omit those helpers so Mesa does not inline them
        // before its dead-code pass.
        [
            "override enable_authored_images: bool = false;\n             override enable_generators: bool = false;\n             override enable_curvature_detail: bool = false;\n             override enable_spatial_fields: bool = false;\n             override enable_anchor_reads: bool = false;",
            include_str!("shaders/noise.wgsl"),
            include(self.patterns, extra.as_ref(), ""),
            include(
                self.tile_generator,
                include_str!("shaders/tile_generator.wgsl"),
                "fn tile_generator(offset: u32, uv: vec2<f32>) -> f32 { return 0.0; }",
            ),
            include(
                self.patterns,
                patterns.as_ref(),
                "fn procedural_value(layer: Operation, uv: vec2<f32>, surface: Surface) -> vec3<f32> { return layer.low.rgb; }",
            ),
            include(
                self.authored_images,
                include_str!("shaders/material_image.wgsl"),
                "fn authored_image_value(layer: Operation, uv: vec2<f32>, surface: Surface) -> vec3<f32> { return layer.low.rgb; }\n                 fn decal_fill_value(layer: Operation, uv: vec2<f32>, surface: Surface) -> vec4<f32> { return vec4(layer.low.rgb, 1.0); }",
            ),
            include_str!("shaders/stencil_projection.wgsl"),
            include(
                self.curvature && supports_curvature,
                include_str!("shaders/curvature_sample.wgsl"),
                "fn curvature_level_at(pixel: vec2<u32>, level: u32) -> f32 { return 0.0; }",
            ),
            include(
                self.curvature,
                include_str!("shaders/composite_curvature_detail.wgsl"),
                "fn generator_curvature(effect: Operation, pixel: vec2<u32>, finest: f32, mode: u32, micro: f32) -> f32 { return 0.0; }",
            ),
            include(
                self.sampled_generator_inputs,
                "fn generator_source_value(index: u32, texel: LayerTexel) -> f32 {\n                     return fill_value(layers[index], texel.uv, texel.surface).x;\n                 }",
                "fn generator_source_value(index: u32, texel: LayerTexel) -> f32 { return layers[index].low.x; }",
            ),
            include(
                self.generators,
                include_str!("shaders/composite_generators.wgsl"),
                "fn generator_ramp(effect: Operation, texel: LayerTexel) -> f32 { return 0.0; }\n                 fn finish_generator(effect: Operation, ramp: f32) -> f32 { return ramp; }",
            ),
            include(
                self.spatial,
                include_str!("shaders/composite_spatial.wgsl"),
                "fn previous_field(start: u32, count: u32) -> u32 { return 0u; }\n                 fn filter_value(slot: u32, pixel: vec2<u32>) -> vec4<f32> { return vec4(0.0); }",
            ),
            include(
                self.material_anchors,
                include_str!("shaders/composite_anchors.wgsl"),
                "fn capture_material_anchor(texel: LayerTexel, end: u32, triangle: u32) -> vec3<f32> { return params.anchor_default.rgb; }",
            ),
            include(
                self.color_selection && supports_id,
                include_str!("shaders/id_sample.wgsl"),
                "fn id_color_at(pixel: vec2<u32>) -> vec3<f32> { return vec3(0.0); }",
            ),
            include(
                self.mask_effects,
                include_str!("shaders/composite_mask_effects.wgsl"),
                "fn layer_mask(layer: Operation, texel: LayerTexel, count: u32) -> f32 { return layer.balance.y; }",
            ),
            include(
                self.content_effects,
                include_str!("shaders/composite_content_effects.wgsl"),
                "fn layer_content(layer: Operation, texel: LayerTexel, count: u32) -> vec4<f32> {
                    let content = base_content(layer, texel);
                    return select(vec4(0.0), content, content.a > 0.0);
                }",
            ),
            include(
                self.height_mask,
                include_str!("shaders/composite_height_mask.wgsl"),
                "fn mask_with_height(layer: Operation, texel: LayerTexel, mask: f32) -> f32 { return mask; }",
            ),
            include_str!("shaders/composite_groups.wgsl"),
            include_str!("shaders/composite.wgsl"),
        ]
        .join("\n")
    }

    fn constants(self, grouped: bool) -> [(&'static str, f64); 8] {
        [
            ("enable_authored_images", f64::from(u8::from(self.authored_images))),
            ("enable_generators", f64::from(u8::from(self.generators))),
            ("enable_curvature_detail", f64::from(u8::from(self.curvature))),
            ("enable_spatial_fields", f64::from(u8::from(self.spatial))),
            ("enable_anchor_reads", f64::from(u8::from(self.anchors))),
            ("composite_groups", f64::from(u8::from(grouped))),
            ("composite_mask_preview", f64::from(u8::from(self.mask_preview))),
            ("enable_extended_blend_modes", f64::from(u8::from(self.extended_blend_modes))),
        ]
    }
}

fn pattern_source(source: &'static str, cases: u128) -> Cow<'static, str> {
    if cases == 0 || cases == u128::MAX {
        return Cow::Borrowed(source);
    }
    // FXC inlines each pattern into the compositor's sampling loops. Keep only
    // the switch arms referenced by this program before handing it to the
    // compiler. The fallback arm is needed for the gradient.
    fn retain_cases(source: &str, cases: u128) -> Option<String> {
        const SWITCH: &str = "    switch kind {\n";
        const CASE: &str = "        case ";
        const DEFAULT: &str = "        default:";
        let (head, switch) = source.split_once(SWITCH)?;
        let (arms, tail) = switch.split_once(DEFAULT)?;
        let mut selected = String::with_capacity(source.len());
        selected.push_str(head);
        selected.push_str(SWITCH);
        for arm in arms.strip_prefix(CASE)?.split("\n        case ") {
            let (opcode, _) = arm.split_once("u:")?;
            let opcode = opcode.parse().ok()?;
            if arm.lines().skip(1).any(|line| line.trim_start().starts_with("case ")) {
                return None;
            }
            if cases & 1u128.checked_shl(opcode)? != 0 {
                selected.push_str(CASE);
                selected.push_str(arm.trim_end());
                selected.push('\n');
            }
        }
        selected.push_str(DEFAULT);
        selected.push_str(tail);
        Some(selected)
    }

    // If the switch format changes, keep all functionality and let the
    // compiler process the original source.
    retain_cases(source, cases).map_or(Cow::Borrowed(source), Cow::Owned)
}

// Limit the number of DX12 pattern combinations retained across project and
// material edits. Ordinary pipeline variants keep their existing lifetime.
const MAX_PATTERN_VARIANTS: usize = 64;

struct Variants {
    module: wgpu::ShaderModule,
    render: [Option<wgpu::ComputePipeline>; 2],
    prepare: [Option<wgpu::ComputePipeline>; 2],
    last_used: u64,
}

pub(super) struct CompositePipelines {
    supports_curvature: bool,
    supports_id: bool,
    specialize_patterns: bool,
    variants: BTreeMap<Features, Variants>,
    dispatches: u64,
}

impl CompositePipelines {
    pub(super) fn new(device: &wgpu::Device, supports_curvature: bool, supports_id: bool) -> Self {
        let specialize_patterns = device.adapter_info().backend == wgpu::Backend::Dx12;
        Self { supports_curvature, supports_id, specialize_patterns, variants: BTreeMap::new(), dispatches: 0 }
    }

    pub(super) fn get(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        program: &CompositeProgram,
    ) -> (wgpu::ComputePipeline, Option<wgpu::ComputePipeline>) {
        let features = Features::of(program, self.specialize_patterns);
        if features.pattern_cases != 0
            && !self.variants.contains_key(&features)
            && self.variants.keys().filter(|variant| variant.pattern_cases != 0).count() >= MAX_PATTERN_VARIANTS
            && let Some((&oldest, _)) = self.variants.iter().filter(|(variant, _)| variant.pattern_cases != 0).min_by_key(|(_, variants)| variants.last_used)
        {
            self.variants.remove(&oldest);
        }
        self.dispatches = self.dispatches.saturating_add(1);
        let supports_curvature = self.supports_curvature;
        let supports_id = self.supports_id;
        let variants = self.variants.entry(features).or_insert_with(|| Variants {
            module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("tc_gpu_composite"),
                source: wgpu::ShaderSource::Wgsl(features.source(supports_curvature, supports_id).into()),
            }),
            render: [None, None],
            prepare: [None, None],
            last_used: self.dispatches,
        });
        variants.last_used = self.dispatches;
        let module = &variants.module;
        let pipeline = |entry, label, grouped| {
            eprintln!("GPU probe create pipeline {label}, grouped={grouped}, features={features:?}");
            #[cfg(not(target_arch = "wasm32"))]
            let started = std::time::Instant::now();
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions { constants: &features.constants(grouped), ..Default::default() },
                cache: None,
            });
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("GPU probe pipeline {label} returned in {:.1} ms ({:?})", started.elapsed().as_secs_f64() * 1000.0, device.adapter_info().backend);
            pipeline
        };
        let grouped = program.groups.depth != 0;
        let render = variants.render[usize::from(grouped)].get_or_insert_with(|| pipeline("compose", "tc_gpu_composite", grouped)).clone();
        // Only material anchor capture needs to walk the group structure while preparing a source.
        let grouped_capture = grouped && features.material_anchors;
        let prepare = features.spatial.then(|| {
            variants.prepare[usize::from(grouped_capture)].get_or_insert_with(|| pipeline("prepare_filter", "tc_spatial_source", grouped_capture)).clone()
        });
        (render, prepare)
    }
}

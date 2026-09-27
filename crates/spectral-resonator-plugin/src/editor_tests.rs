//! Headless rendering uses the production egui UI and wgpu painter. It renders
//! to our own texture, without capturing or controlling any desktop window.
use super::*;
use egui_wgpu::{RenderState, RendererOptions, ScreenDescriptor, wgpu};
use nice_plug::{context::gui::GuiContextInner, params::internals::ParamPtr};

struct TestGui;
impl GuiContextInner for TestGui {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, _: ParamPtr, _: f32) {}
    unsafe fn raw_end_set_parameter(&self, _: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        unreachable!()
    }
    fn set_state(&self, _: PluginState) {
        unreachable!()
    }
    fn request_restart(&self) {
        unreachable!()
    }
}

struct GestureHost {
    params: Arc<SpectralResonatorParams>,
    events: std::sync::Mutex<Vec<char>>,
}
impl GuiContextInner for GestureHost {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _: ParamPtr) {
        self.events.lock().unwrap().push('b');
    }
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, value: f32) {
        assert_eq!(param, self.params.low_mid_hz.as_ptr());
        // SAFETY: The parameter pointer is validated above and owned by our
        // Arc for the entire synchronous simulated-host interaction.
        unsafe {
            param._internal_set_normalized_value(value);
        }
        self.events.lock().unwrap().push('s');
    }
    unsafe fn raw_end_set_parameter(&self, _: ParamPtr) {
        self.events.lock().unwrap().push('e');
    }
    fn get_state(&self) -> PluginState {
        unreachable!()
    }
    fn set_state(&self, _: PluginState) {
        unreachable!()
    }
    fn request_restart(&self) {
        unreachable!()
    }
}

#[test]
fn crossover_drag_sends_a_balanced_host_gesture_and_layout_fits() {
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let params = Arc::new(SpectralResonatorParams::default());
    let host = Arc::new(GestureHost {
        params: params.clone(),
        events: Default::default(),
    });
    let gui = GuiContext::new(host.clone());
    let mut editor = ResonatorEditor::new(params.clone(), SharedAnalysis::new());
    let mut default_plot_height = 0.0;
    for (width, height) in [(1120.0, 780.0), (1120.0, 1100.0), (980.0, 700.0)] {
        for tab in 0..4 {
            editor.tab = tab;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))),
                    ..Default::default()
                },
                |ui| {
                    editor.draw(ui, &gui);
                    assert!(
                        ui.min_rect().right() <= width + 1.0,
                        "tab {tab} grew past width"
                    );
                    assert!(
                        ui.min_rect().bottom() <= height + 1.0,
                        "tab {tab} grew past height {height}: {:?}",
                        ui.min_rect()
                    );
                },
            );
            output.textures_delta.clear();
            if height == 780.0 {
                default_plot_height = editor.plot_rect.height();
            } else if height == 1100.0 {
                assert!(
                    (editor.plot_rect.height() - default_plot_height - 320.0).abs() < 1.0,
                    "Resizing must grow the spectrum by the full added height"
                );
            }
        }
    }
    let start = Pos2::new(
        editor.plot_rect.left()
            + analyzer::fraction(params.low_mid_hz.value(), 48000.0) * editor.plot_rect.width(),
        editor.plot_rect.center().y,
    );
    let end = start + Vec2::new(60.0, 0.0);
    for events in [
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        vec![egui::Event::PointerMoved(end)],
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    ] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 700.0))),
                events,
                ..Default::default()
            },
            |ui| editor.draw(ui, &gui),
        );
        output.textures_delta.clear();
    }
    assert!(params.low_mid_hz.value() > 300.0);
    let events = host.events.lock().unwrap();
    assert_eq!(events.first(), Some(&'b'));
    assert_eq!(events.last(), Some(&'e'));
    assert_eq!(events.iter().filter(|&&e| e == 'b').count(), 1);
    assert_eq!(events.iter().filter(|&&e| e == 'e').count(), 1);
}

#[test]
#[ignore = "requires a GPU; writes diagnostic UI images under target/ui-preview"]
fn render_gpu_preview() {
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let gui = GuiContext::new(Arc::new(TestGui));
    let shared = SharedAnalysis::new();
    let mut capture = crate::analyzer::AudioCapture::new(shared.clone());
    let preview_params = SpectralResonatorParams {
        unison_mode: EnumParam::new("Unison Mode", crate::params::UnisonMode::Post),
        unison_voices: IntParam::new("Unison", 4, IntRange::Linear { min: 1, max: 8 }),
        ..Default::default()
    };
    let mut editor = ResonatorEditor::new(Arc::new(preview_params), shared);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let render = pollster::block_on(RenderState::create(
        &Default::default(),
        &instance,
        None,
        RendererOptions::default(),
    ))
    .unwrap();
    eprintln!("Preview GPU: {:?}", render.adapter.get_info());
    let device = &render.device;
    let queue = &render.queue;
    let mut renderer = render.renderer.write();
    let mut engine = spectral_dsp::SpectralResonator::new(2, 4096, 512, 48000.0).unwrap();
    let controls = spectral_dsp::ResonatorControls {
        note: Some(57),
        wet_level: 4.0,
        unison_mode: spectral_dsp::UnisonMode::Post,
        modulation: spectral_dsp::ModulationControls {
            unison_voices: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut seed = 7u32;
    // Eight seconds of a sweep plus stereo noise, processed by the actual DSP.
    // Upload each frame's deltas; no invented waterfall pixels are used.
    for block in 0..240 {
        for i in 0..1600 {
            let t = (block * 1600 + i) as f32 / 48000.0;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (seed as f32 / u32::MAX as f32 - 0.5) * 0.06;
            let input = [
                noise + 0.15 * (std::f32::consts::TAU * (100.0 * t + 60.0 * t * t)).sin(),
                noise,
            ];
            let mut audio = input;
            engine.process_frame(&mut audio, controls);
            capture.push(input, engine.output_parts().1, true);
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1120.0, 780.0))),
                time: Some(block as f64 / 30.0),
                ..Default::default()
            },
            |ui| editor.draw(ui, &gui),
        );
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        for id in &output.textures_delta.free {
            renderer.free_texture(id);
        }
        output.textures_delta.clear();
    }
    assert_eq!(editor.rows, HISTORY);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-preview");
    std::fs::create_dir_all(&root).unwrap();
    for (name, tab, width, height) in [
        ("tab-0", 0, 1120u32, 780u32),
        ("tab-1", 1, 980, 700),
        ("tab-2", 2, 1120, 780),
        ("tab-3", 3, 1120, 780),
        ("tall", 0, 1120, 1100),
    ] {
        editor.tab = tab;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(width as f32, height as f32),
                )),
                ..Default::default()
            },
            |ui| editor.draw(ui, &gui),
        );
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        output.textures_delta.clear();
        let shapes = ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: output.pixels_per_point,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("UI test target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let commands = renderer.update_buffers(device, queue, &mut encoder, &shapes, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.render(&mut pass.forget_lifetime(), &shapes, &screen);
        }
        let stride = (width * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (stride * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for row in mapped.chunks(stride as usize) {
            for rgba in row[..width as usize * 4].chunks(4) {
                if matches!(
                    render.target_format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    ppm.extend_from_slice(&[rgba[2], rgba[1], rgba[0]]);
                } else {
                    ppm.extend_from_slice(&rgba[..3]);
                }
            }
        }
        std::fs::write(root.join(format!("{name}.ppm")), ppm).unwrap();
        drop(mapped);
        readback.unmap();
    }
    editor.editor_closed();
    assert!(!editor.shared.enabled.load(Ordering::Relaxed));
    assert!(editor.texture.is_none());
}

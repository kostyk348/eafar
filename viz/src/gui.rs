//! Drawing-only module: converts the simulation session into an egui
//! texture and provides the controls/HUD. All simulation logic (rewind
//! buffer, stats, playback) lives in [`crate`] and is unit-tested there.
//!
//! This module is compiled only with the `gui` feature, which requires
//! cargo >= 1.85 (parts of the egui/eframe dependency tree ship
//! edition-2024 manifests).

use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, Vec2};

use eafar::prelude::*;

use crate::{Frame, Playback, RewindBuffer, StatsTracker};

/// Demo world edge (192x192 cells = 6x6 regions of 32x32).
pub const WORLD: u32 = 192;
/// Region edge for the demo (matches the engine default).
pub const REGION: u32 = 32;
/// Rewind ring-buffer depth (frames; ~148 KiB each at this world size).
pub const REWIND_CAP: usize = 180;
/// Initial seed of the demo.
pub const SEED: u64 = 20260731;
const DEFAULT_GPS: f64 = 30.0;

/// The eframe application: session + GUI state.
pub struct VizApp {
    session: SparseSession,
    rule: Workload,
    seed: u64,
    playback: Playback,
    rewind: RewindBuffer,
    stats: StatsTracker,
    texture: egui::TextureHandle,
    last_stats: GenerationStats,
    show_active: bool,
    /// Slider mirror of `playback.gens_per_sec` (egui needs a `&mut`).
    speed_display: f64,
}

impl VizApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let session =
            SparseSession::new(Workload::Conway, WORLD, WORLD, REGION, SEED)
                .expect("valid demo session parameters");
        let texture = cc.egui_ctx.load_texture(
            "world",
            egui::ColorImage::new(
                [WORLD as usize, WORLD as usize],
                vec![Color32::BLACK; (WORLD * WORLD) as usize],
            ),
            egui::TextureOptions::NEAREST,
        );
        VizApp {
            session,
            rule: Workload::Conway,
            seed: SEED,
            playback: Playback::new(DEFAULT_GPS),
            rewind: RewindBuffer::new(REWIND_CAP),
            stats: StatsTracker::new(0.2),
            texture,
            last_stats: GenerationStats::default(),
            show_active: true,
            speed_display: DEFAULT_GPS,
        }
    }

    /// Rebuild the session after a workload/seed change.
    fn rebuild(&mut self) {
        self.session =
            SparseSession::new(self.rule, WORLD, WORLD, REGION, self.seed)
                .expect("valid demo session parameters");
        self.rewind.clear();
        self.playback.reset();
        self.playback.pause();
        self.stats.reset(0);
        self.last_stats = GenerationStats::default();
    }

    /// Step the simulation `steps` generations, pushing a rewind frame
    /// before each step (so undo lands on the pre-step state).
    fn simulate(&mut self, steps: u64) {
        if steps == 0 {
            return;
        }
        let t0 = Instant::now();
        for _ in 0..steps {
            self.rewind.push(Frame::capture(&self.session));
            match self.session.step_generation() {
                Ok(stats) => self.last_stats = stats,
                Err(e) => eprintln!("eafar-viz: step failed: {e}"),
            }
        }
        let elapsed = t0.elapsed().as_secs_f64();
        self.stats
            .record(self.session.events_total(), elapsed.max(1e-6));
    }

    /// Rewind one frame (one step back).
    fn rewind_one(&mut self) {
        if let Some(frame) = self.rewind.undo() {
            if let Err(e) = frame.restore(&mut self.session) {
                eprintln!("eafar-viz: rewind failed: {e}");
            }
            self.playback.reset();
            self.stats.reset(self.session.events_total());
        }
    }

    /// Inject an event: poke a cell (the click position) and wake its
    /// region — the live demonstration of the event-driven wake path.
    fn poke(&mut self, rect: &Rect, pos: Pos2) {
        let fx = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 0.9999);
        let fy = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 0.9999);
        let x = (fx * WORLD as f32) as u32;
        let y = (fy * WORLD as f32) as u32;
        let field = self.rule.field();
        // Capture BEFORE the poke so Rewind undoes it too.
        self.rewind.push(Frame::capture(&self.session));
        let cur = self.session.world().get(field, x, y).unwrap_or(0.0);
        let value = match self.rule {
            Workload::Conway => {
                if cur >= 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            _ => 1.0,
        };
        let _ = self.session.world_mut().set(field, x, y, value);
        let _ = self.session.sched_mut().wake(x, y);
    }

    /// Rebuild the texture from the current session: heat-ramp colors per
    /// rule, plus a white outline around ACTIVE regions (the highlight of
    /// "which parts of the world are awake right now").
    fn upload_texture(&mut self) {
        let (w, h) = (WORLD as usize, WORLD as usize);
        let field = self.rule.field();
        let cells = self.session.world().data(field);
        let sched = self.session.sched();
        let (ww, wh) = (self.session.world().width(), self.session.world().height());
        let mut pixels: Vec<Color32> = Vec::with_capacity(w * h);
        for y in 0..wh {
            for x in 0..ww {
                let v = cells[y as usize * w + x as usize];
                let mut color = cell_color(self.rule, v);
                if self.show_active {
                    let active = sched.is_active_id(sched.region_id(x, y));
                    let east = if x + 1 < ww {
                        sched.is_active_id(sched.region_id(x + 1, y))
                    } else {
                        false
                    };
                    let south = if y + 1 < wh {
                        sched.is_active_id(sched.region_id(x, y + 1))
                    } else {
                        false
                    };
                    if active != east || active != south {
                        color = Color32::WHITE;
                    }
                }
                pixels.push(color);
            }
        }
        self.texture
            .set(egui::ColorImage::new([w, h], pixels), egui::TextureOptions::NEAREST);
    }

    fn controls_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("EAFAR viz");
        ui.small("sparse event-driven cellular automata");
        ui.separator();

        // Playback controls.
        ui.horizontal(|ui| {
            let label = if self.playback.is_playing() { "Pause" } else { "Play" };
            if ui.button(label).clicked() {
                self.playback.toggle();
            }
            if ui.button("Step").clicked() {
                self.playback.step_once();
            }
            if ui.button("Rewind").clicked() {
                self.rewind_one();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Reset").clicked() {
                self.rebuild();
            }
            if ui.button("Rewind all").clicked() {
                while let Some(frame) = self.rewind.undo() {
                    if let Err(e) = frame.restore(&mut self.session) {
                        eprintln!("eafar-viz: rewind failed: {e}");
                        break;
                    }
                }
                self.playback.reset();
                self.playback.pause();
                self.stats.reset(self.session.events_total());
            }
        });
        ui.add(
            egui::Slider::new(&mut self.speed_display, 1.0..=240.0)
                .text("gens/s")
                .logarithmic(true),
        );
        self.playback.set_gens_per_sec(self.speed_display);

        ui.separator();
        // Workload selector: switching rebuilds the session.
        let mut rule = self.rule;
        egui::ComboBox::from_label("Workload")
            .selected_text(self.rule.name())
            .show_ui(ui, |ui| {
                for w in Workload::ALL {
                    ui.selectable_value(&mut rule, w, w.name());
                }
            });
        if rule != self.rule {
            self.rule = rule;
            self.rebuild();
        }

        ui.horizontal(|ui| {
            ui.label("seed");
            if ui.add(egui::DragValue::new(&mut self.seed).speed(1.0)).changed() {
                self.rebuild();
            }
        });
        ui.checkbox(&mut self.show_active, "outline active regions");
        ui.label(format!("rewind buffer: {}/{} frames", self.rewind.len(), self.rewind.cap()));

        ui.separator();
        ui.heading("Live stats");
        let total_regions = self.session.sched().region_count();
        ui.monospace(format!(
            "generation:       {}",
            self.session.generation()
        ));
        ui.monospace(format!(
            "active regions:   {} / {}",
            self.session.sched().active_count(),
            total_regions
        ));
        ui.monospace(format!(
            "active ratio:     {:.1}%",
            100.0 * self.last_stats.active_cells as f64
                / (WORLD * WORLD) as f64
        ));
        ui.monospace(format!(
            "changed last gen: {}",
            self.last_stats.changed_cells
        ));
        ui.monospace(format!(
            "events total:     {}",
            self.session.events_total()
        ));
        ui.monospace(format!(
            "events/s (EMA):   {:.0}",
            self.stats.events_per_sec()
        ));
        ui.monospace(format!(
            "state hash:       0x{:016x}",
            self.session.state_hash()
        ));
        ui.separator();
        ui.small("click the world to inject an event (it wakes its region)");
        ui.small("rewind walks back through the ring buffer of recent states");
    }

    fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        self.upload_texture();
        let avail = ui.available_size();
        let side = avail.x.min(avail.y);
        let response = ui.add(egui::Image::new(&self.texture).max_size(Vec2::splat(side)));
        let resp = response.interact(egui::Sense::click());
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.poke(&resp.rect, pos);
            }
        }
    }
}

impl eframe::App for VizApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt) as f64;
        let steps = self.playback.advance(dt);
        self.simulate(steps);
        if self.playback.is_playing() {
            // Pace repaints to the requested rate (and never slower than
            // 30 Hz so the UI stays responsive while stepping in bursts).
            let interval = (1.0 / self.playback.gens_per_sec().max(1.0)).min(1.0 / 30.0);
            ctx.request_repaint_after(Duration::from_secs_f64(interval));
        }

        egui::SidePanel::left("controls")
            .default_width(280.0)
            .show(ctx, |ui| self.controls_ui(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.canvas_ui(ui));
    }
}

fn cell_color(rule: Workload, v: f32) -> Color32 {
    match rule {
        Workload::Conway => {
            if v >= 0.5 {
                Color32::from_rgb(245, 158, 66)
            } else {
                Color32::from_rgb(16, 18, 26)
            }
        }
        Workload::Heat => {
            let t = v.clamp(0.0, 1.0);
            Color32::from_rgb(
                (40.0 + 215.0 * t) as u8,
                (60.0 + 40.0 * t) as u8,
                (120.0 - 90.0 * t) as u8,
            )
        }
        Workload::Contagion => {
            if v >= 2.0 {
                Color32::from_rgb(96, 128, 96) // recovered / immune
            } else if v >= 1.0 {
                let t = (v - 1.0).clamp(0.0, 1.0);
                Color32::from_rgb(255, (90.0 - 40.0 * t) as u8, (60.0 - 40.0 * t) as u8)
            } else {
                Color32::from_rgb(20, 42, 48) // susceptible
            }
        }
    }
}

/// Launch the window.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 840.0]),
        ..Default::default()
    };
    eframe::run_native(
        "EAFAR — sparse event-driven cellular automata",
        options,
        Box::new(|cc| Ok(Box::new(VizApp::new(cc)))),
    )
}

//! The vinopilot window: a grid of cards, one per vinoPilot component, each
//! with Install, Update and Uninstall, and the installer's output in a panel
//! underneath. All the knowledge is in `vinopilot-core`; this only draws it.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, RichText, vec2};
use vino_ui::{Button, ChipKind, Row, Tone, a11y, theme, widgets};
use vinopilot_core::catalog::{self, Action, Channel, Id};
use vinopilot_core::detect::{Installed, Local, Part, Paths, SystemPackage};
use vinopilot_core::job::{Env, Job, Outcome};
use vinopilot_core::remote::Latest;
use vinopilot_core::status::{self, Update};
use vinopilot_core::{Config, config, detect, remote};

pub mod desktop;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Nine cards, three by three; one column when three don't fit.
const COLUMNS: usize = 3;
/// Wide enough for the four ways in of a card with a Flatpak, on a row of
/// their own.
const CARD_MIN: f32 = 290.0;
/// Narrower inside than this, four ways in go above Install, not beside it.
const WAYS_BESIDE: f32 = 330.0;
const CARD_HEIGHT: f32 = 172.0;
const GAP: f32 = 12.0;
/// The grid stops growing here, so cards don't stretch across a 4K screen.
const PAGE_MAX: f32 = 3.0 * 340.0 + 2.0 * GAP;

/// The grid, in vinopilot.ca's order. vinoPilot itself has no card: its
/// version and update live in the header.
const CARDS: [Id; 9] = [
    Id::App(vino_ui::App::Whisper),
    Id::App(vino_ui::App::AuthFace),
    Id::App(vino_ui::App::OpenEffects),
    Id::App(vino_ui::App::Pop),
    Id::App(vino_ui::App::Speak),
    Id::App(vino_ui::App::Search),
    Id::App(vino_ui::App::Watch),
    Id::Ovfetch,
    Id::Ui,
];

/// What a local scan finds: each component, and vinoPilot's own commands.
type Scan = (HashMap<Id, Local>, Vec<Part>);

pub struct Pilot {
    config: Config,
    source_field: String,
    local: HashMap<Id, Local>,
    /// vinoPilot's own commands, for the About popup.
    parts: Vec<Part>,
    scanning: Option<Receiver<Scan>>,
    latest: HashMap<Id, Latest>,
    checking: Option<Receiver<HashMap<Id, Latest>>>,
    /// The way each not-installed card will install from.
    choice: HashMap<Id, Channel>,
    /// The card asking "really uninstall?".
    confirm: Option<Id>,
    job: Option<Job>,
    /// Whether the job's end has been acted on (a rescan).
    job_settled: bool,
    /// Components `Update all` still has to do, in order.
    queue: Vec<Id>,
    focused: bool,
    /// The card whose remove command was just copied, and when.
    copied: Option<(Id, f64)>,
    /// This window is the Flatpak's, started from where Flatpak keeps it:
    /// it restarts through `flatpak run`, having no command on `PATH`.
    flatpak: bool,
}

impl Pilot {
    /// The window for this machine: starts the local scan.
    pub fn new(ctx: &egui::Context) -> Self {
        let mut pilot = Pilot::with(Config::load(), HashMap::new(), HashMap::new());
        pilot.flatpak = std::env::current_exe().is_ok_and(|exe| in_flatpak(&exe));
        pilot.rescan(ctx);
        pilot
    }

    /// A window showing exactly `local` and `latest`, with no scan: for the
    /// screenshots.
    pub fn with(config: Config, local: HashMap<Id, Local>, latest: HashMap<Id, Latest>) -> Self {
        Pilot {
            source_field: tilde(&config.source_root),
            config,
            local,
            parts: Vec::new(),
            scanning: None,
            latest,
            checking: None,
            choice: HashMap::new(),
            confirm: None,
            job: None,
            job_settled: true,
            queue: Vec::new(),
            focused: false,
            copied: None,
            flatpak: false,
        }
    }

    /// The same window, with `parts` as the commands found: for the tests.
    pub fn with_parts(mut self, parts: Vec<Part>) -> Self {
        self.parts = parts;
        self
    }

    fn paths(&self) -> Paths {
        self.config.paths()
    }

    fn rescan(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let paths = self.paths();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((detect::scan(&paths), detect::parts(&paths)));
            ctx.request_repaint();
        });
        self.scanning = Some(rx);
    }

    fn check(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let paths = self.paths();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(remote::check(&paths));
            ctx.request_repaint();
        });
        self.checking = Some(rx);
    }

    fn busy(&self) -> bool {
        self.job.as_ref().is_some_and(Job::running)
    }

    fn start(&mut self, ctx: &egui::Context, id: Id, channel: Channel, action: Action) {
        let steps = catalog::steps(id, channel, action);
        if steps.is_empty() {
            return;
        }
        let env = Env::for_id(id, &self.paths());
        let ctx = ctx.clone();
        let notify = Arc::new(move |_: Option<&str>| ctx.request_repaint());
        self.job = Some(Job::start(id, channel, action, steps, env, notify));
        self.job_settled = false;
        self.confirm = None;
    }

    fn update(&self, id: Id) -> Update {
        status::update(
            self.local.get(&id).and_then(|l| l.installed.as_ref()),
            self.latest.get(&id),
        )
    }

    /// What Update all does: the cards. vinoPilot updates from the header.
    fn updatable(&self) -> Vec<Id> {
        CARDS
            .into_iter()
            .filter(|id| matches!(self.update(*id), Update::Available { .. }))
            .collect()
    }

    /// Takes whatever the background threads have finished.
    fn poll(&mut self, ctx: &egui::Context) {
        if let Some((found, parts)) = self.scanning.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.local = found;
            self.parts = parts;
            self.scanning = None;
        }
        if let Some(found) = self.checking.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.latest = found;
            self.checking = None;
        }
        let finished = self.job.as_ref().map(Job::outcome);
        if !self.job_settled && finished.as_ref().is_some_and(|o| *o != Outcome::Running) {
            self.job_settled = true;
            if finished != Some(Outcome::Done) {
                self.queue.clear();
            }
            // A checkout may have moved, so its "behind" is stale too.
            if self
                .job
                .as_ref()
                .is_some_and(|j| j.channel == Channel::Source)
                && !self.latest.is_empty()
            {
                self.check(ctx);
            }
            self.rescan(ctx);
        }
        // Update all: the next one once the scan after the last has landed.
        if !self.busy() && self.job_settled && self.scanning.is_none() && !self.queue.is_empty() {
            let id = self.queue.remove(0);
            if let Some(installed) = self.local.get(&id).and_then(|l| l.installed.clone()) {
                self.start(ctx, id, installed.channel, Action::Update);
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let narrow = ui.available_width() < 620.0;
        ui.horizontal(|ui| {
            widgets::title(ui, "vinoPilot");
            self.version_button(ui);
            self.about_button(ui);
            if ui
                .add(Button::ghost(vino_ui::family::SITE_LABEL).small())
                .on_hover_text(vino_ui::family::SITE)
                .clicked()
                && let Err(e) = vino_ui::family::open(vino_ui::family::SITE)
            {
                eprintln!("vinopilot: couldn't open {}: {e}", vino_ui::family::SITE);
            }
            if !narrow {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.actions(ui);
                });
            }
        });
        if narrow {
            ui.horizontal(|ui| self.actions(ui));
        }
        widgets::muted(
            ui,
            "Each app installs through its own installer, which checks what it downloads.",
        );
        ui.add_space(6.0);
    }

    /// The window's version as a button: it checks for a newer vinoPilot,
    /// then updates it (through the same steps as the cards, for however it's
    /// installed), then restarts into the new one.
    fn version_button(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let busy = self.busy();
        let installed = self
            .local
            .get(&Id::App(vino_ui::App::Pilot))
            .and_then(|l| l.installed.clone());
        if let Some(newer) = self.updated_to(installed.as_ref()) {
            if ui
                .add(Button::primary(format!("Restart into {newer}")).small())
                .on_hover_text(format!(
                    "vinoPilot {newer} is installed; this window is {VERSION}"
                ))
                .clicked()
            {
                restart(&ctx, self.flatpak);
            }
            return;
        }
        if self.checking.is_some() {
            ui.add_enabled(
                false,
                Button::ghost(format!("{VERSION} · checking…")).small(),
            );
            return;
        }
        let how = installed.as_ref().map_or_else(
            || "This window isn't an installed vinoPilot".to_string(),
            |i| {
                format!(
                    "Installed from {}",
                    i.channel.label(Id::App(vino_ui::App::Pilot))
                )
            },
        );
        match (self.update(Id::App(vino_ui::App::Pilot)), installed) {
            (Update::Available { to }, Some(installed)) => {
                if ui
                    .add_enabled(
                        !busy,
                        Button::primary(format!("{VERSION} · update to {to}")).small(),
                    )
                    .on_hover_text(format!("Update vinoPilot to {to}. {how}."))
                    .clicked()
                {
                    self.queue.clear();
                    self.start(
                        &ctx,
                        Id::App(vino_ui::App::Pilot),
                        installed.channel,
                        Action::Update,
                    );
                }
            }
            (update, _) => {
                let (label, hover) = match update {
                    Update::UpToDate => (format!("{VERSION} · up to date"), "Check again".into()),
                    Update::Unknown { reason: Some(why) } => {
                        (VERSION.to_string(), format!("{why}. Check again"))
                    }
                    _ => (VERSION.to_string(), "Check for a newer vinoPilot".into()),
                };
                if ui
                    .add(Button::ghost(label).small())
                    .on_hover_text(format!("{hover}. {how}."))
                    .clicked()
                {
                    self.check(&ctx);
                }
            }
        }
    }

    /// ⓘ, always there: a popup with this window's version, how vinoPilot is
    /// installed, and which of its commands are.
    fn about_button(&self, ui: &mut egui::Ui) {
        let info = ui
            .add(Button::ghost("").icon(vino_ui::Icon::Info).small())
            .on_hover_text("About vinoPilot");
        info.ctx.accesskit_node_builder(info.id, |node| {
            node.set_label("About vinoPilot");
        });
        egui::Popup::from_toggle_button_response(&info)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        self.about(ui);
                    });
            });
    }

    fn about(&self, ui: &mut egui::Ui) {
        let muted = theme::palette(ui.ctx()).muted;
        let dim = |ui: &mut egui::Ui, text: &str| {
            ui.add(egui::Label::new(RichText::new(text).color(muted)).extend())
        };
        let local = self
            .local
            .get(&Id::App(vino_ui::App::Pilot))
            .cloned()
            .unwrap_or_default();
        ui.label(
            RichText::new(format!("vinoPilot {VERSION}"))
                .strong()
                .size(15.0),
        );
        widgets::muted(ui, "The version of this window");
        ui.add_space(6.0);
        egui::Grid::new("about-vinopilot")
            .num_columns(2)
            .spacing(vec2(16.0, 6.0))
            .show(ui, |ui| {
                dim(ui, "Installed from");
                match &local.installed {
                    Some(i) => {
                        let label = i.channel.label(Id::App(vino_ui::App::Pilot));
                        match i.version.as_deref() {
                            Some(v) if v != VERSION => ui.label(format!("{label}, {v}")),
                            _ => ui.label(label),
                        };
                    }
                    None if self.scanning.is_some() => {
                        dim(ui, "Looking…");
                    }
                    None => {
                        dim(ui, "Not found in ~/.local/bin, ~/.cargo/bin or a package");
                    }
                }
                ui.end_row();
                if let Some(c) = &local.checkout {
                    dim(ui, "Checkout");
                    ui.label(format!("{} ({})", tilde(&c.path), c.describe));
                    ui.end_row();
                }
                dim(ui, "Updates");
                match self.update(Id::App(vino_ui::App::Pilot)) {
                    _ if self.checking.is_some() => dim(ui, "Checking…"),
                    Update::Available { to } => ui.label(format!("{to} available")),
                    Update::UpToDate => ui.label("Up to date"),
                    Update::Unknown { reason: Some(why) } => dim(ui, &why),
                    Update::Unknown { reason: None } => dim(ui, "Not checked yet"),
                };
                ui.end_row();
                for part in &self.parts {
                    dim(ui, part_name(part.command));
                    match &part.path {
                        Some(path) => {
                            let version = part.version.as_deref().unwrap_or("version unknown");
                            ui.label(format!("{} ({version})", tilde(path)));
                        }
                        None if part.command == "vinopilot-tui" => {
                            dim(ui, "Not installed: uv tool install vinopilot");
                        }
                        None => {
                            dim(ui, "Not installed");
                        }
                    }
                    ui.end_row();
                }
            });
    }

    /// The version vinoPilot's own update just installed, if it isn't this
    /// window's.
    fn updated_to(&self, installed: Option<&Installed>) -> Option<String> {
        let job = self.job.as_ref()?;
        let done = job.id == Id::App(vino_ui::App::Pilot)
            && job.action == Action::Update
            && job.outcome() == Outcome::Done;
        let version = installed?.version.clone()?;
        (done && self.scanning.is_none() && version != VERSION).then_some(version)
    }

    /// Check for updates and Update all. In a right-to-left layout the
    /// order flips, so Update all always sits at the outer edge.
    fn actions(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
        let updatable = self.updatable();
        let busy = self.busy();
        let update_all = |ui: &mut egui::Ui, queue: &mut Vec<Id>| {
            ui.add_enabled_ui(!busy, |ui| {
                if !updatable.is_empty()
                    && ui
                        .add(Button::primary(format!("Update all ({})", updatable.len())).small())
                        .clicked()
                {
                    *queue = updatable.clone();
                }
            });
        };
        if rtl {
            update_all(ui, &mut self.queue);
        }
        if self.checking.is_some() {
            widgets::chip(ui, ChipKind::Neutral, "Checking…");
            widgets::spinner(ui);
        } else if ui
            .add(
                Button::secondary("Check for updates")
                    .icon(vino_ui::Icon::Refresh)
                    .small(),
            )
            .clicked()
        {
            self.check(&ctx);
        }
        if !rtl {
            update_all(ui, &mut self.queue);
        }
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let columns = if width + GAP >= COLUMNS as f32 * (CARD_MIN + GAP) {
            COLUMNS
        } else {
            1
        };
        let card = (width - GAP * (columns - 1) as f32) / columns as f32;
        for chunk in CARDS.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for id in chunk {
                    ui.allocate_ui_with_layout(
                        vec2(card, CARD_HEIGHT),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.card(ui, *id, card),
                    );
                }
            });
            ui.add_space(GAP);
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, id: Id, width: f32) {
        let ctx = ui.ctx().clone();
        let p = theme::palette(&ctx);
        let local = self.local.get(&id).cloned().unwrap_or_default();
        let update = self.update(id);
        let confirming = self.confirm == Some(id);
        let margin = 14.0;
        let busy = self.busy();
        let mut intent: Option<(Channel, Action)> = None;
        let mut confirm = self.confirm;
        let channels = id.channels();
        let choice = self.choice.entry(id).or_insert(channels[0]);
        let looking = self.scanning.is_some() && self.local.is_empty();
        let now = ctx.input(|i| i.time);
        let copied_now = self
            .copied
            .is_some_and(|(copied, at)| copied == id && now - at < COPIED_FOR);
        if copied_now {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(COPIED_FOR));
        }
        let mut copy_clicked = false;
        let options: Vec<(Channel, &str)> = channels.iter().map(|c| (*c, c.label(id))).collect();
        let ways_above = channels.len() > 3 && width - 2.0 * margin - 2.0 < WAYS_BESIDE;
        let install_from = format!("Install {} from", id.name());

        egui::Frame::new()
            .fill(if confirming { p.error_tint } else { p.surface })
            .stroke(egui::Stroke::new(1.0, p.rule))
            .corner_radius(widgets::corner(10.0))
            .inner_margin(egui::Margin::same(margin as i8))
            .show(ui, |ui| {
                ui.set_width(width - 2.0 * margin - 2.0);
                ui.set_height(CARD_HEIGHT - 2.0 * margin - 2.0);
                ui.spacing_mut().item_spacing.y = 4.0;

                ui.horizontal(|ui| {
                    let name = ui.label(
                        RichText::new(id.name())
                            .size(15.0)
                            .family(theme::semibold()),
                    );
                    a11y::heading(&name, id.name(), 3);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(installed) = &local.installed {
                            widgets::tag(ui, Tone::Neutral, installed.channel.label(id));
                        }
                        if id.needs_root() {
                            widgets::tag(ui, Tone::Neutral, "system").on_hover_text(
                                "Installs system-wide, so it asks for your password",
                            );
                        }
                        if let Some(s) = local.shadowed.first() {
                            widgets::tag(ui, Tone::Warn, "2 copies").on_hover_text(format!(
                                "{} runs; {} is another install it hides.\n\
                                 Uninstall and reinstall to keep one.",
                                tilde(&s.runs),
                                tilde(&s.hidden)
                            ));
                        }
                        if let Some(system) = &local.system_package
                            && distro_tag(ui, system, copied_now)
                        {
                            ui.ctx().copy_text(system.remove.clone());
                            copy_clicked = true;
                        }
                    });
                });
                match &local.installed {
                    // Confirming: what uninstalling removes and keeps, in
                    // place of the blurb.
                    Some(installed) if confirming => {
                        let note = catalog::uninstall_note(id, installed.channel);
                        ui.add(
                            egui::Label::new(RichText::new(note).size(12.5).color(p.error)).wrap(),
                        );
                    }
                    _ => {
                        widgets::muted(ui, id.blurb());
                    }
                }
                ui.add_space(2.0);
                status_line(ui, id, &local, &update, looking);
                // The note says where the checkout goes, in its place.
                if let Some(checkout) = local.checkout.as_ref().filter(|_| !confirming) {
                    let text = format!("{} · {}", tilde(&checkout.path), checkout.describe);
                    ui.add(
                        egui::Label::new(RichText::new(&text).size(12.0).color(p.muted)).truncate(),
                    )
                    .on_hover_text(format!("Source checkout: {text}"));
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_enabled_ui(!busy && !looking, |ui| {
                        ui.horizontal_wrapped(|ui| match &local.installed {
                            None => {
                                if ui.add(Button::primary("Install").small()).clicked() {
                                    intent = Some((*choice, Action::Install));
                                }
                                if channels.len() > 1 && !ways_above {
                                    widgets::segmented(ui, &install_from, choice, &options);
                                }
                            }
                            Some(installed) if confirming => {
                                let label = if id == Id::Ui {
                                    "Move to trash"
                                } else {
                                    "Uninstall"
                                };
                                if ui.add(Button::destructive(label).small()).clicked() {
                                    intent = Some((installed.channel, Action::Uninstall));
                                }
                                if ui.add(Button::ghost("Cancel").small()).clicked() {
                                    confirm = None;
                                }
                            }
                            Some(installed) => {
                                let button = if matches!(update, Update::Available { .. }) {
                                    Button::primary("Update")
                                } else {
                                    Button::secondary("Update")
                                };
                                if ui.add(button.small()).clicked() {
                                    intent = Some((installed.channel, Action::Update));
                                }
                                let label = if id == Id::Ui {
                                    "Trash checkout…"
                                } else {
                                    "Uninstall…"
                                };
                                if ui.add(Button::ghost(label).small()).clicked() {
                                    confirm = Some(id);
                                }
                            }
                        });
                        // Bottom-up: this lands above the row just drawn.
                        if local.installed.is_none() && ways_above {
                            widgets::segmented(ui, &install_from, choice, &options);
                        }
                    });
                });
            });

        self.confirm = confirm;
        if copy_clicked {
            self.copied = Some((id, now));
        }
        if let Some((channel, action)) = intent {
            self.queue.clear();
            self.start(&ctx, id, channel, action);
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        widgets::group_label(ui, "Settings");
        let help = format!(
            "Source installs clone here, e.g. {}",
            tilde(&self.paths().checkout(Id::App(vino_ui::App::Search)))
        );
        widgets::group(ui, |ui| {
            Row::new("Source folder").help(&help).show(ui, |ui, named| {
                let wanted = config::expand(self.source_field.trim(), &config::home());
                if ui
                    .add_enabled(
                        wanted != self.config.source_root && !self.busy(),
                        Button::secondary("Save").small(),
                    )
                    .clicked()
                {
                    self.config.source_root = wanted;
                    if let Err(e) = self.config.save() {
                        eprintln!("vinopilot: couldn't save the config: {e}");
                    }
                    self.rescan(&ctx);
                }
                named(ui.add(widgets::field(
                    &mut self.source_field,
                    "~/Development",
                    220.0,
                )));
            });
        });
    }

    fn log(&mut self, ui: &mut egui::Ui) {
        let Some(job) = &self.job else { return };
        let outcome = job.outcome();
        let title = format!(
            "{} {} from {}",
            job.action.verb(),
            job.id.name(),
            job.channel.label(job.id)
        );
        let mut close = false;
        ui.horizontal(|ui| {
            match &outcome {
                Outcome::Running => {
                    widgets::spinner(ui);
                    a11y::live(&widgets::chip(ui, ChipKind::Neutral, &title))
                }
                Outcome::Done => {
                    a11y::live(&widgets::chip(ui, ChipKind::Ok, &format!("{title}: done")))
                }
                Outcome::Stopped => a11y::live(&widgets::chip(
                    ui,
                    ChipKind::Warn,
                    &format!("{title}: stopped"),
                )),
                Outcome::Failed(_) => a11y::live(&widgets::chip(
                    ui,
                    ChipKind::Err,
                    &format!("{title}: failed"),
                )),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if outcome == Outcome::Running {
                    if ui.add(Button::secondary("Stop").small()).clicked() {
                        job.stop();
                        self.queue.clear();
                    }
                } else if ui.add(Button::ghost("Close").small()).clicked() {
                    close = true;
                }
            });
        });
        if let Outcome::Failed(why) = &outcome {
            widgets::banner(ui, Tone::Error, why, |_| {});
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in job.lines() {
                    ui.label(RichText::new(line).monospace().size(12.0));
                }
            });
        if close {
            self.job = None;
        }
    }

    /// The whole window.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        let p = theme::palette(&ctx);

        if self.job.is_some() {
            egui::Panel::bottom("log")
                .resizable(true)
                .default_size(220.0)
                .frame(
                    egui::Frame::NONE
                        .fill(p.surface)
                        .inner_margin(egui::Margin::same(12)),
                )
                .show(ui, |ui| self.log(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(p.bg).inner_margin(egui::Margin {
                left: widgets::PAGE_INSET as i8,
                right: 0,
                top: 20,
                bottom: 0,
            }))
            .show(ui, |ui| {
                // Above the scroll area, so it stays put; as wide as the
                // cards, which sit inside the scroll bar and its gap.
                ui.scope(|ui| {
                    let bar = ui.spacing().scroll.allocated_width();
                    let inside = ui.available_width() - bar - widgets::SCROLL_GAP;
                    ui.set_max_width(inside.min(PAGE_MAX));
                    self.header(ui);
                });
                ui.add_space(6.0);
                widgets::page_scroll(ui, "vinoPilot", PAGE_MAX, |ui| {
                    self.grid(ui);
                    self.settings(ui);
                });
            });
    }
}

/// How long a distro tag says "copied".
const COPIED_FOR: f64 = 1.5;

/// `RPM` in the warn tag's colours, as a button: a distro package vinoPilot
/// doesn't remove, which copies the command that does. True when clicked.
fn distro_tag(ui: &mut egui::Ui, system: &SystemPackage, copied: bool) -> bool {
    let p = theme::palette(ui.ctx());
    let text = if copied { "copied" } else { system.kind };
    let button = egui::Button::new(
        RichText::new(text.to_uppercase())
            .size(10.5)
            .family(theme::bold())
            .color(p.warn),
    )
    .fill(p.warn_tint)
    .stroke(egui::Stroke::NONE)
    .corner_radius(4.0)
    .min_size(vec2(0.0, 16.0));
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = vec2(6.0, 0.0);
            ui.spacing_mut().interact_size.y = 16.0;
            ui.add(button)
        })
        .inner
        .on_hover_text(format!(
            "{} came from your package manager, which vinoPilot doesn't uninstall.\n\
             Click to copy the command that does:\n{}",
            system.packages.join(", "),
            system.remove
        ));
    let name = if copied {
        format!(
            "Copied the command that removes the {} package",
            system.kind
        )
    } else {
        format!("Copy the command that removes the {} package", system.kind)
    };
    response
        .ctx
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    response.clicked()
}

/// "1.4.0 · v1.4.1 available", coloured by whether it needs the user.
fn status_line(ui: &mut egui::Ui, id: Id, local: &Local, update: &Update, looking: bool) {
    let p = theme::palette(ui.ctx());
    let Some(Installed { version, .. }) = &local.installed else {
        widgets::small(
            ui,
            if looking {
                "Looking…"
            } else {
                "Not installed"
            },
        );
        return;
    };
    let version = version.as_deref().unwrap_or("installed");
    let (text, color) = match update {
        Update::Available { to } => (format!("{version} · {to} available"), p.warn),
        Update::UpToDate => (format!("{version} · up to date"), p.accent),
        Update::Unknown { reason: Some(why) } => (format!("{version} · {why}"), p.muted),
        Update::Unknown { reason: None } => (version.to_string(), p.text),
    };
    let shown = ui.add(egui::Label::new(RichText::new(text).size(13.0).color(color)).truncate());
    if id == Id::App(vino_ui::App::Pilot) && version != VERSION {
        shown.on_hover_text(format!("This window is {VERSION}"));
    }
}

/// Whether `exe` is in a Flatpak's files: `<installation>/app/<id>/<arch>/
/// <branch>/<commit>/files/libexec/vinopilot/vinopilot-gui`, where
/// packaging/flatpak/host-run.sh starts it on the host.
fn in_flatpak(exe: &std::path::Path) -> bool {
    let mut parts = exe.components().map(|c| c.as_os_str());
    parts.any(|c| c == "app") && parts.next().is_some_and(|c| c == desktop::APP_ID)
}

/// What each of vinoPilot's commands is, for the About popup.
fn part_name(command: &str) -> &'static str {
    match command {
        "vinopilot" => "Command",
        "vinopilot-gui" => "Window",
        "vinopilot-tui" => "Terminal UI",
        _ => "Other",
    }
}

/// Starts the installed window and closes this one.
fn restart(ctx: &egui::Context, flatpak: bool) {
    let mut new = if flatpak {
        let mut run = vinopilot_core::command("flatpak");
        run.args(["run", desktop::APP_ID]);
        run
    } else {
        vinopilot_core::command("vinopilot-gui")
    };
    match new.spawn() {
        Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        Err(e) => eprintln!("vinopilot: couldn't start the new vinopilot-gui: {e}"),
    }
}

/// `/home/k/x` as `~/x`.
fn tilde(path: &std::path::Path) -> String {
    let home = config::home();
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

impl eframe::App for Pilot {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The desktop's light/dark switch may have flipped while the window was in the background.
        let focused = ui.input(|i| i.focused);
        if focused && !self.focused {
            theme::follow_desktop(ui.ctx());
        }
        self.focused = focused;
        Pilot::ui(self, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_flatpak_is_told_by_where_its_binary_is() {
        for dir in ["/var/lib/flatpak", "/home/k/.local/share/flatpak"] {
            let exe = format!(
                "{dir}/app/{}/x86_64/master/0a1b/files/libexec/vinopilot/vinopilot-gui",
                desktop::APP_ID
            );
            assert!(in_flatpak(Path::new(&exe)), "{exe}");
        }
        assert!(!in_flatpak(Path::new("/usr/bin/vinopilot-gui")));
        assert!(!in_flatpak(Path::new("/home/k/app/vinopilot-gui")));
    }
}

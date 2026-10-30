//! The main window: hosts on the left; the video, toolbar and status bar on
//! the right. It renders the core's state and forwards input while
//! capturing.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use kvms_core::controller::{Controller, ControllerEvent, ControllerObserver, ControllerState};
use kvms_core::keymap::linux_to_hid;
use kvms_core::profiles::HostProfile;
use kvms_core::video::{FrameOutput, NativeSurface, ScaleMode};
use kvms_core::{ConnectionState, KeyResult, MouseButton};

use crate::capture::Capture;
use crate::video::VideoView;

pub const APP_ID: &str = "io.github.misterkvms.MiSTerKVMS";

const CSS: &str = "
.video-area { background-color: black; }
.video-area statuspage { color: white; }
.stats { font-family: monospace; font-size: 9pt; color: #4ce06a; }
.stats-box { background-color: rgba(0, 0, 0, 0.6); border-radius: 6px; padding: 8px; }
.hint { background-color: rgba(0, 0, 0, 0.75); color: white; border-radius: 10px; padding: 10px 16px; }
.hint .warning { color: #ffd54f; }
.status-dot.connected { color: #33cc55; }
.status-dot.connecting { color: #e6b800; }
.status-dot.disconnected { color: #e01b24; }
.recording { color: #e01b24; }
.host-picture { background-color: black; border-radius: 6px; }
.host-picture image { color: rgba(255, 255, 255, 0.4); }
";

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

/// The window's state, reachable from core callbacks (which arrive on
/// other threads and hop to the main loop first).
fn ui() -> Option<Rc<Ui>> {
    UI.with(|u| u.borrow().clone())
}

struct Observer;

impl ControllerObserver for Observer {
    fn on_event(&self, event: ControllerEvent) {
        glib::MainContext::default().invoke(move || {
            if let Some(ui) = ui() {
                ui.on_event(event);
            }
        });
    }
}

pub struct Window;

impl Window {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(app: &adw::Application) -> adw::ApplicationWindow {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("display"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let ui = Ui::build(app);
        UI.with(|u| *u.borrow_mut() = Some(ui.clone()));
        ui.window.clone()
    }
}

/// A host's row in the sidebar: what previews update.
struct HostRow {
    picture: gtk::Picture,
    frame: gtk::Overlay,
    placeholder: gtk::Image,
    caption: gtk::Label,
    /// Shown until the core knows what the host is running.
    fallback: String,
    version: Cell<u64>,
}

pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub core: Arc<Controller>,
    frames: Arc<FrameOutput>,
    frame_pending: Arc<AtomicBool>,
    toasts: adw::ToastOverlay,
    host_list: gtk::ListBox,
    rows: RefCell<HashMap<String, HostRow>>,
    /// No hosts yet: what to do, in the session area (the list stays empty).
    no_hosts: adw::StatusPage,
    profiles: RefCell<Vec<HostProfile>>,
    state: RefCell<ControllerState>,
    /// Set while the host list is refilled, so that doesn't open hosts.
    updating_list: Cell<bool>,
    /// The first-capture dialog is up (connecting and a click can both ask).
    asking_intro: Cell<bool>,
    title: adw::WindowTitle,
    menu_button: gtk::Button,
    capture_button: gtk::Button,
    zoom_label: gtk::Button,
    record_button: gtk::Button,
    mute_button: gtk::ToggleButton,
    video: VideoView,
    placeholder: adw::StatusPage,
    capture_hint: gtk::Box,
    hint_note: gtk::Label,
    stats_box: gtk::Box,
    stats_label: gtk::Label,
    stats_pin: gtk::Button,
    status_dot: gtk::Label,
    status_label: gtk::Label,
    recording_label: gtk::Label,
    last_recording: gtk::Button,
    game_label: gtk::Label,
    capture_label: gtk::Label,
    /// The capture hint's "Press … together to release" line.
    hint_keys: gtk::Label,
    /// Connection Info: the open host's connection in plain words.
    info_button: gtk::MenuButton,
    info_grid: gtk::Grid,
    capture: RefCell<Capture>,
    hint_timer: RefCell<Option<glib::SourceId>>,
}

impl Ui {
    fn build(app: &adw::Application) -> Rc<Self> {
        let frame_pending = Arc::new(AtomicBool::new(false));
        let pending = frame_pending.clone();
        let frames = FrameOutput::new(move || {
            // One wake-up per batch of frames; the newest is drawn.
            if !pending.swap(true, Ordering::AcqRel) {
                glib::MainContext::default().invoke(|| {
                    if let Some(ui) = ui() {
                        ui.on_frame();
                    }
                });
            }
        });
        let core = Controller::new(Arc::new(Observer));
        core.set_frame_output(Some(frames.clone()));
        // `misterkvms --auto-capture` (main.rs keeps GTK from seeing it):
        // capture by itself this run, whatever the setting.
        if std::env::args().any(|a| a == "--auto-capture") {
            core.force_auto_capture();
        }
        core.set_host_previews(true);
        kvms_core::log(&format!("settings: {}", core.settings_path()));

        // Sidebar: hosts.
        let host_list = gtk::ListBox::new();
        host_list.add_css_class("navigation-sidebar");
        let add_button = gtk::Button::from_icon_name("list-add-symbolic");
        add_button.set_tooltip_text(Some("Add a host"));
        add_button.set_action_name(Some("win.add-host"));
        let sidebar_header = adw::HeaderBar::new();
        sidebar_header.set_title_widget(Some(&adw::WindowTitle::new("Hosts", "")));
        sidebar_header.pack_end(&add_button);
        let add_first = gtk::Button::with_label("Add Host…");
        add_first.set_action_name(Some("win.add-host"));
        add_first.set_halign(gtk::Align::Center);
        add_first.add_css_class("pill");
        let no_hosts = adw::StatusPage::builder()
            .icon_name("video-display-symbolic")
            .title("No Hosts")
            .description("Add a MiSTer and the capture server stream that carries its video.")
            .child(&add_first)
            .visible(false)
            .build();
        let sidebar = adw::ToolbarView::new();
        sidebar.add_top_bar(&sidebar_header);
        sidebar.set_content(Some(&gtk::ScrolledWindow::builder().child(&host_list).vexpand(true).build()));

        // Header bar.
        let title = adw::WindowTitle::new("MiSTerKVMS", "");
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let menu_button = gtk::Button::with_label("F12 Menu");
        menu_button.set_action_name(Some("win.mister-menu"));
        menu_button.set_tooltip_text(Some("Sends F12, which opens the MiSTer menu (OSD). You can also press F12 while capturing."));
        let capture_button = gtk::Button::from_icon_name("input-mouse-symbolic");
        capture_button.set_action_name(Some("win.capture"));
        header.pack_start(&menu_button);
        header.pack_start(&capture_button);
        let info_grid = gtk::Grid::builder().column_spacing(12).row_spacing(6).margin_top(8).margin_bottom(8).margin_start(8).margin_end(8).build();
        let info_popover = gtk::Popover::builder().child(&info_grid).build();
        let info_button = gtk::MenuButton::builder()
            .icon_name("dialog-information-symbolic")
            .popover(&info_popover)
            .tooltip_text("Where the picture and input go and how the connection is doing (Ctrl+Alt+I)")
            .build();
        header.pack_start(&info_button);

        let main_menu = gio::Menu::new();
        let session_section = gio::Menu::new();
        session_section.append(Some("Send Ctrl+Alt+Del"), Some("win.ctrl-alt-del"));
        session_section.append(Some("Type Text…"), Some("win.type-text"));
        session_section.append(Some("Edit Host…"), Some("win.edit-host"));
        session_section.append(Some("Show Last Recording"), Some("win.show-recording"));
        session_section.append(Some("Show Screenshots"), Some("win.show-screenshots"));
        main_menu.append_section(None, &session_section);
        let stats_section = gio::Menu::new();
        stats_section.append(Some("Show Stats"), Some("win.show-stats"));
        stats_section.append(Some("Pin Stats Overlay"), Some("win.pin-stats"));
        main_menu.append_section(None, &stats_section);
        let app_section = gio::Menu::new();
        app_section.append(Some("Capture When a Host Connects"), Some("win.auto-capture"));
        let release_keys = gio::Menu::new();
        for o in core.release_chord_options() {
            release_keys.append(Some(&o.name), Some(&format!("win.release-chord::{}", o.id)));
        }
        app_section.append_submenu(Some("Release Keys"), &release_keys);
        app_section.append(Some("Core and Time on Screenshots"), Some("win.screenshot-stamp"));
        app_section.append(Some("Export Hosts…"), Some("win.export"));
        app_section.append(Some("Import Hosts…"), Some("win.import"));
        app_section.append(Some("About MiSTerKVMS"), Some("win.about"));
        main_menu.append_section(None, &app_section);
        let main_menu_button = gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&main_menu).build();
        main_menu_button.set_tooltip_text(Some("Main menu"));

        let view_menu = gio::Menu::new();
        let scale_section = gio::Menu::new();
        scale_section.append(Some("Fit"), Some("win.scale::fit"));
        scale_section.append(Some("Integer (sharpest)"), Some("win.scale::integer"));
        scale_section.append(Some("1:1"), Some("win.scale::native"));
        view_menu.append_section(Some("Scale"), &scale_section);
        let options = gio::Menu::new();
        options.append(Some("Smooth Scaling"), Some("win.smooth"));
        options.append(Some("Auto-crop Borders"), Some("win.auto-crop"));
        options.append(Some("Predictive Cursor"), Some("win.predictive"));
        view_menu.append_section(None, &options);
        let view_button = gtk::MenuButton::builder()
            .icon_name("video-display-symbolic")
            .menu_model(&view_menu)
            .tooltip_text("Scale and picture options")
            .build();

        let zoom_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        zoom_box.add_css_class("linked");
        let zoom_out = gtk::Button::from_icon_name("zoom-out-symbolic");
        zoom_out.set_action_name(Some("win.zoom-out"));
        zoom_out.set_tooltip_text(Some("Zoom out (Ctrl+−)"));
        let zoom_label = gtk::Button::with_label("100%");
        zoom_label.set_action_name(Some("win.zoom-reset"));
        zoom_label.set_tooltip_text(Some("Reset zoom (Ctrl+0)"));
        let zoom_in = gtk::Button::from_icon_name("zoom-in-symbolic");
        zoom_in.set_action_name(Some("win.zoom-in"));
        zoom_in.set_tooltip_text(Some("Zoom in (Ctrl+=)"));
        zoom_box.append(&zoom_out);
        zoom_box.append(&zoom_label);
        zoom_box.append(&zoom_in);

        let copy_shot = gtk::Button::from_icon_name("camera-photo-symbolic");
        copy_shot.set_action_name(Some("win.copy-screenshot"));
        copy_shot.set_tooltip_text(Some("Copy the picture to the clipboard (Ctrl+Shift+C)"));
        let save_shot = gtk::Button::from_icon_name("document-save-symbolic");
        save_shot.set_action_name(Some("win.save-screenshot"));
        save_shot.set_tooltip_text(Some("Save the picture to Pictures/MiSTerKVMS (Ctrl+Shift+S)"));
        let record_button = gtk::Button::from_icon_name("media-record-symbolic");
        record_button.set_action_name(Some("win.record"));
        let mute_button = gtk::ToggleButton::new();
        mute_button.set_icon_name("audio-volume-high-symbolic");
        mute_button.set_action_name(Some("win.mute"));
        mute_button.set_tooltip_text(Some("Mute (Ctrl+Shift+M)"));
        header.pack_end(&main_menu_button);
        header.pack_end(&mute_button);
        header.pack_end(&record_button);
        header.pack_end(&save_shot);
        header.pack_end(&copy_shot);
        header.pack_end(&zoom_box);
        header.pack_end(&view_button);

        // Video and overlays.
        let video = VideoView::default();
        let placeholder = adw::StatusPage::builder().icon_name("video-display-symbolic").build();
        placeholder.set_can_target(false);
        let capture_hint = gtk::Box::new(gtk::Orientation::Vertical, 4);
        capture_hint.add_css_class("hint");
        capture_hint.set_halign(gtk::Align::Center);
        capture_hint.set_valign(gtk::Align::Start);
        capture_hint.set_margin_top(12);
        capture_hint.set_can_target(false);
        capture_hint.set_visible(false);
        let hint_title = gtk::Label::new(Some("Keyboard and mouse go to MiSTer"));
        hint_title.add_css_class("heading");
        let hint_keys = gtk::Label::new(None);

        let hint_note = gtk::Label::new(None);
        hint_note.add_css_class("warning");
        hint_note.set_wrap(true);
        hint_note.set_visible(false);
        capture_hint.append(&hint_title);
        capture_hint.append(&hint_keys);
        capture_hint.append(&hint_note);

        let stats_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        stats_box.add_css_class("stats-box");
        stats_box.set_halign(gtk::Align::Start);
        stats_box.set_valign(gtk::Align::End);
        stats_box.set_margin_start(12);
        stats_box.set_margin_bottom(12);
        stats_box.set_visible(false);
        let stats_label = gtk::Label::new(None);
        stats_label.add_css_class("stats");
        stats_label.set_xalign(0.0);
        stats_label.set_valign(gtk::Align::Start);
        let stats_pin = gtk::Button::from_icon_name("view-pin-symbolic");
        stats_pin.add_css_class("flat");
        stats_pin.set_valign(gtk::Align::Start);
        stats_pin.set_action_name(Some("win.pin-stats"));
        stats_box.append(&stats_label);
        stats_box.append(&stats_pin);

        let overlay = gtk::Overlay::new();
        overlay.add_css_class("video-area");
        overlay.set_child(Some(&video));
        overlay.add_overlay(&placeholder);
        overlay.add_overlay(&no_hosts);
        overlay.add_overlay(&capture_hint);
        overlay.add_overlay(&stats_box);

        // Status bar.
        let status = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        status.set_margin_start(12);
        status.set_margin_end(12);
        status.set_margin_top(6);
        status.set_margin_bottom(6);
        let small = |text: &str| {
            let l = gtk::Label::new(Some(text));
            l.add_css_class("caption");
            l
        };
        let status_dot = small("●");
        status_dot.add_css_class("status-dot");
        let status_label = small("Not connected");
        let recording_label = small("");
        recording_label.add_css_class("recording");
        let last_recording = gtk::Button::new();
        last_recording.add_css_class("flat");
        last_recording.add_css_class("caption");
        last_recording.set_action_name(Some("win.show-recording"));
        last_recording.set_visible(false);
        let game_label = small("");
        game_label.add_css_class("dim-label");
        game_label.set_hexpand(true);
        game_label.set_xalign(1.0);
        let capture_label = small("");
        capture_label.add_css_class("dim-label");
        status.append(&status_dot);
        status.append(&status_label);
        status.append(&recording_label);
        status.append(&last_recording);
        status.append(&game_label);
        status.append(&capture_label);

        let content = adw::ToolbarView::new();
        content.add_top_bar(&header);
        content.set_content(Some(&overlay));
        content.add_bottom_bar(&status);

        let split = adw::OverlaySplitView::new();
        split.set_sidebar(Some(&sidebar));
        split.set_content(Some(&content));
        split.set_min_sidebar_width(220.0);
        split.set_max_sidebar_width(280.0);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("MiSTerKVMS")
            .default_width(1280)
            .default_height(800)
            .content(&toasts)
            .icon_name(APP_ID)
            .build();

        let ui = Rc::new(Self {
            window,
            core,
            frames,
            frame_pending,
            toasts,
            host_list,
            rows: RefCell::new(HashMap::new()),
            no_hosts,
            profiles: RefCell::new(Vec::new()),
            state: RefCell::new(ControllerState {
                profile_id: None,
                connection: ConnectionState::Disconnected { reason: "not connected".into() },
                core_running: String::new(),
                game_running: String::new(),
                capturing: false,
                stats_visible: false,
                stats_pinned: false,
                input_stats: None,
                video_stats: None,
                video_error: None,
                display: Default::default(),
                muted: false,
                recording_seconds: None,
                last_recording: None,
                recording_error: None,
                stream: None,
            }),
            updating_list: Cell::new(false),
            asking_intro: Cell::new(false),
            title,
            menu_button,
            capture_button,
            zoom_label,
            record_button,
            mute_button,
            video,
            placeholder,
            capture_hint,
            hint_note,
            stats_box,
            stats_label,
            stats_pin,
            status_dot,
            status_label,
            recording_label,
            last_recording,
            game_label,
            capture_label,
            hint_keys,
            info_button,
            info_grid,
            capture: RefCell::new(Capture::default()),
            hint_timer: RefCell::new(None),
        });
        ui.connect();
        ui.load_profiles();
        ui.render();
        ui
    }

    /// Signals, actions and input controllers.
    fn connect(self: &Rc<Self>) {
        let core = self.core.clone();
        self.video.connect_resized(move |w, h, scale| {
            core.attach_surface(NativeSurface::Offscreen, w, h, scale);
        });

        self.host_list.connect_row_selected(|_, row| {
            let Some(ui) = ui() else { return };
            if ui.updating_list.get() {
                return;
            }
            let id = row.map(|r| r.widget_name().to_string());
            match id {
                Some(id) => {
                    ui.stop_capture();
                    ui.core.open(id);
                }
                None => ui.core.close(),
            }
        });

        // Click the video to capture.
        let click = gtk::GestureClick::new();
        click.connect_pressed(|_, _, _, _| {
            if let Some(ui) = ui() {
                ui.start_capture();
            }
        });
        self.video.add_controller(click);

        // Pointer motion when the compositor doesn't lock the pointer for us.
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(|_, x, y| {
            if let Some(ui) = ui() {
                ui.capture.borrow_mut().motion(x, y);
            }
        });
        self.video.add_controller(motion);

        // Not capturing: Ctrl+wheel zooms, the wheel pans when zoomed in.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        scroll.connect_scroll(|controller, dx, dy| {
            let Some(ui) = ui() else { return glib::Propagation::Proceed };
            if ui.capture.borrow().is_active() {
                return glib::Propagation::Stop;
            }
            let ctrl = controller.current_event_state().contains(gdk::ModifierType::CONTROL_MASK);
            let zoom = ui.state.borrow().display.zoom;
            if ctrl {
                ui.core.zoom_by(1.25f64.powf(-dy));
            } else if zoom > 1.0 {
                ui.core.pan_by(-0.05 * dx / zoom, -0.05 * dy / zoom);
            }
            glib::Propagation::Stop
        });
        self.video.add_controller(scroll);

        // While capturing, every key goes to MiSTer (before GTK shortcuts).
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(|_, _, keycode, _| key(keycode, true));
        keys.connect_key_released(|_, _, keycode, _| {
            key(keycode, false);
        });
        self.window.add_controller(keys);

        // ...and every mouse button.
        let buttons = gtk::EventControllerLegacy::new();
        buttons.set_propagation_phase(gtk::PropagationPhase::Capture);
        buttons.connect_event(|_, event| {
            let Some(ui) = ui() else { return glib::Propagation::Proceed };
            let capture = ui.capture.borrow();
            let Some(session) = capture.session() else { return glib::Propagation::Proceed };
            match event.event_type() {
                gdk::EventType::ButtonPress | gdk::EventType::ButtonRelease => {
                    let down = event.event_type() == gdk::EventType::ButtonPress;
                    let button = event.downcast_ref::<gdk::ButtonEvent>().map_or(0, |b| b.button());
                    let button = match button {
                        1 => Some(MouseButton::Left),
                        2 => Some(MouseButton::Middle),
                        3 => Some(MouseButton::Right),
                        _ => None,
                    };
                    if let Some(b) = button {
                        session.mouse_button(b, down);
                    }
                    glib::Propagation::Stop
                }
                // mrext has no wheel command yet.
                gdk::EventType::Scroll => glib::Propagation::Stop,
                _ => glib::Propagation::Proceed,
            }
        });
        self.window.add_controller(buttons);

        // Release everything when another window takes over.
        self.window.connect_is_active_notify(|w| {
            if !w.is_active() {
                if let Some(ui) = ui() {
                    ui.stop_capture();
                }
            }
        });
        self.window.connect_close_request(|_| {
            if let Some(ui) = ui() {
                ui.stop_capture();
                ui.core.close();
                ui.core.detach_surface();
            }
            UI.with(|u| u.borrow_mut().take());
            glib::Propagation::Proceed
        });

        self.add_actions();
    }

    fn add_actions(self: &Rc<Self>) {
        let w = &self.window;
        let simple = |name: &str, f: fn(&Rc<Ui>)| {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                if let Some(ui) = ui() {
                    f(&ui);
                }
            });
            w.add_action(&action);
        };
        simple("add-host", |ui| ui.edit_host(None));
        simple("edit-host", |ui| {
            let id = ui.state.borrow().profile_id.clone();
            ui.edit_host(id);
        });
        simple("mister-menu", |ui| ui.core.open_menu());
        simple("ctrl-alt-del", |ui| ui.core.send_ctrl_alt_del());
        simple("type-text", |ui| ui.type_text());
        simple("capture", |ui| {
            if ui.capture.borrow().is_active() {
                ui.stop_capture();
            } else {
                ui.start_capture();
            }
        });
        simple("zoom-in", |ui| ui.core.zoom_by(1.25));
        simple("zoom-out", |ui| ui.core.zoom_by(1.0 / 1.25));
        simple("zoom-reset", |ui| ui.core.reset_zoom());
        simple("record", |ui| ui.toggle_recording());
        simple("mute", |ui| {
            let muted = ui.state.borrow().muted;
            ui.core.set_muted(!muted);
        });
        simple("show-stats", |ui| ui.core.flash_stats());
        simple("connection-info", |ui| {
            if ui.state.borrow().profile_id.is_some() {
                ui.info_button.popup();
            }
        });
        // Refreshed every second while it's open.
        if let Some(popover) = self.info_button.popover() {
            popover.connect_show(|popover| {
                if let Some(ui) = ui() {
                    ui.render_info();
                }
                let popover = popover.downgrade();
                glib::timeout_add_seconds_local(1, move || {
                    let (Some(ui), Some(popover)) = (ui(), popover.upgrade()) else { return glib::ControlFlow::Break };
                    if !popover.is_visible() {
                        return glib::ControlFlow::Break;
                    }
                    ui.render_info();
                    glib::ControlFlow::Continue
                });
            });
        }
        simple("pin-stats", |ui| {
            let pinned = ui.state.borrow().stats_pinned;
            ui.core.set_stats_pinned(!pinned);
        });
        simple("show-recording", |ui| {
            if let Some(path) = ui.state.borrow().last_recording.clone() {
                let file = gio::File::for_path(&path);
                gtk::FileLauncher::new(Some(&file)).open_containing_folder(Some(&ui.window), gio::Cancellable::NONE, |_| {});
            }
        });
        simple("copy-screenshot", |ui| ui.copy_screenshot());
        simple("save-screenshot", |ui| ui.save_screenshot());
        simple("show-screenshots", |ui| {
            let dir = screenshot_dir();
            let _ = std::fs::create_dir_all(&dir);
            gtk::FileLauncher::new(Some(&gio::File::for_path(&dir))).launch(Some(&ui.window), gio::Cancellable::NONE, |_| {});
        });
        let stamp = gio::SimpleAction::new_stateful("screenshot-stamp", None, &self.core.screenshot_stamp().to_variant());
        stamp.connect_activate(|action, _| {
            let Some(ui) = ui() else { return };
            let on = !action.state().and_then(|s| s.get::<bool>()).unwrap_or(true);
            ui.core.set_screenshot_stamp(on);
            action.set_state(&on.to_variant());
        });
        w.add_action(&stamp);
        let auto = gio::SimpleAction::new_stateful("auto-capture", None, &self.core.auto_capture().to_variant());
        auto.connect_activate(|action, _| {
            let Some(ui) = ui() else { return };
            let on = !action.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            ui.core.set_auto_capture(on);
            action.set_state(&on.to_variant());
        });
        w.add_action(&auto);
        let chord = gio::SimpleAction::new_stateful(
            "release-chord",
            Some(glib::VariantTy::STRING),
            &self.core.release_chord().id().to_variant(),
        );
        chord.connect_activate(|action, param| {
            let (Some(ui), Some(id)) = (ui(), param.and_then(|p| p.str().map(str::to_string))) else { return };
            ui.core.set_release_chord(kvms_core::ReleaseChord::from_id(&id));
            action.set_state(&id.to_variant());
            if let Some(o) = ui.core.release_chord_options().into_iter().find(|o| o.id == id) {
                ui.toast(&format!("Release with {}: press both together, then let go. {}", o.name, o.note));
            }
        });
        w.add_action(&chord);
        // Hosts' pictures shrink to fit as the window changes.
        for prop in ["default-height", "maximized", "fullscreened"] {
            w.connect_notify_local(Some(prop), |_, _| {
                if let Some(ui) = ui() {
                    ui.size_pictures();
                }
            });
        }
        simple("export", |ui| ui.export_profiles());
        simple("import", |ui| ui.import_profiles());
        simple("about", |ui| crate::about::show(&ui.window));

        let scale = gio::SimpleAction::new_stateful("scale", Some(glib::VariantTy::STRING), &"fit".to_variant());
        scale.connect_activate(|_, param| {
            let (Some(ui), Some(mode)) = (ui(), param.and_then(|p| p.str().map(str::to_string))) else { return };
            let mut d = ui.state.borrow().display.clone();
            d.scale_mode = ScaleMode::from_name(&mode);
            ui.core.set_display(d);
        });
        w.add_action(&scale);
        let toggle = |name: &str, f: fn(&mut kvms_core::DisplaySettings)| {
            let action = gio::SimpleAction::new_stateful(name, None, &false.to_variant());
            action.connect_activate(move |_, _| {
                if let Some(ui) = ui() {
                    let mut d = ui.state.borrow().display.clone();
                    f(&mut d);
                    ui.core.set_display(d);
                }
            });
            w.add_action(&action);
        };
        toggle("smooth", |d| d.smooth = !d.smooth);
        toggle("auto-crop", |d| d.auto_crop = !d.auto_crop);
        toggle("predictive", |d| d.predictive_cursor = !d.predictive_cursor);

        let app = self.window.application().expect("application");
        for (action, accels) in [
            ("win.capture", &["<Control><Shift>k"][..]),
            ("win.type-text", &["<Control><Shift>v"]),
            ("win.show-stats", &["<Control>i"]),
            ("win.connection-info", &["<Control><Alt>i"]),
            ("win.pin-stats", &["<Control><Shift>i"]),
            ("win.record", &["<Control><Shift>r"]),
            ("win.copy-screenshot", &["<Control><Shift>c"]),
            ("win.save-screenshot", &["<Control><Shift>s"]),
            ("win.mute", &["<Control><Shift>m"]),
            ("win.zoom-in", &["<Control>equal", "<Control>plus", "<Control>KP_Add"]),
            ("win.zoom-out", &["<Control>minus", "<Control>KP_Subtract"]),
            ("win.zoom-reset", &["<Control>0"]),
        ] {
            app.set_accels_for_action(action, accels);
        }
    }

    // MARK: Core events and rendering

    fn on_event(self: &Rc<Self>, event: ControllerEvent) {
        match event {
            ControllerEvent::Changed => self.render(),
            ControllerEvent::ProfilesChanged => {
                self.load_profiles();
                self.render();
            }
            ControllerEvent::CaptureRequested => {
                // Auto-capture on connect, if the user is looking at us.
                if self.window.is_active() {
                    self.start_capture();
                }
            }
            ControllerEvent::ReleaseRequested => self.stop_capture(),
            ControllerEvent::PreviewsChanged => self.load_previews(),
        }
    }

    fn on_frame(&self) {
        self.frame_pending.store(false, Ordering::Release);
        if let Some(frame) = self.frames.take() {
            if self.state.borrow().profile_id.is_some() {
                self.video.show_frame(frame);
            }
        }
    }

    fn load_profiles(&self) {
        let profiles = self.core.profiles();
        self.updating_list.set(true);
        while let Some(row) = self.host_list.row_at_index(0) {
            self.host_list.remove(&row);
        }
        let open = self.state.borrow().profile_id.clone();
        let mut rows = HashMap::new();
        for p in &profiles {
            // The host's picture (kept fresh by the core), its name, and
            // what it's running.
            let row = gtk::ListBoxRow::new();
            row.set_widget_name(&p.id);
            let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
            column.set_margin_top(6);
            column.set_margin_bottom(6);
            let picture = gtk::Picture::new();
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_can_shrink(true);
            let placeholder = gtk::Image::from_icon_name("video-display-symbolic");
            placeholder.set_pixel_size(24);
            let frame = gtk::Overlay::new();
            frame.set_child(Some(&picture));
            frame.add_overlay(&placeholder);
            frame.add_css_class("host-picture");
            frame.set_overflow(gtk::Overflow::Hidden);
            let name = gtk::Label::new(Some(&p.name));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let fallback = if p.mister_host.is_empty() { "No address".to_string() } else { p.mister_host.clone() };
            let caption = gtk::Label::new(Some(&fallback));
            caption.set_xalign(0.0);
            caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
            caption.add_css_class("caption");
            caption.add_css_class("dim-label");
            let names = gtk::Box::new(gtk::Orientation::Vertical, 2);
            names.append(&name);
            names.append(&caption);
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            line.append(&names);
            column.append(&frame);
            column.append(&line);
            row.set_child(Some(&column));
            rows.insert(p.id.clone(), HostRow { picture, frame, placeholder, caption, fallback, version: Cell::new(0) });
            let menu = gio::Menu::new();
            menu.append(Some("Edit…"), Some(&format!("win.edit-host-id::{}", p.id)));
            menu.append(Some("Delete"), Some(&format!("win.delete-host::{}", p.id)));
            let more = gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .menu_model(&menu)
                .valign(gtk::Align::Center)
                .build();
            more.add_css_class("flat");
            line.append(&more);
            self.host_list.append(&row);
            if open.as_deref() == Some(p.id.as_str()) {
                self.host_list.select_row(Some(&row));
            }
        }
        self.updating_list.set(false);
        self.no_hosts.set_visible(profiles.is_empty());
        *self.profiles.borrow_mut() = profiles;
        *self.rows.borrow_mut() = rows;
        self.add_row_actions();
        self.size_pictures();
        self.load_previews();
    }

    /// The Connection Info popover's lines (the core's connection_info).
    fn render_info(&self) {
        while let Some(child) = self.info_grid.first_child() {
            self.info_grid.remove(&child);
        }
        for (row, line) in self.core.connection_info().into_iter().enumerate() {
            let label = gtk::Label::new(Some(&line.label));
            label.set_xalign(1.0);
            label.set_valign(gtk::Align::Start);
            label.add_css_class("dim-label");
            let value = gtk::Label::new(Some(&line.value));
            value.set_xalign(0.0);
            value.set_wrap(true);
            value.set_max_width_chars(44);
            value.set_selectable(true);
            self.info_grid.attach(&label, 0, row as i32, 1, 1);
            self.info_grid.attach(&value, 1, row as i32, 1, 1);
        }
    }

    /// The core's pictures and captions for the host list.
    fn load_previews(&self) {
        let rows = self.rows.borrow();
        for p in self.core.host_previews() {
            let Some(row) = rows.get(&p.id) else { continue };
            row.caption.set_text(if p.caption.is_empty() { &row.fallback } else { &p.caption });
            if p.version == row.version.get() {
                continue;
            }
            let Some(jpeg) = p.jpeg else { continue };
            match gdk::Texture::from_bytes(&glib::Bytes::from_owned(jpeg)) {
                Ok(texture) => {
                    row.picture.set_paintable(Some(&texture));
                    row.placeholder.set_visible(false);
                    row.version.set(p.version);
                }
                Err(e) => kvms_core::log(&format!("preview: {e}")),
            }
        }
    }

    /// Hosts' pictures as big as the sidebar's width allows (4:3) while one
    /// or two hosts fit, smaller as more are added so they still fit, down
    /// to a size that's still recognizable.
    fn size_pictures(&self) {
        let rows = self.rows.borrow();
        let width = if self.host_list.width() > 0 { self.host_list.width() } else { 250 };
        let height = if self.window.height() > 0 { self.window.height() } else { self.window.default_height() };
        let widest = (width - 40).max(60) as f64 * 3.0 / 4.0;
        // The sidebar's header bar, then each row's name and caption.
        let fit = (height - 64) as f64 / rows.len().max(1) as f64 - 60.0;
        let picture = fit.max(54.0).min(widest).min(220.0);
        for row in rows.values() {
            row.frame.set_size_request(-1, picture as i32);
        }
    }

    /// Per-host menu actions (they take the host id).
    fn add_row_actions(&self) {
        if self.window.lookup_action("edit-host-id").is_some() {
            return;
        }
        let edit = gio::SimpleAction::new("edit-host-id", Some(glib::VariantTy::STRING));
        edit.connect_activate(|_, p| {
            if let (Some(ui), Some(id)) = (ui(), p.and_then(|p| p.str().map(str::to_string))) {
                ui.edit_host(Some(id));
            }
        });
        self.window.add_action(&edit);
        let delete = gio::SimpleAction::new("delete-host", Some(glib::VariantTy::STRING));
        delete.connect_activate(|_, p| {
            if let (Some(ui), Some(id)) = (ui(), p.and_then(|p| p.str().map(str::to_string))) {
                ui.confirm_delete(id);
            }
        });
        self.window.add_action(&delete);
    }

    fn open_profile(&self) -> Option<HostProfile> {
        let id = self.state.borrow().profile_id.clone()?;
        self.profiles.borrow().iter().find(|p| p.id == id).cloned()
    }

    fn render(&self) {
        let s = self.core.state();
        let was_open = self.state.borrow().profile_id.clone();
        if was_open != s.profile_id {
            self.video.clear();
        }
        *self.state.borrow_mut() = s.clone();
        let profile = self.open_profile();
        let open = profile.is_some();
        let connected = s.connection == ConnectionState::Connected;
        let video_up = s.video_stats.as_ref().is_some_and(|v| v.state == "connected");

        self.title.set_title(profile.as_ref().map_or("MiSTerKVMS", |p| p.name.as_str()));
        self.title.set_subtitle(&match &profile {
            None => String::new(),
            Some(p) if s.core_running.is_empty() => p.mister_host.clone(),
            Some(_) => s.core_running.clone(),
        });

        let set_enabled = |name: &str, on: bool| {
            if let Some(a) = self.window.lookup_action(name).and_then(|a| a.downcast::<gio::SimpleAction>().ok()) {
                a.set_enabled(on);
            }
        };
        for name in ["edit-host", "scale", "smooth", "auto-crop", "predictive", "zoom-in", "zoom-out", "zoom-reset", "mute", "show-stats", "connection-info"] {
            set_enabled(name, open);
        }
        set_enabled("mister-menu", connected);
        set_enabled("ctrl-alt-del", connected);
        set_enabled("capture", connected || s.capturing);
        set_enabled("record", s.recording_seconds.is_some() || video_up);
        set_enabled("show-recording", s.last_recording.is_some());
        set_enabled("copy-screenshot", video_up);
        set_enabled("save-screenshot", video_up);
        let set_state = |name: &str, v: glib::Variant| {
            if let Some(a) = self.window.lookup_action(name).and_then(|a| a.downcast::<gio::SimpleAction>().ok()) {
                a.set_state(&v);
            }
        };
        set_state("scale", s.display.scale_mode.name().to_variant());
        set_state("smooth", s.display.smooth.to_variant());
        set_state("auto-crop", s.display.auto_crop.to_variant());
        set_state("predictive", s.display.predictive_cursor.to_variant());

        self.capture_button.set_icon_name(if s.capturing { "process-stop-symbolic" } else { "input-mouse-symbolic" });
        let tooltip = if s.capturing {
            format!("Release keyboard and mouse ({})", self.core.release_chord_name())
        } else {
            "Send keyboard and mouse to MiSTer (or click the screen) — Ctrl+Shift+K".to_string()
        };
        self.capture_button.set_tooltip_text(Some(&tooltip));
        self.menu_button.set_sensitive(connected);
        self.info_button.set_sensitive(open);
        self.zoom_label.set_label(&format!("{}%", (s.display.zoom * 100.0).round()));
        let recording = s.recording_seconds.is_some();
        self.record_button.set_icon_name(if recording { "media-playback-stop-symbolic" } else { "media-record-symbolic" });
        self.record_button.set_tooltip_text(Some(if recording {
            "Stop recording (Ctrl+Shift+R)"
        } else {
            "Record video and audio to Videos/MiSTerKVMS (Ctrl+Shift+R)"
        }));
        if self.mute_button.is_active() != s.muted {
            self.mute_button.set_active(s.muted);
        }
        self.mute_button.set_icon_name(if s.muted { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" });
        // Adwaita has one pin icon: dimmed when not pinned.
        self.stats_pin.set_opacity(if s.stats_pinned { 1.0 } else { 0.45 });
        self.stats_pin.set_tooltip_text(Some(if s.stats_pinned {
            "Unpin: hide automatically (Ctrl+Shift+I)"
        } else {
            "Pin: always show (Ctrl+Shift+I)"
        }));

        self.render_placeholder(profile.as_ref(), &s, video_up);
        self.render_stats(&s, open);
        self.render_status(&s);
    }

    fn render_placeholder(&self, profile: Option<&HostProfile>, s: &ControllerState, video_up: bool) {
        // With no hosts the No Hosts page says what to do instead.
        self.placeholder.set_visible(!video_up && (profile.is_some() || !self.profiles.borrow().is_empty()));
        let connected = s.connection == ConnectionState::Connected;
        let hint = if connected && !s.capturing { "\n\nClick here to capture keyboard and mouse" } else { "" };
        let (title, detail) = match profile {
            None if self.profiles.borrow().is_empty() => ("Add a host to get started".to_string(), String::new()),
            None => ("Select a Host".to_string(), "Choose a MiSTer in the sidebar to connect.".to_string()),
            // No stream yet: the core says why (none chosen, or still
            // looking for the one that shows this MiSTer).
            Some(p) if p.stream.trim().is_empty() => (
                "No capture stream set".to_string(),
                match &s.connection {
                    ConnectionState::Disconnected { reason } if reason != "no capture stream chosen" => sentence(reason),
                    _ => "Edit the host and choose the capture server stream for this MiSTer.".to_string(),
                },
            ),
            Some(_) if s.video_error.is_some() => ("Video unavailable".to_string(), s.video_error.clone().unwrap_or_default()),
            Some(p) => (
                p.stream.trim().to_string(),
                match s.video_stats.as_ref().map(|v| v.state.as_str()) {
                    None | Some("searching") | Some("connecting") | Some("opening") => "Connecting to the capture stream…".to_string(),
                    Some("no video") => "Connected, waiting for video".to_string(),
                    Some(other) => sentence(other),
                },
            ),
        };
        self.placeholder.set_title(&title);
        self.placeholder.set_description(Some(&glib::markup_escape_text(&format!("{detail}{hint}"))));
    }

    fn render_stats(&self, s: &ControllerState, open: bool) {
        self.stats_box.set_visible(open && s.stats_visible);
        if !self.stats_box.is_visible() {
            return;
        }
        let mut lines = vec!["INPUT".to_string()];
        if let Some(i) = &s.input_stats {
            lines.push(format!("link   {}", if i.connected { "up" } else { "down" }));
            lines.push(format!("msg/s  {:.0}", i.messages_per_sec));
            lines.push(format!("sent   {}", i.messages_sent));
            lines.push(format!("queue  {}", i.queued));
        }
        lines.push(String::new());
        lines.push("VIDEO".to_string());
        match &s.video_stats {
            Some(v) => {
                lines.push(format!("state  {}", v.state));
                lines.push(format!("src    {}×{}", v.source_width, v.source_height));
                lines.push(format!("crop   {}×{}", v.crop_width, v.crop_height));
                lines.push(format!("fps    {:.1} in / {:.1} drawn", v.received_fps, v.rendered_fps));
                lines.push(format!("drop   {} source / {} skipped", v.dropped_frames, v.skipped_frames));
                lines.push(format!("age    {}", age_text(v.frame_age_ms)));
                lines.push(format!("audio  {:.0} ms buf · {} underruns", v.audio_buffer_ms, v.audio_underruns));
                if s.display.predictive_cursor {
                    lines.push(format!(
                        "cursor {} · err {:.0} px",
                        if v.cursor_locked { "tracked" } else { "searching" },
                        v.cursor_error_px
                    ));
                    lines.push(format!(
                        "model  lag {:.0} ms · ×{:.2} · {} fits/min",
                        v.cursor_lag_ms, v.cursor_gain, v.cursor_fit_samples
                    ));
                }
            }
            None => lines.push("off".to_string()),
        }
        self.stats_label.set_text(&lines.join("\n"));
    }

    fn render_status(&self, s: &ControllerState) {
        for c in ["connected", "connecting", "disconnected"] {
            self.status_dot.remove_css_class(c);
        }
        let text = match &s.connection {
            ConnectionState::Connected => {
                self.status_dot.add_css_class("connected");
                if s.core_running.is_empty() {
                    "Connected · MiSTer menu".to_string()
                } else {
                    format!("Connected · {}", s.core_running)
                }
            }
            ConnectionState::Connecting => {
                self.status_dot.add_css_class("connecting");
                "Connecting…".to_string()
            }
            ConnectionState::Disconnected { reason } => {
                self.status_dot.add_css_class("disconnected");
                format!("Disconnected: {reason}")
            }
        };
        self.status_label.set_text(&text);
        match (s.recording_seconds, &s.recording_error, &s.last_recording) {
            (Some(secs), _, _) => {
                let secs = secs as u64;
                self.recording_label.set_text(&format!("● {}:{:02}", secs / 60, secs % 60));
                self.last_recording.set_visible(false);
            }
            (None, Some(e), _) => {
                self.recording_label.set_text(&format!("Recording failed: {e}"));
                self.last_recording.set_visible(false);
            }
            (None, None, Some(path)) => {
                self.recording_label.set_text("");
                let name = std::path::Path::new(path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into());
                self.last_recording.set_label(&format!("Saved {name} · Show in Folder"));
                self.last_recording.set_visible(true);
            }
            _ => {
                self.recording_label.set_text("");
                self.last_recording.set_visible(false);
            }
        }
        self.game_label.set_text(&s.game_running);
        let capturing = if s.capturing {
            format!("Capturing — {} to release", self.core.release_chord_name())
        } else {
            "F12 = MiSTer menu · not capturing".to_string()
        };
        self.capture_label.set_text(&capturing);
    }

    // MARK: Capture

    fn start_capture(self: &Rc<Self>) {
        let raw = self.open_profile().is_some_and(|p| p.raw_mouse);
        let Some(session) = self.core.input_session() else { return };
        if !self.core.is_connected() || self.capture.borrow().is_active() {
            return;
        }
        if !self.core.capture_intro_seen() {
            self.ask_capture_intro();
            return;
        }
        let note = self.capture.borrow_mut().start(self.video.upcast_ref(), session, raw);
        self.core.set_capturing(true);
        self.show_hint(note);
    }

    /// Type text (the clipboard, to start with) on the MiSTer key by key, as
    /// on a US keyboard, at a speed the core copes with (remembered for the
    /// host: kvms_core::text).
    fn type_text(self: &Rc<Self>) {
        if !self.core.is_connected() {
            return;
        }
        let range = kvms_core::text::type_speed_range();
        let saved = self.open_profile().map(|p| p.type_speed).unwrap_or(0.0);
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        let view = gtk::TextView::with_buffer(&buffer);
        view.set_monospace(true);
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        let scroll = gtk::ScrolledWindow::builder().min_content_height(160).min_content_width(420).child(&view).build();
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, range[0], range[1], 1.0);
        scale.set_value(if saved > 0.0 { saved } else { range[2] });
        scale.set_digits(0);
        scale.set_draw_value(true);
        scale.set_hexpand(true);
        let speed = gtk::Label::new(Some("Speed (keys per second): slow it down if letters go missing"));
        speed.set_xalign(0.0);
        let summary = gtk::Label::new(None);
        summary.set_wrap(true);
        summary.set_xalign(0.0);
        summary.add_css_class("dim-label");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.append(&scroll);
        content.append(&speed);
        content.append(&scale);
        content.append(&summary);

        let dialog = adw::AlertDialog::new(
            Some("Type text on the MiSTer"),
            Some("Typed key by key, as on a US keyboard (the MiSTer has no clipboard)."),
        );
        dialog.set_extra_child(Some(&content));
        dialog.add_responses(&[("cancel", "Cancel"), ("type", "Type")]);
        dialog.set_response_appearance("type", adw::ResponseAppearance::Suggested);
        dialog.set_close_response("cancel");

        let text_of = |b: &gtk::TextBuffer| b.text(&b.start_iter(), &b.end_iter(), false).to_string();
        let update: Rc<dyn Fn()> = Rc::new({
            let (buffer, scale, summary, dialog) = (buffer.clone(), scale.clone(), summary.clone(), dialog.clone());
            move || {
                let Some(ui) = ui() else { return };
                let speed = scale.value().round();
                let p = kvms_core::text::text_typing(text_of(&buffer), speed);
                let st = ui.core.state();
                let other = st.stream.as_ref().is_some_and(|s| !s.have_control && !s.holder.is_empty());
                let can_type = ui.core.is_connected() && !other;
                let time = if p.seconds < 60.0 { format!("{} s", p.seconds.ceil()) } else { format!("{} min", (p.seconds / 60.0).ceil()) };
                let text = if !can_type {
                    "Someone else has control of this MiSTer: take control first.".to_string()
                } else if p.keys == 0 {
                    "Nothing to type.".to_string()
                } else if p.skipped > 0 {
                    format!("{} keys, about {time} · {} character(s) can't be typed on a US keyboard and will be skipped", p.keys, p.skipped)
                } else {
                    format!("{} keys, about {time}", p.keys)
                };
                summary.set_text(&text);
                dialog.set_response_enabled("type", can_type && p.keys > 0);
            }
        });
        buffer.connect_changed({
            let update = update.clone();
            move |_| update()
        });
        scale.connect_value_changed({
            let update = update.clone();
            move |_| update()
        });
        update();
        self.window.clipboard().read_text_async(None::<&gio::Cancellable>, {
            let buffer = buffer.clone();
            move |result| {
                if let Ok(Some(text)) = result {
                    buffer.set_text(&text);
                }
            }
        });
        dialog.connect_response(None, move |_, response| {
            if response == "type" {
                if let Some(ui) = ui() {
                    ui.core.type_text_at(text_of(&buffer), scale.value().round());
                }
            }
        });
        dialog.present(Some(&self.window));
    }

    /// Before the very first capture: how to get the keyboard and mouse
    /// back, so nobody thinks the computer is stuck. "Not now" asks again
    /// next time.
    fn ask_capture_intro(self: &Rc<Self>) {
        if self.asking_intro.replace(true) {
            return;
        }
        let dialog = adw::AlertDialog::new(
            Some("Your keyboard and mouse are about to go to the MiSTer"),
            Some(
                &format!(
                    "The pointer disappears and everything you type goes to the MiSTer. To get them back, \
                     press {} together and let go (the menu's Release Keys has others). Switching to \
                     another window also gets them back.\n\nF12 opens the MiSTer menu. Clicking the picture \
                     captures again.",
                    self.core.release_chord_name()
                ),
            ),
        );
        dialog.add_responses(&[("later", "Not Now"), ("capture", "Capture")]);
        dialog.set_response_appearance("capture", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("capture"));
        dialog.set_close_response("later");
        dialog.connect_response(None, |_, response| {
            if let Some(ui) = ui() {
                ui.asking_intro.set(false);
                if response == "capture" {
                    ui.core.set_capture_intro_seen();
                    ui.start_capture();
                }
            }
        });
        dialog.present(Some(&self.window));
    }

    fn stop_capture(&self) {
        if self.capture.borrow().is_active() {
            self.capture.borrow_mut().stop();
            self.core.set_capturing(false);
        }
        self.capture_hint.set_visible(false);
    }

    fn show_hint(self: &Rc<Self>, note: Option<String>) {
        self.hint_keys.set_markup(&format!(
            "Press <b>{}</b> together to release · <b>F12</b> opens the MiSTer menu",
            glib::markup_escape_text(&self.core.release_chord_name())
        ));
        self.hint_note.set_text(note.as_deref().unwrap_or(""));
        self.hint_note.set_visible(note.is_some());
        self.capture_hint.set_visible(true);
        if let Some(t) = self.hint_timer.borrow_mut().take() {
            t.remove();
        }
        // The desktop may ask before passing its shortcuts on; say so if it didn't.
        let timer = glib::timeout_add_local_once(Duration::from_secs(4), || {
            if let Some(ui) = ui() {
                ui.hint_timer.borrow_mut().take();
                ui.capture_hint.set_visible(false);
                if ui.capture.borrow().shortcuts_inhibited() == Some(false) {
                    ui.toast("The desktop kept its own shortcuts (Super, Alt+Tab): they won't reach MiSTer.");
                }
            }
        });
        *self.hint_timer.borrow_mut() = Some(timer);
    }

    // MARK: Hosts

    fn edit_host(self: &Rc<Self>, id: Option<String>) {
        self.stop_capture();
        let profile = match &id {
            Some(id) => match self.profiles.borrow().iter().find(|p| &p.id == id) {
                Some(p) => p.clone(),
                None => return,
            },
            None => kvms_core::profiles::new_host_profile(),
        };
        crate::editor::show(&self.window, profile, id.is_none(), |saved, is_new| {
            if let Some(ui) = ui() {
                let id = saved.id.clone();
                ui.core.save_profile(saved);
                if is_new {
                    ui.core.open(id);
                    ui.load_profiles();
                }
            }
        });
    }

    fn confirm_delete(&self, id: String) {
        let Some(name) = self.profiles.borrow().iter().find(|p| p.id == id).map(|p| p.name.clone()) else { return };
        let dialog = adw::AlertDialog::new(Some(&format!("Delete “{name}”?")), Some("The host's settings are removed."));
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_close_response("cancel");
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                if let Some(ui) = ui() {
                    ui.core.delete_profile(id.clone());
                }
            }
        });
        dialog.present(Some(&self.window));
    }

    fn export_profiles(&self) {
        let dialog = gtk::FileDialog::builder().title("Export Hosts").initial_name("MiSTerKVMS hosts.json").build();
        dialog.save(Some(&self.window), gio::Cancellable::NONE, |result| {
            let (Ok(file), Some(ui)) = (result, ui()) else { return };
            let Some(path) = file.path() else { return };
            match std::fs::write(&path, ui.core.export_profiles()) {
                Ok(()) => ui.toast("Hosts exported."),
                Err(e) => ui.toast(&format!("Export failed: {e}")),
            }
        });
    }

    fn import_profiles(&self) {
        let dialog = gtk::FileDialog::builder().title("Import Hosts").build();
        dialog.open(Some(&self.window), gio::Cancellable::NONE, |result| {
            let (Ok(file), Some(ui)) = (result, ui()) else { return };
            let Some(path) = file.path() else { return };
            let message = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|json| {
                ui.core.import_profiles(json).map_err(|e| e.to_string())
            }) {
                Ok(1) => "Imported 1 host.".to_string(),
                Ok(n) => format!("Imported {n} hosts."),
                Err(e) => format!("That file couldn't be read: {e}"),
            };
            ui.toast(&message);
        });
    }

    // MARK: Recording

    fn toggle_recording(&self) {
        if self.state.borrow().recording_seconds.is_some() {
            self.core.stop_recording();
            return;
        }
        let Some(profile) = self.open_profile() else { return };
        let dir = glib::user_special_dir(glib::UserDirectory::Videos)
            .unwrap_or_else(glib::home_dir)
            .join("MiSTerKVMS");
        let stamp = glib::DateTime::now_local()
            .and_then(|t| t.format("%Y-%m-%d %H.%M.%S"))
            .map(|s| s.to_string())
            .unwrap_or_default();
        let path = kvms_core::video::ffmpeg::recording_path(&dir, &profile.name, &stamp);
        if let Err(e) = self.core.start_recording(path.display().to_string()) {
            self.toast(&format!("Can't record: {e}"));
        }
    }

    // MARK: Screenshots

    /// The picture as it is now, onto the clipboard.
    fn copy_screenshot(&self) {
        let shot = match self.core.screenshot(screenshot_time()) {
            Ok(shot) => shot,
            Err(e) => return self.toast(&format!("No screenshot: {e}")),
        };
        match gdk::Texture::from_bytes(&glib::Bytes::from_owned(shot.png)) {
            Ok(texture) => {
                self.window.clipboard().set_texture(&texture);
                self.toast("Screenshot copied");
            }
            Err(e) => self.toast(&format!("No screenshot: {e}")),
        }
    }

    /// The picture as it is now, into Pictures/MiSTerKVMS.
    fn save_screenshot(&self) {
        match self.core.save_screenshot(screenshot_time(), screenshot_dir().display().to_string()) {
            Ok(path) => {
                let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(path);
                self.toast(&format!("Saved {name}"));
            }
            Err(e) => self.toast(&format!("Not saved: {e}")),
        }
    }

    pub fn toast(&self, message: &str) {
        self.toasts.add_toast(adw::Toast::new(message));
    }
}

/// Where Save Screenshot puts them.
fn screenshot_dir() -> std::path::PathBuf {
    glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(glib::home_dir).join("MiSTerKVMS")
}

/// Local time, as it goes in the picture and the file name.
fn screenshot_time() -> String {
    glib::DateTime::now_local().and_then(|t| t.format("%Y-%m-%d %H:%M:%S")).map(|s| s.to_string()).unwrap_or_default()
}

/// A key from the window's key controller. While capturing it goes to
/// MiSTer and GTK never sees it.
fn key(keycode: u32, down: bool) -> glib::Propagation {
    let Some(ui) = ui() else { return glib::Propagation::Proceed };
    let session = match ui.capture.borrow().session() {
        Some(s) => s.clone(),
        None => return glib::Propagation::Proceed,
    };
    // GDK key codes are evdev codes + 8 on both Wayland and X11.
    if let Some(usage) = keycode.checked_sub(8).and_then(|c| linux_to_hid(c as u16)) {
        if session.key_event(usage, down) == KeyResult::ExitCapture {
            ui.stop_capture();
        }
    }
    glib::Propagation::Stop
}

/// A core message ("looking for …") as a sentence for the placeholder.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Capture→screen age (the core corrects for the two clocks' difference).
fn age_text(ms: f64) -> String {
    if ms < 0.0 {
        "n/a".into()
    } else {
        format!("≈{ms:.0} ms capture→screen")
    }
}

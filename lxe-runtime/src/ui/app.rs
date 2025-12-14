//! LXE Application - GTK4 Application Setup
//!
//! Initializes the GTK4/Libadwaita application and handles the main event loop.

use crate::payload::PayloadInfo;
use crate::state::{detect_install_state, WizardMode};
use crate::ui::window::LxeWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::gio;
use std::cell::RefCell;

/// Application ID for LXE Runtime
const APP_ID: &str = "org.lxe.Runtime";

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LxeApplication {
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub force_install: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LxeApplication {
        const NAME: &'static str = "LxeApplication";
        type Type = super::LxeApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for LxeApplication {}

    impl ApplicationImpl for LxeApplication {
        fn activate(&self) {
            let app = self.obj();
            
            // Get payload info
            let payload_info = self.payload_info.borrow();
            
            // Determine wizard mode
            let wizard_mode = if let Some(ref info) = *payload_info {
                if *self.force_install.borrow() {
                    WizardMode::Install
                } else {
                    let state = detect_install_state(&info.metadata);
                    state.to_wizard_mode(&info.metadata.version)
                }
            } else {
                // No payload - show demo mode
                WizardMode::Install
            };
            
            // Create and show the main window
            let window = LxeWindow::new(&app, payload_info.clone(), wizard_mode);
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();
            
            // Load CSS - with graceful handling for missing display
            // V9 FIX: Don't panic if no display available
            let css_provider = gtk::CssProvider::new();
            css_provider.load_from_data(include_str!("styles.css"));
            
            // Check if display is available before adding CSS provider
            match gtk::gdk::Display::default() {
                Some(display) => {
                    gtk::style_context_add_provider_for_display(
                        &display,
                        &css_provider,
                        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                }
                None => {
                    // No display - this might be running in a container or via SSH
                    // Log warning but don't panic
                    tracing::warn!(
                        "No display available. CSS styling will not be applied. \
                         Consider using --silent mode for headless operation."
                    );
                }
            }
            
            // Set up actions
            let app = self.obj();
            app.setup_actions();
        }
    }

    impl GtkApplicationImpl for LxeApplication {}
    impl AdwApplicationImpl for LxeApplication {}
}

glib::wrapper! {
    pub struct LxeApplication(ObjectSubclass<imp::LxeApplication>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl LxeApplication {
    pub fn new(payload_info: Option<PayloadInfo>, force_install: bool) -> Self {
        let app: Self = glib::Object::builder()
            .property("application-id", APP_ID)
            .property("flags", gio::ApplicationFlags::FLAGS_NONE)
            .build();
        
        let imp = app.imp();
        *imp.payload_info.borrow_mut() = payload_info;
        *imp.force_install.borrow_mut() = force_install;
        
        app
    }
    
    fn setup_actions(&self) {
        // Quit action
        let quit_action = gio::SimpleAction::new("quit", None);
        quit_action.connect_activate(glib::clone!(
            @weak self as app =>
            move |_, _| {
                app.quit();
            }
        ));
        self.add_action(&quit_action);
        
        // Set keyboard shortcuts
        self.set_accels_for_action("app.quit", &["<Ctrl>q"]);
    }
    
    pub fn run(&self) -> glib::ExitCode {
        ApplicationExtManual::run(self)
    }
}

impl Default for LxeApplication {
    fn default() -> Self {
        Self::new(None, false)
    }
}

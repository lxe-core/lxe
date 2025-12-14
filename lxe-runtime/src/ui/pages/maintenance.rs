//! Maintenance Page - Shown when application is already installed
//!
//! Provides options to Uninstall, Repair, or Upgrade the application.

use crate::payload::PayloadInfo;
use crate::state::WizardMode;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MaintenancePage {
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub wizard_mode: RefCell<WizardMode>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MaintenancePage {
        const NAME: &'static str = "LxeMaintenancePage";
        type Type = super::MaintenancePage;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for MaintenancePage {
        fn constructed(&self) {
            self.parent_constructed();
            // NOTE: DO NOT call setup_ui() here!
            // payload_info must be set first in new() before setup_ui() runs
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("action-selected")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for MaintenancePage {}
    impl BoxImpl for MaintenancePage {}
}

glib::wrapper! {
    pub struct MaintenancePage(ObjectSubclass<imp::MaintenancePage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl MaintenancePage {
    pub fn new(payload_info: Option<PayloadInfo>, wizard_mode: WizardMode) -> Self {
        let obj: Self = glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .property("spacing", 12)
            .property("valign", gtk::Align::Center)
            .property("vexpand", true)
            .build();
        
        let imp = obj.imp();
        *imp.payload_info.borrow_mut() = payload_info;
        *imp.wizard_mode.borrow_mut() = wizard_mode;
        
        // CRITICAL: setup_ui() must be called AFTER payload_info is set!
        obj.setup_ui();
        
        obj
    }
    
    fn setup_ui(&self) {
        let imp = self.imp();
        let payload = imp.payload_info.borrow();
        let mode = imp.wizard_mode.borrow().clone();
        
        // DEBUG: Warn if payload_info is None (See UI_DEVELOPMENT.md)
        #[cfg(debug_assertions)]
        if payload.is_none() {
            tracing::warn!("MaintenancePage::setup_ui called with no payload_info");
        }
        
        let app_name = payload
            .as_ref()
            .map(|p| p.metadata.name.clone())
            .unwrap_or_else(|| "Application".to_string());
        
        let (current_version, can_upgrade, can_repair) = match mode {
            WizardMode::Maintenance {
                current_version,
                can_upgrade,
                can_repair,
                ..
            } => (current_version, can_upgrade, can_repair),
            _ => ("unknown".to_string(), false, false),
        };
        
        let new_version = payload
            .as_ref()
            .map(|p| p.metadata.version.clone())
            .unwrap_or_else(|| "1.0.0".to_string());
        
        // Application icon
        let icon = gtk::Image::builder()
            .icon_name("application-x-executable")
            .pixel_size(64)
            .margin_bottom(8)
            .build();
        
        // Title
        let title = gtk::Label::builder()
            .label(&format!("{} is installed", app_name))
            .css_classes(["title-1"])
            .build();
        
        // Version info
        let version_info = if can_upgrade {
            format!("Current: v{}  →  Available: v{}", current_version, new_version)
        } else {
            format!("Version: {}", current_version)
        };
        
        let version_label = gtk::Label::builder()
            .label(&version_info)
            .css_classes(["body", "dim-label"])
            .margin_bottom(16)
            .build();
        
        self.append(&icon);
        self.append(&title);
        self.append(&version_label);
        
        // Action buttons in a preferences group style
        let actions_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Center)
            .margin_top(8)
            .build();
        
        // Upgrade button (if available)
        if can_upgrade {
            let upgrade_row = self.create_action_row(
                "software-update-available-symbolic",
                "Upgrade",
                &format!("Update to version {}", new_version),
                "upgrade",
                &["suggested-action"],
            );
            actions_box.append(&upgrade_row);
        }
        
        // Repair button (if needed)
        if can_repair {
            let repair_row = self.create_action_row(
                "wrench-wide-symbolic",
                "Repair",
                "Reinstall missing or corrupted files",
                "repair",
                &[],
            );
            actions_box.append(&repair_row);
        }
        
        // Uninstall button (always available)
        let uninstall_row = self.create_action_row(
            "user-trash-symbolic",
            "Uninstall",
            "Remove the application from your system",
            "uninstall",
            &["destructive-action"],
        );
        actions_box.append(&uninstall_row);
        
        self.append(&actions_box);
    }
    
    fn create_action_row(
        &self,
        icon_name: &str,
        title: &str,
        subtitle: &str,
        action: &str,
        extra_classes: &[&str],
    ) -> gtk::Button {
        let button = gtk::Button::builder()
            .css_classes(["action-row-button"])
            .width_request(320)
            .build();
        
        for class in extra_classes {
            button.add_css_class(class);
        }
        
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();
        
        let icon = gtk::Image::builder()
            .icon_name(icon_name)
            .pixel_size(24)
            .build();
        
        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build();
        
        let title_label = gtk::Label::builder()
            .label(title)
            .css_classes(["heading"])
            .halign(gtk::Align::Start)
            .build();
        
        let subtitle_label = gtk::Label::builder()
            .label(subtitle)
            .css_classes(["caption", "dim-label"])
            .halign(gtk::Align::Start)
            .build();
        
        text_box.append(&title_label);
        text_box.append(&subtitle_label);
        
        let arrow = gtk::Image::builder()
            .icon_name("go-next-symbolic")
            .css_classes(["dim-label"])
            .build();
        
        content.append(&icon);
        content.append(&text_box);
        content.append(&arrow);
        
        button.set_child(Some(&content));
        
        // Connect click handler
        let action_str = action.to_string();
        button.connect_clicked(glib::clone!(
            @weak self as page =>
            move |_| {
                page.emit_by_name::<()>("action-selected", &[&action_str]);
            }
        ));
        
        button
    }
}

impl Default for MaintenancePage {
    fn default() -> Self {
        Self::new(None, WizardMode::Install)
    }
}

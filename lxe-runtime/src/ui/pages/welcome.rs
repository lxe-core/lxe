//! Welcome Page - First page of the installation wizard
//!
//! Shows the application icon, name, and Install button.

use crate::payload::PayloadInfo;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct WelcomePage {
        pub payload_info: RefCell<Option<PayloadInfo>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WelcomePage {
        const NAME: &'static str = "LxeWelcomePage";
        type Type = super::WelcomePage;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for WelcomePage {
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
                    glib::subclass::Signal::builder("install-clicked")
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for WelcomePage {}
    impl BoxImpl for WelcomePage {}
}

glib::wrapper! {
    pub struct WelcomePage(ObjectSubclass<imp::WelcomePage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl WelcomePage {
    pub fn new(payload_info: Option<PayloadInfo>) -> Self {
        let obj: Self = glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .property("spacing", 16)
            .property("valign", gtk::Align::Center)
            .property("vexpand", true)
            .build();
        
        *obj.imp().payload_info.borrow_mut() = payload_info;
        
        // CRITICAL: setup_ui() must be called AFTER payload_info is set!
        obj.setup_ui();
        
        obj
    }
    
    fn setup_ui(&self) {
        let payload = self.imp().payload_info.borrow();
        
        // DEBUG: Warn if we're in demo mode unexpectedly
        // This catches the init order bug where setup_ui() is called before payload is set
        #[cfg(debug_assertions)]
        if payload.is_none() {
            tracing::warn!(
                "WelcomePage::setup_ui called with no payload_info. \
                 If this is not intentional, check that setup_ui() is called \
                 AFTER payload_info is set in new(). See UI_DEVELOPMENT.md"
            );
        }
        
        let (app_name, app_version, app_description) = if let Some(ref info) = *payload {
            // Use custom installer text if provided, otherwise fall back to package metadata
            let title = info.metadata.installer.welcome_title.clone()
                .unwrap_or_else(|| info.metadata.name.clone());
            let text = info.metadata.installer.welcome_text.clone()
                .unwrap_or_else(|| info.metadata.description.clone().unwrap_or_default());
            (
                title,
                info.metadata.version.clone(),
                text,
            )
        } else {
            // Demo mode
            (
                "LXE Package".to_string(),
                "1.0.0".to_string(),
                "A modern Linux application".to_string(),
            )
        };
        
        // Extract and display actual package icon from payload
        let icon = if let Some(ref info) = *payload {
            // Try to extract icon from payload
            match crate::payload::extract_icon_to_temp(info) {
                Ok(Some(icon_path)) => {
                    // Load the extracted icon as a Paintable/Texture
                    match gtk::gdk::Texture::from_filename(&icon_path) {
                        Ok(texture) => {
                            gtk::Image::builder()
                                .paintable(&texture)
                                .pixel_size(96)
                                .margin_bottom(8)
                                .css_classes(["app-icon"])
                                .build()
                        }
                        Err(_) => {
                            // Fallback to generic icon if load fails
                            gtk::Image::builder()
                                .icon_name("application-x-executable")
                                .pixel_size(96)
                                .margin_bottom(8)
                                .css_classes(["app-icon"])
                                .build()
                        }
                    }
                }
                _ => {
                    // No icon in package
                    gtk::Image::builder()
                        .icon_name("application-x-executable")
                        .pixel_size(96)
                        .margin_bottom(8)
                        .css_classes(["app-icon"])
                        .build()
                }
            }
        } else {
            // Demo mode - use placeholder
            gtk::Image::builder()
                .icon_name("application-x-executable")
                .pixel_size(96)
                .margin_bottom(8)
                .css_classes(["app-icon"])
                .build()
        };
        
        // Application name
        let title = gtk::Label::builder()
            .label(&app_name)
            .css_classes(["title-1"])
            .build();
        
        // Version
        let version = gtk::Label::builder()
            .label(&format!("Version {}", app_version))
            .css_classes(["dim-label"])
            .build();
        
        // Description
        let description = gtk::Label::builder()
            .label(&app_description)
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(40)
            .margin_top(8)
            .css_classes(["body"])
            .build();
        
        // Install button with pill shape and accent color
        let install_button = gtk::Button::builder()
            .label("Install")
            .css_classes(["pill", "suggested-action", "install-button"])
            .halign(gtk::Align::Center)
            .width_request(200)
            .height_request(48)
            .margin_top(24)
            .build();
        
        // Connect install button
        install_button.connect_clicked(glib::clone!(
            @weak self as page =>
            move |_| {
                page.emit_by_name::<()>("install-clicked", &[]);
            }
        ));
        
        // Add all widgets
        self.append(&icon);
        self.append(&title);
        self.append(&version);
        self.append(&description);
        self.append(&install_button);
        
        // Add installation path hint
        let install_path = dirs::data_local_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.local/share".to_string());
        
        let path_label = gtk::Label::builder()
            .label(&format!("Will be installed to: {}", install_path))
            .css_classes(["caption", "dim-label"])
            .margin_top(8)
            .build();
        
        self.append(&path_label);
    }
}

impl Default for WelcomePage {
    fn default() -> Self {
        Self::new(None)
    }
}

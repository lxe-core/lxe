//! Complete Page - Shows success or failure after installation
//!
//! Offers options to launch the application or close the installer.

use crate::payload::PayloadInfo;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CompletePage {
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub is_uninstall: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CompletePage {
        const NAME: &'static str = "LxeCompletePage";
        type Type = super::CompletePage;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for CompletePage {
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
                    glib::subclass::Signal::builder("launch-clicked")
                        .build(),
                    glib::subclass::Signal::builder("close-clicked")
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for CompletePage {}
    impl BoxImpl for CompletePage {}
}

glib::wrapper! {
    pub struct CompletePage(ObjectSubclass<imp::CompletePage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl CompletePage {
    pub fn new(payload_info: Option<PayloadInfo>, is_uninstall: bool) -> Self {
        let obj: Self = glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .property("spacing", 16)
            .property("valign", gtk::Align::Center)
            .property("vexpand", true)
            .build();
        
        let imp = obj.imp();
        *imp.payload_info.borrow_mut() = payload_info;
        *imp.is_uninstall.borrow_mut() = is_uninstall;
        
        // CRITICAL: setup_ui() must be called AFTER payload_info is set!
        obj.setup_ui();
        
        obj
    }
    
    fn setup_ui(&self) {
        let imp = self.imp();
        let payload = imp.payload_info.borrow();
        let is_uninstall = *imp.is_uninstall.borrow();
        
        // DEBUG: Warn if payload_info is None (See UI_DEVELOPMENT.md)
        #[cfg(debug_assertions)]
        if payload.is_none() {
            tracing::warn!("CompletePage::setup_ui called with no payload_info");
        }
        
        let app_name = payload
            .as_ref()
            .map(|p| p.metadata.name.clone())
            .unwrap_or_else(|| "Application".to_string());
        
        // Show actual app icon if available, otherwise use success/trash icon
        let icon = if is_uninstall {
            gtk::Image::builder()
                .icon_name("user-trash-symbolic")
                .pixel_size(64)
                .css_classes(["success-icon"])
                .margin_bottom(8)
                .build()
        } else if let Some(ref info) = *payload {
            // Try to extract and display actual package icon
            match crate::payload::extract_icon_to_temp(info) {
                Ok(Some(icon_path)) => {
                    match gtk::gdk::Texture::from_filename(&icon_path) {
                        Ok(texture) => {
                            gtk::Image::builder()
                                .paintable(&texture)
                                .pixel_size(64)
                                .css_classes(["success-icon"])
                                .margin_bottom(8)
                                .build()
                        }
                        Err(_) => {
                            gtk::Image::builder()
                                .icon_name("emblem-ok-symbolic")
                                .pixel_size(64)
                                .css_classes(["success-icon"])
                                .margin_bottom(8)
                                .build()
                        }
                    }
                }
                _ => {
                    gtk::Image::builder()
                        .icon_name("emblem-ok-symbolic")
                        .pixel_size(64)
                        .css_classes(["success-icon"])
                        .margin_bottom(8)
                        .build()
                }
            }
        } else {
            gtk::Image::builder()
                .icon_name("emblem-ok-symbolic")
                .pixel_size(64)
                .css_classes(["success-icon"])
                .margin_bottom(8)
                .build()
        };
        
        // Add success styling via CSS
        icon.add_css_class("success");
        
        // Title
        let title = if is_uninstall {
            format!("{} Uninstalled", app_name)
        } else {
            format!("{} Installed!", app_name)
        };
        
        let title_label = gtk::Label::builder()
            .label(&title)
            .css_classes(["title-1"])
            .build();
        
        // Subtitle
        let subtitle = if is_uninstall {
            "The application has been removed from your system."
        } else {
            "The application is ready to use."
        };
        
        let subtitle_label = gtk::Label::builder()
            .label(subtitle)
            .css_classes(["body"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .margin_bottom(16)
            .build();
        
        self.append(&icon);
        self.append(&title_label);
        self.append(&subtitle_label);
        
        // Button box
        let button_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk::Align::Center)
            .margin_top(8)
            .build();
        
        if !is_uninstall {
            // Launch button
            let launch_button = gtk::Button::builder()
                .label(&format!("Launch {}", app_name))
                .css_classes(["pill", "suggested-action"])
                .width_request(160)
                .height_request(44)
                .build();
            
            launch_button.connect_clicked(glib::clone!(
                @weak self as page =>
                move |_| {
                    page.launch_application();
                }
            ));
            
            button_box.append(&launch_button);
        }
        
        // Close button
        let close_button = gtk::Button::builder()
            .label("Close")
            .css_classes(["pill"])
            .width_request(100)
            .height_request(44)
            .build();
        
        close_button.connect_clicked(glib::clone!(
            @weak self as page =>
            move |_| {
                if let Some(window) = page.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
                    window.close();
                }
            }
        ));
        
        button_box.append(&close_button);
        self.append(&button_box);
    }
    
    fn launch_application(&self) {
        let payload = self.imp().payload_info.borrow();
        
        if let Some(ref info) = *payload {
            let exec_path = dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
                .join(&info.metadata.app_id)
                .join(&info.metadata.exec);
            
            if exec_path.exists() {
                let _ = std::process::Command::new(&exec_path)
                    .spawn();
            } else {
                // Try launching by app ID using gtk-launch
                let _ = std::process::Command::new("gtk-launch")
                    .arg(&info.metadata.app_id)
                    .spawn();
            }
        }
        
        // Close the installer window
        if let Some(window) = self.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            window.close();
        }
    }
}

impl Default for CompletePage {
    fn default() -> Self {
        Self::new(None, false)
    }
}

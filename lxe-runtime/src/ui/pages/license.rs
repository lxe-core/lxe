//! License Page - Shows license/EULA with acceptance checkbox
//!
//! Only shown if license_text is present in package metadata.

use crate::payload::PayloadInfo;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LicensePage {
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub accept_checkbox: RefCell<Option<gtk::CheckButton>>,
        pub next_button: RefCell<Option<gtk::Button>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LicensePage {
        const NAME: &'static str = "LxeLicensePage";
        type Type = super::LicensePage;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for LicensePage {
        fn constructed(&self) {
            self.parent_constructed();
            // NOTE: setup_ui() called in new() after payload is set
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("next-clicked")
                        .build(),
                    glib::subclass::Signal::builder("back-clicked")
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for LicensePage {}
    impl BoxImpl for LicensePage {}
}

glib::wrapper! {
    pub struct LicensePage(ObjectSubclass<imp::LicensePage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl LicensePage {
    pub fn new(payload_info: Option<PayloadInfo>) -> Self {
        let obj: Self = glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .property("spacing", 12)
            .property("vexpand", true)
            .property("margin-start", 24)
            .property("margin-end", 24)
            .property("margin-top", 16)
            .property("margin-bottom", 16)
            .build();
        
        *obj.imp().payload_info.borrow_mut() = payload_info;
        obj.setup_ui();
        
        obj
    }
    
    fn setup_ui(&self) {
        let payload = self.imp().payload_info.borrow();
        
        // Get license text from metadata
        let license_text = payload
            .as_ref()
            .and_then(|p| p.metadata.installer.license_text.clone())
            .unwrap_or_else(|| "No license information provided.".to_string());
        
        // Title
        let title = gtk::Label::builder()
            .label("License Agreement")
            .css_classes(["title-2"])
            .halign(gtk::Align::Start)
            .build();
        
        // Scrollable license text
        let text_view = gtk::TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(gtk::WrapMode::Word)
            .vexpand(true)
            .top_margin(12)
            .bottom_margin(12)
            .left_margin(12)
            .right_margin(12)
            .build();
        
        text_view.buffer().set_text(&license_text);
        
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .min_content_height(200)
            .css_classes(["card", "license-scroll"])
            .child(&text_view)
            .build();
        
        // Accept checkbox
        let accept_checkbox = gtk::CheckButton::builder()
            .label("I accept the terms of the license agreement")
            .halign(gtk::Align::Start)
            .margin_top(8)
            .build();
        
        // Button box
        let button_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk::Align::End)
            .margin_top(12)
            .build();
        
        // Back button
        let back_button = gtk::Button::builder()
            .label("Back")
            .css_classes(["pill"])
            .width_request(100)
            .height_request(40)
            .build();
        
        // Next button (disabled until checkbox checked)
        let next_button = gtk::Button::builder()
            .label("Next")
            .css_classes(["pill", "suggested-action"])
            .width_request(100)
            .height_request(40)
            .sensitive(false) // Disabled by default
            .build();
        
        // Connect checkbox to enable/disable Next button
        accept_checkbox.connect_toggled(glib::clone!(
            @weak next_button =>
            move |checkbox| {
                next_button.set_sensitive(checkbox.is_active());
            }
        ));
        
        // Connect buttons
        back_button.connect_clicked(glib::clone!(
            @weak self as page =>
            move |_| {
                page.emit_by_name::<()>("back-clicked", &[]);
            }
        ));
        
        next_button.connect_clicked(glib::clone!(
            @weak self as page =>
            move |_| {
                page.emit_by_name::<()>("next-clicked", &[]);
            }
        ));
        
        button_box.append(&back_button);
        button_box.append(&next_button);
        
        // Store references
        *self.imp().accept_checkbox.borrow_mut() = Some(accept_checkbox.clone());
        *self.imp().next_button.borrow_mut() = Some(next_button.clone());
        
        // Add all widgets
        self.append(&title);
        self.append(&scroll);
        self.append(&accept_checkbox);
        self.append(&button_box);
    }
    
    /// Check if user has accepted the license
    pub fn is_accepted(&self) -> bool {
        self.imp()
            .accept_checkbox
            .borrow()
            .as_ref()
            .map(|cb| cb.is_active())
            .unwrap_or(false)
    }
}

impl Default for LicensePage {
    fn default() -> Self {
        Self::new(None)
    }
}

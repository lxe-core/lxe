//! Wizard Stack - Multi-page wizard with smooth transitions
//!
//! Uses AdwCarousel for beautiful page transitions and manages
//! the wizard flow based on installation state.

use crate::payload::PayloadInfo;
use crate::state::WizardMode;
use crate::ui::pages::{CompletePage, MaintenancePage, ProgressPage, WelcomePage};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct WizardStack {
        pub carousel: RefCell<Option<adw::Carousel>>,
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub wizard_mode: RefCell<WizardMode>,
        
        // Page references
        pub welcome_page: RefCell<Option<WelcomePage>>,
        pub progress_page: RefCell<Option<ProgressPage>>,
        pub complete_page: RefCell<Option<CompletePage>>,
        pub maintenance_page: RefCell<Option<MaintenancePage>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WizardStack {
        const NAME: &'static str = "LxeWizardStack";
        type Type = super::WizardStack;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for WizardStack {
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
                    glib::subclass::Signal::builder("installation-complete")
                        .build(),
                    glib::subclass::Signal::builder("installation-cancelled")
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for WizardStack {}
    impl BoxImpl for WizardStack {}
}

glib::wrapper! {
    pub struct WizardStack(ObjectSubclass<imp::WizardStack>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl WizardStack {
    pub fn new(payload_info: Option<PayloadInfo>, wizard_mode: WizardMode) -> Self {
        let obj: Self = glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .property("vexpand", true)
            .build();
        
        let imp = obj.imp();
        *imp.payload_info.borrow_mut() = payload_info;
        *imp.wizard_mode.borrow_mut() = wizard_mode;
        
        // CRITICAL: setup_ui() must be called AFTER payload_info is set!
        // Previously this was in constructed() which runs before new() sets payload_info
        obj.setup_ui();
        
        obj
    }
    
    fn setup_ui(&self) {
        let imp = self.imp();
        let wizard_mode = imp.wizard_mode.borrow().clone();
        let payload_info = imp.payload_info.borrow().clone();
        
        // Create carousel for smooth page transitions
        let carousel = adw::Carousel::builder()
            .interactive(false) // Disable swipe - controlled programmatically
            .allow_scroll_wheel(false)
            .allow_mouse_drag(false)
            .vexpand(true)
            .build();
        
        // Create carousel indicator dots
        let dots = adw::CarouselIndicatorDots::builder()
            .carousel(&carousel)
            .halign(gtk::Align::Center)
            .margin_top(12)
            .build();
        
        match wizard_mode {
            WizardMode::Install => {
                self.setup_install_flow(&carousel, payload_info);
            }
            WizardMode::Maintenance { .. } => {
                self.setup_maintenance_flow(&carousel, payload_info, wizard_mode);
            }
        }
        
        self.append(&carousel);
        self.append(&dots);
        
        *imp.carousel.borrow_mut() = Some(carousel);
    }
    
    fn setup_install_flow(
        &self,
        carousel: &adw::Carousel,
        payload_info: Option<PayloadInfo>,
    ) {
        let imp = self.imp();
        
        // Welcome page
        let welcome_page = WelcomePage::new(payload_info.clone());
        carousel.append(&welcome_page);
        
        // Progress page
        let progress_page = ProgressPage::new(payload_info.clone());
        carousel.append(&progress_page);
        
        // Complete page
        let complete_page = CompletePage::new(payload_info.clone(), false);
        carousel.append(&complete_page);
        
        // Connect welcome -> progress transition
        welcome_page.connect_local(
            "install-clicked",
            false,
            glib::clone!(@weak carousel, @weak progress_page => @default-return None, move |_| {
                carousel.scroll_to(&progress_page, true);
                progress_page.start_installation();
                None
            }),
        );
        
        // Connect progress -> complete transition
        progress_page.connect_local(
            "extraction-complete",
            false,
            glib::clone!(@weak carousel, @weak complete_page => @default-return None, move |_| {
                carousel.scroll_to(&complete_page, true);
                None
            }),
        );
        
        // Store page references
        *imp.welcome_page.borrow_mut() = Some(welcome_page);
        *imp.progress_page.borrow_mut() = Some(progress_page);
        *imp.complete_page.borrow_mut() = Some(complete_page);
    }
    
    fn setup_maintenance_flow(
        &self,
        carousel: &adw::Carousel,
        payload_info: Option<PayloadInfo>,
        wizard_mode: WizardMode,
    ) {
        let imp = self.imp();
        
        // Maintenance page (uninstall/repair/upgrade options)
        let maintenance_page = MaintenancePage::new(payload_info.clone(), wizard_mode.clone());
        carousel.append(&maintenance_page);
        
        // Progress page (for uninstall/repair operations)
        let progress_page = ProgressPage::new(payload_info.clone());
        carousel.append(&progress_page);
        
        // Complete page
        let is_uninstall = true; // Will be determined by action
        let complete_page = CompletePage::new(payload_info, is_uninstall);
        carousel.append(&complete_page);
        
        // Connect maintenance actions
        maintenance_page.connect_local(
            "action-selected",
            false,
            glib::clone!(@weak carousel, @weak progress_page => @default-return None, move |values: &[glib::Value]| {
                let action = values[1].get::<String>().unwrap_or_default();
                
                carousel.scroll_to(&progress_page, true);
                
                match action.as_str() {
                    "uninstall" => progress_page.start_uninstallation(),
                    // "repair" => progress_page.start_repair(),
                    _ => tracing::warn!("Unknown action: {}", action),
                }
                None
            }),
        );
        
        // Connect progress -> complete
        progress_page.connect_local(
            "extraction-complete",
            false,
            glib::clone!(@weak carousel, @weak complete_page => @default-return None, move |_| {
                carousel.scroll_to(&complete_page, true);
                None
            }),
        );
        
        *imp.maintenance_page.borrow_mut() = Some(maintenance_page);
        *imp.progress_page.borrow_mut() = Some(progress_page);
        *imp.complete_page.borrow_mut() = Some(complete_page);
    }
    
    /// Navigate to a specific page by index
    pub fn go_to_page(&self, index: u32) {
        if let Some(ref carousel) = *self.imp().carousel.borrow() {
            let page = carousel.nth_page(index);
            carousel.scroll_to(&page, true);
        }
    }
}

impl Default for WizardStack {
    fn default() -> Self {
        Self::new(None, WizardMode::Install)
    }
}

impl Default for WizardMode {
    fn default() -> Self {
        WizardMode::Install
    }
}

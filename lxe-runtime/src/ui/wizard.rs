//! Wizard Stack - Multi-page wizard with smooth transitions
//!
//! Uses GtkStack for clean page transitions and manages
//! the wizard flow based on installation state.

use crate::payload::PayloadInfo;
use crate::state::WizardMode;
use crate::ui::pages::{CompletePage, LicensePage, MaintenancePage, ProgressPage, WelcomePage};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct WizardStack {
        pub stack: RefCell<Option<gtk::Stack>>,
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub wizard_mode: RefCell<WizardMode>,
        
        // Page references
        pub welcome_page: RefCell<Option<WelcomePage>>,
        pub license_page: RefCell<Option<LicensePage>>,
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
        
        // Create stack for clean page transitions
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(300)
            .vexpand(true)
            .hexpand(true)
            .build();
        
        match wizard_mode {
            WizardMode::Install => {
                self.setup_install_flow(&stack, payload_info);
            }
            WizardMode::Maintenance { .. } => {
                self.setup_maintenance_flow(&stack, payload_info, wizard_mode);
            }
        }
        
        self.append(&stack);
        
        *imp.stack.borrow_mut() = Some(stack);
    }
    
    fn setup_install_flow(
        &self,
        stack: &gtk::Stack,
        payload_info: Option<PayloadInfo>,
    ) {
        let imp = self.imp();
        
        // Check if license page should be shown
        let has_license = payload_info
            .as_ref()
            .and_then(|p| p.metadata.installer.license_text.as_ref())
            .is_some();
        
        // Welcome page (always first)
        let welcome_page = WelcomePage::new(payload_info.clone());
        stack.add_named(&welcome_page, Some("welcome"));
        
        // License page (only if license_text is present)
        let license_page = if has_license {
            let page = LicensePage::new(payload_info.clone());
            stack.add_named(&page, Some("license"));
            Some(page)
        } else {
            None
        };
        
        // Progress page
        let progress_page = ProgressPage::new(payload_info.clone());
        stack.add_named(&progress_page, Some("progress"));
        
        // Complete page
        let complete_page = CompletePage::new(payload_info.clone(), false);
        stack.add_named(&complete_page, Some("complete"));
        
        // Connect navigation based on whether license page exists
        if let Some(ref license_pg) = license_page {
            // Welcome -> License
            welcome_page.connect_local(
                "install-clicked",
                false,
                glib::clone!(@weak stack, @weak license_pg as lp => @default-return None, move |_| {
                    stack.set_visible_child(&lp);
                    None
                }),
            );
            
            // License -> Progress (when accepted)
            license_pg.connect_local(
                "next-clicked",
                false,
                glib::clone!(@weak stack, @weak progress_page => @default-return None, move |_| {
                    stack.set_visible_child(&progress_page);
                    
                    // Delay start to allow transition to complete/start smoothly
                    let page = progress_page.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
                        page.start_installation();
                        glib::ControlFlow::Break
                    });
                    
                    None
                }),
            );
            
            // License <- back to Welcome
            license_pg.connect_local(
                "back-clicked",
                false,
                glib::clone!(@weak stack, @weak welcome_page => @default-return None, move |_| {
                    stack.set_visible_child(&welcome_page);
                    None
                }),
            );
        } else {
            // No license - Welcome -> Progress directly
            welcome_page.connect_local(
                "install-clicked",
                false,
                glib::clone!(@weak stack, @weak progress_page => @default-return None, move |_| {
                    stack.set_visible_child(&progress_page);
                    
                    // Delay start to allow transition to complete/start smoothly
                    let page = progress_page.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
                        page.start_installation();
                        glib::ControlFlow::Break
                    });
                    
                    None
                }),
            );
        }
        
        // Connect progress -> complete transition
        progress_page.connect_local(
            "extraction-complete",
            false,
            glib::clone!(@weak stack, @weak complete_page => @default-return None, move |_| {
                stack.set_visible_child(&complete_page);
                None
            }),
        );
        
        *imp.license_page.borrow_mut() = license_page;
        
        // Store page references
        *imp.welcome_page.borrow_mut() = Some(welcome_page);
        *imp.progress_page.borrow_mut() = Some(progress_page);
        *imp.complete_page.borrow_mut() = Some(complete_page);
    }
    
    fn setup_maintenance_flow(
        &self,
        stack: &gtk::Stack,
        payload_info: Option<PayloadInfo>,
        wizard_mode: WizardMode,
    ) {
        let imp = self.imp();
        
        // Maintenance page (uninstall/repair/upgrade options)
        let maintenance_page = MaintenancePage::new(payload_info.clone(), wizard_mode.clone());
        stack.add_named(&maintenance_page, Some("maintenance"));
        
        // Progress page (for uninstall/repair operations)
        let progress_page = ProgressPage::new(payload_info.clone());
        stack.add_named(&progress_page, Some("progress"));
        
        // Complete page
        let is_uninstall = true; // Will be determined by action
        let complete_page = CompletePage::new(payload_info, is_uninstall);
        stack.add_named(&complete_page, Some("complete"));
        
        // Connect maintenance actions
        maintenance_page.connect_local(
            "action-selected",
            false,
            glib::clone!(@weak stack, @weak progress_page => @default-return None, move |values: &[glib::Value]| {
                let action = values[1].get::<String>().unwrap_or_default();
                
                stack.set_visible_child(&progress_page);
                
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
            glib::clone!(@weak stack, @weak complete_page => @default-return None, move |_| {
                stack.set_visible_child(&complete_page);
                None
            }),
        );
        
        *imp.maintenance_page.borrow_mut() = Some(maintenance_page);
        *imp.progress_page.borrow_mut() = Some(progress_page);
        *imp.complete_page.borrow_mut() = Some(complete_page);
    }
    
    /// Navigate to a specific page by name
    pub fn go_to_page(&self, name: &str) {
        if let Some(ref stack) = *self.imp().stack.borrow() {
            stack.set_visible_child_name(name);
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

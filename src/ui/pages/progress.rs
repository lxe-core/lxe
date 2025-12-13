//! Progress Page - Shows extraction progress with smooth animations
//!
//! ARCHITECTURE: Uses a separate OS thread for the Tokio runtime to avoid
//! blocking the GTK main thread. Communication happens via std::sync::mpsc
//! and glib::idle_add for thread-safe UI updates.

use crate::extractor::{self, ExtractProgress};
use crate::installer::{self, InstallConfig};
use crate::payload::PayloadInfo;
use crate::polkit;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;

/// Messages sent from worker thread to GTK main thread
#[derive(Debug, Clone)]
pub enum ProgressMessage {
    /// Progress update during extraction
    Update(ExtractProgress),
    /// Extraction phase complete, starting installation
    InstallingDesktopEntry,
    /// All operations complete
    Complete,
    /// An error occurred
    Error(String),
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ProgressPage {
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub progress_bar: RefCell<Option<gtk::ProgressBar>>,
        pub status_label: RefCell<Option<gtk::Label>>,
        pub file_label: RefCell<Option<gtk::Label>>,
        pub percent_label: RefCell<Option<gtk::Label>>,
        pub is_uninstall: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProgressPage {
        const NAME: &'static str = "LxeProgressPage";
        type Type = super::ProgressPage;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for ProgressPage {
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
                    glib::subclass::Signal::builder("extraction-complete")
                        .build(),
                    glib::subclass::Signal::builder("extraction-failed")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for ProgressPage {}
    impl BoxImpl for ProgressPage {}
}

glib::wrapper! {
    pub struct ProgressPage(ObjectSubclass<imp::ProgressPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ProgressPage {
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
        let imp = self.imp();
        
        // DEBUG: Warn if payload_info is None (See UI_DEVELOPMENT.md)
        #[cfg(debug_assertions)]
        if imp.payload_info.borrow().is_none() {
            tracing::warn!("ProgressPage::setup_ui called with no payload_info");
        }
        
        // Animated spinner
        let spinner = gtk::Spinner::builder()
            .width_request(48)
            .height_request(48)
            .spinning(true)
            .margin_bottom(16)
            .build();
        
        // Status label
        let status_label = gtk::Label::builder()
            .label("Preparing installation...")
            .css_classes(["title-2"])
            .build();
        
        // Progress bar with smooth animation
        let progress_bar = gtk::ProgressBar::builder()
            .show_text(false)
            .margin_top(8)
            .width_request(300)
            .halign(gtk::Align::Center)
            .css_classes(["osd"])
            .build();
        
        // Progress percentage
        let percent_label = gtk::Label::builder()
            .label("0%")
            .css_classes(["title-3", "numeric"])
            .margin_top(8)
            .build();
        
        // Current file label
        let file_label = gtk::Label::builder()
            .label("")
            .css_classes(["caption", "dim-label"])
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .max_width_chars(50)
            .margin_top(4)
            .build();
        
        self.append(&spinner);
        self.append(&status_label);
        self.append(&progress_bar);
        self.append(&percent_label);
        self.append(&file_label);
        
        *imp.progress_bar.borrow_mut() = Some(progress_bar);
        *imp.status_label.borrow_mut() = Some(status_label);
        *imp.file_label.borrow_mut() = Some(file_label);
        *imp.percent_label.borrow_mut() = Some(percent_label);
    }
    
    /// Start the installation process
    pub fn start_installation(&self) {
        let imp = self.imp();
        *imp.is_uninstall.borrow_mut() = false;
        
        if let Some(ref status) = *imp.status_label.borrow() {
            status.set_label("Installing...");
        }
        
        let payload_info = imp.payload_info.borrow().clone();
        
        if let Some(payload) = payload_info {
            self.run_extraction(payload, false);
        } else {
            // Demo mode - simulate progress
            self.simulate_progress();
        }
    }
    
    /// Start the uninstallation process
    pub fn start_uninstallation(&self) {
        let imp = self.imp();
        *imp.is_uninstall.borrow_mut() = true;
        
        if let Some(ref status) = *imp.status_label.borrow() {
            status.set_label("Uninstalling...");
        }
        
        // TODO: Implement actual uninstallation with same worker pattern
        self.simulate_progress();
    }
    
    /// Run extraction in a SEPARATE THREAD to avoid blocking GTK main loop
    fn run_extraction(&self, payload: PayloadInfo, is_system: bool) {
        let page = self.clone();
        
        // Create an std::sync::mpsc channel for cross-thread communication
        let (sender, receiver) = mpsc::channel::<ProgressMessage>();
        
        // Get installation config
        let config = if is_system {
            InstallConfig::system()
        } else {
            InstallConfig::user_local()
        };
        let target_dir = config.base_dir.join("share");
        
        // Spawn a NATIVE OS THREAD for the worker
        // This thread will have its own Tokio runtime
        // The GTK main thread remains free to process events
        thread::spawn(move || {
            // Create Tokio runtime inside the worker thread
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = sender.send(ProgressMessage::Error(
                        format!("Failed to initialize async runtime: {}", e)
                    ));
                    return;
                }
            };
            
            // Run all async operations inside this thread's runtime
            rt.block_on(async {
                // Check polkit authorization for system installs
                if is_system {
                    if !polkit::is_root() {
                        match polkit::request_authorization(polkit::ACTION_INSTALL_SYSTEM).await {
                            Ok(true) => {
                                tracing::info!("Polkit authorization granted");
                            }
                            Ok(false) => {
                                let _ = sender.send(ProgressMessage::Error(
                                    "Authorization denied. Cannot install system-wide.".to_string()
                                ));
                                return;
                            }
                            Err(e) => {
                                let _ = sender.send(ProgressMessage::Error(
                                    format!("Authorization failed: {}", e)
                                ));
                                return;
                            }
                        }
                    }
                }
                
                // Start extraction
                let (mut rx, handle) = extractor::extract_async(payload.clone(), target_dir.clone());
                
                // Forward progress updates to GTK thread via channel
                let sender_clone = sender.clone();
                let progress_forwarder = tokio::spawn(async move {
                    while rx.changed().await.is_ok() {
                        let progress = rx.borrow().clone();
                        let is_complete = progress.complete;
                        
                        if sender_clone.send(ProgressMessage::Update(progress)).is_err() {
                            break; // Receiver dropped
                        }
                        
                        if is_complete {
                            break;
                        }
                    }
                });
                
                // Wait for extraction to complete
                let extraction_result = handle.await;
                
                // Ensure progress forwarder is done
                let _ = progress_forwarder.await;
                
                // Handle extraction result
                match extraction_result {
                    Ok(Ok(())) => {
                        // Extraction successful, now install desktop files
                        let _ = sender.send(ProgressMessage::InstallingDesktopEntry);
                        
                        // Create .desktop file
                        if let Err(e) = installer::create_desktop_entry(&payload.metadata, &config).await {
                            let _ = sender.send(ProgressMessage::Error(e.to_string()));
                            return;
                        }
                        
                        // Create symlink in bin
                        if let Err(e) = installer::create_bin_symlink(&payload.metadata, &config).await {
                            // Non-fatal - log and continue
                            tracing::warn!("Could not create bin symlink: {}", e);
                        }
                        
                        // Install icon
                        if payload.metadata.icon.is_some() {
                            if let Err(e) = installer::install_icon(&payload.metadata, &config).await {
                                tracing::warn!("Could not install icon: {}", e);
                            }
                        }
                        
                        let _ = sender.send(ProgressMessage::Complete);
                    }
                    Ok(Err(e)) => {
                        let _ = sender.send(ProgressMessage::Error(e.to_string()));
                    }
                    Err(e) => {
                        let _ = sender.send(ProgressMessage::Error(format!("Task panicked: {}", e)));
                    }
                }
            });
        });
        
        // Poll the receiver from the GTK main thread using glib::timeout_add_local
        // This runs periodically on the GTK main loop, checking for new messages
        let receiver = Rc::new(RefCell::new(Some(receiver)));
        let receiver_clone = receiver.clone();
        
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            let mut should_continue = true;
            
            // Try to receive all pending messages
            if let Some(ref rx) = *receiver_clone.borrow() {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        ProgressMessage::Update(progress) => {
                            page.update_progress(&progress);
                        }
                        ProgressMessage::InstallingDesktopEntry => {
                            page.set_status("Installing shortcuts...");
                        }
                        ProgressMessage::Complete => {
                            page.emit_by_name::<()>("extraction-complete", &[]);
                            should_continue = false;
                            break;
                        }
                        ProgressMessage::Error(err) => {
                            page.emit_by_name::<()>("extraction-failed", &[&err]);
                            should_continue = false;
                            break;
                        }
                    }
                }
            }
            
            if should_continue {
                glib::ControlFlow::Continue
            } else {
                // Clean up receiver
                *receiver_clone.borrow_mut() = None;
                glib::ControlFlow::Break
            }
        });
    }
    
    fn set_status(&self, status: &str) {
        if let Some(ref label) = *self.imp().status_label.borrow() {
            label.set_label(status);
        }
    }
    
    fn update_progress(&self, progress: &ExtractProgress) {
        let imp = self.imp();
        
        if let Some(ref bar) = *imp.progress_bar.borrow() {
            bar.set_fraction(progress.fraction());
        }
        
        if let Some(ref label) = *imp.file_label.borrow() {
            label.set_label(&progress.current_file);
        }
        
        if let Some(ref label) = *imp.percent_label.borrow() {
            label.set_label(&format!("{}%", (progress.fraction() * 100.0) as u32));
        }
    }
    
    /// Simulate progress for demo mode
    fn simulate_progress(&self) {
        let page = self.clone();
        
        glib::spawn_future_local(async move {
            let files = [
                "Preparing files...",
                "Extracting application...",
                "Installing dependencies...",
                "Creating shortcuts...",
                "Finishing up...",
            ];
            
            for (i, file) in files.iter().enumerate() {
                let progress = ExtractProgress {
                    total_bytes: 100,
                    extracted_bytes: ((i + 1) * 20) as u64,
                    files_extracted: (i + 1) as u32,
                    current_file: file.to_string(),
                    complete: i == files.len() - 1,
                    error: None,
                };
                
                page.update_progress(&progress);
                
                // Simulate work - this doesn't block because we're in spawn_future_local
                glib::timeout_future(std::time::Duration::from_millis(500)).await;
            }
            
            page.emit_by_name::<()>("extraction-complete", &[]);
        });
    }
}

impl Default for ProgressPage {
    fn default() -> Self {
        Self::new(None)
    }
}

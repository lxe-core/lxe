//! LXE Window - Frameless Adaptive Glass Window
//!
//! Creates a frameless, draggable window with the "Adaptive Glass" aesthetic.
//! Uses GtkWindowHandle to make the entire window draggable.

use crate::payload::PayloadInfo;
use crate::state::WizardMode;
use crate::ui::app::LxeApplication;
use crate::ui::wizard::WizardStack;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(string = r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <interface>
            <template class="LxeWindow" parent="AdwApplicationWindow">
                <property name="default-width">750</property>
                <property name="default-height">450</property>
                <property name="resizable">false</property>
                <property name="decorated">false</property>
                <style>
                    <class name="lxe-window"/>
                </style>
                <property name="content">
                    <!-- GtkWindowHandle is standard GTK4 (no Libadwaita dependency) -->
                    <object class="GtkWindowHandle">
                        <property name="child">
                            <object class="GtkOverlay" id="overlay">
                                <!-- Main content -->
                                <property name="child">
                                    <object class="GtkBox" id="main_box">
                                        <property name="orientation">vertical</property>
                                        <property name="margin-start">24</property>
                                        <property name="margin-end">24</property>
                                        <property name="margin-top">16</property>
                                        <property name="margin-bottom">24</property>
                                        
                                        <!-- Window controls (close button) -->
                                        <child>
                                            <object class="GtkBox" id="header_box">
                                                <property name="halign">end</property>
                                                <child>
                                                    <object class="GtkButton" id="close_button">
                                                        <property name="icon-name">window-close-symbolic</property>
                                                        <property name="valign">center</property>
                                                        <property name="css-classes">circular</property>
                                                        <property name="css-classes">flat</property>
                                                        <property name="tooltip-text">Close</property>
                                                    </object>
                                                </child>
                                            </object>
                                        </child>
                                        
                                        <!-- Wizard content area -->
                                        <child>
                                            <object class="GtkBox" id="content_box">
                                                <property name="orientation">vertical</property>
                                                <property name="vexpand">true</property>
                                                <property name="valign">fill</property>
                                            </object>
                                        </child>
                                    </object>
                                </property>
                            </object>
                        </property>
                    </object>
                </property>
            </template>
        </interface>
    "#)]
    pub struct LxeWindow {
        #[template_child]
        pub content_box: TemplateChild<gtk::Box>,
        
        #[template_child]
        pub close_button: TemplateChild<gtk::Button>,
        
        pub payload_info: RefCell<Option<PayloadInfo>>,
        pub wizard_mode: RefCell<WizardMode>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LxeWindow {
        const NAME: &'static str = "LxeWindow";
        type Type = super::LxeWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LxeWindow {
        fn constructed(&self) {
            self.parent_constructed();
            
            let window = self.obj();
            
            // Connect close button
            self.close_button.connect_clicked(glib::clone!(
                @weak window =>
                move |_| {
                    window.close();
                }
            ));
        }
    }

    impl WidgetImpl for LxeWindow {}
    impl WindowImpl for LxeWindow {}
    impl ApplicationWindowImpl for LxeWindow {}
    impl AdwApplicationWindowImpl for LxeWindow {}

    impl Default for LxeWindow {
        fn default() -> Self {
            Self {
                content_box: TemplateChild::default(),
                close_button: TemplateChild::default(),
                payload_info: RefCell::new(None),
                wizard_mode: RefCell::new(WizardMode::Install),
            }
        }
    }
}

glib::wrapper! {
    pub struct LxeWindow(ObjectSubclass<imp::LxeWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl LxeWindow {
    pub fn new(
        app: &LxeApplication,
        payload_info: Option<PayloadInfo>,
        wizard_mode: WizardMode,
    ) -> Self {
        let window: Self = glib::Object::builder()
            .property("application", app)
            .build();
        
        let imp = window.imp();
        *imp.payload_info.borrow_mut() = payload_info.clone();
        *imp.wizard_mode.borrow_mut() = wizard_mode.clone();
        
        // Create and add the wizard
        let wizard = WizardStack::new(payload_info, wizard_mode);
        imp.content_box.append(&wizard);
        
        // Connect wizard completion to window close
        wizard.connect_local(
            "installation-complete",
            false,
            glib::clone!(@weak window => @default-return None, move |_| {
                    // Don't close immediately - let user see the success page
                    None
                }
            ),
        );
        
        window
    }
}

use std::cell::{OnceCell, RefCell};
use std::collections::VecDeque;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationIcon, NSAboutPanelOptionApplicationVersion, NSAboutPanelOptionKey,
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSAlertStyle, NSApplication,
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSButton,
    NSEventModifierFlags, NSImage, NSMenu, NSMenuItem, NSTextField, NSView,
};
use objc2_foundation::{
    NSData, NSDictionary, NSObject, NSPoint, NSRect, NSSize, NSString, ns_string,
};
use std::path::{Path, PathBuf};

use crate::shell::StudioCommand;

const LOGO_BYTES: &[u8] = include_bytes!("../assets/logo.png");
const NEW_PROJECT_TAG: isize = 1;
const OPEN_PROJECT_TAG: isize = 2;
const SAVE_TAG: isize = 3;
const COPY_TAG: isize = 8;

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            let application = NSApplication::sharedApplication(self.mtm());
            let image = logo_image();
            let image: &AnyObject = &image;
            // SAFETY: This is AppKit's immutable, process-wide option-key
            // constant and is valid while the framework is loaded.
            let application_icon_key = unsafe { NSAboutPanelOptionApplicationIcon };
            // Supplying the version explicitly keeps unbundled development
            // builds consistent with packaged builds, whose Info.plist is
            // generated from the same Cargo package version.
            let application_version_key = unsafe { NSAboutPanelOptionApplicationVersion };
            let version = NSString::from_str(env!("CARGO_PKG_VERSION"));
            let version: &AnyObject = &version;
            let options = NSDictionary::<NSAboutPanelOptionKey, AnyObject>::from_slices(
                &[application_icon_key, application_version_key],
                &[image, version],
            );

            // SAFETY: The dictionary maps the documented application-icon key
            // to an NSImage, which is the type AppKit requires for this option.
            unsafe { application.orderFrontStandardAboutPanelWithOptions(&options) };
        }

        #[unsafe(method(performStudioMenuAction:))]
        fn perform_studio_menu_action(&self, sender: &NSMenuItem) {
            if let Some(command) = command_for_tag(sender.tag()) {
                MENU_ACTIONS.with(|actions| actions.borrow_mut().push_back(command));
            }
        }
    }
);

struct NewProjectLocationIvars {
    parent: RefCell<PathBuf>,
    path_label: Retained<NSTextField>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = NewProjectLocationIvars]
    struct NewProjectLocationTarget;

    impl NewProjectLocationTarget {
        #[unsafe(method(chooseProjectLocation:))]
        fn choose_project_location(&self, _sender: Option<&AnyObject>) {
            let current_parent = self.ivars().parent.borrow().clone();
            if let Some(parent) = rfd::FileDialog::new()
                .set_title("Choose where to create the project")
                .set_directory(&current_parent)
                .pick_folder()
            {
                self.ivars()
                    .path_label
                    .setStringValue(&NSString::from_str(&parent.display().to_string()));
                self.ivars()
                    .path_label
                    .setToolTip(Some(&NSString::from_str(&parent.display().to_string())));
                *self.ivars().parent.borrow_mut() = parent;
            }
        }
    }
);

thread_local! {
    static MENU_TARGET: OnceCell<Retained<MenuTarget>> = const { OnceCell::new() };
    static MENU_ACTIONS: RefCell<VecDeque<StudioCommand>> = const { RefCell::new(VecDeque::new()) };
}

impl MenuTarget {
    fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(());
        // SAFETY: NSObject's initializer is valid for this ivar-free subclass.
        unsafe { msg_send![super(this), init] }
    }
}

impl NewProjectLocationTarget {
    fn new(
        main_thread: MainThreadMarker,
        parent: PathBuf,
        path_label: Retained<NSTextField>,
    ) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(NewProjectLocationIvars {
            parent: RefCell::new(parent),
            path_label,
        });
        // SAFETY: NSObject's initializer is valid for this stored-value subclass.
        unsafe { msg_send![super(this), init] }
    }
}

pub(crate) fn take_menu_action() -> Option<StudioCommand> {
    MENU_ACTIONS.with(|actions| actions.borrow_mut().pop_front())
}

pub(crate) fn set_application_icon() {
    let main_thread = main_thread_marker();
    let application = NSApplication::sharedApplication(main_thread);
    let image = logo_image();

    // SAFETY: NSApplication retains the valid NSImage for its icon property.
    unsafe { application.setApplicationIconImage(Some(&image)) };
}

pub(crate) fn system_symbol_png(name: &str) -> Option<Vec<u8>> {
    let _main_thread = main_thread_marker();
    let name = NSString::from_str(name);
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&name, None)?;
    let tiff = image.TIFFRepresentation()?;
    let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let properties = NSDictionary::<NSBitmapImageRepPropertyKey, AnyObject>::new();
    // SAFETY: An empty properties dictionary is valid for PNG export and has
    // the exact key/value types required by AppKit's image representation API.
    unsafe {
        bitmap
            .representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
            .map(|data| data.to_vec())
    }
}

/// Present the first-run project flow with controls owned and laid out by
/// AppKit. Keeping this out of the shared shell gives macOS native type,
/// keyboard behavior, appearance, and accessibility without forking Studio's
/// actual editor UI.
pub(crate) fn show_new_project_dialog(
    initial_title: &str,
    initial_parent: &Path,
    initial_error: Option<&str>,
) -> Option<(String, PathBuf)> {
    let main_thread = main_thread_marker();
    let mut title = initial_title.to_owned();
    let mut parent = initial_parent.to_path_buf();
    let mut error = initial_error.map(str::to_owned);

    loop {
        let alert = NSAlert::new(main_thread);
        alert.setAlertStyle(NSAlertStyle::Informational);
        alert.setMessageText(ns_string!("New Project"));

        let information = match error.as_deref() {
            Some(message) => format!(
                "{message}\n\nCreate a starter game with its manifest, Luau entry point, and local SDK."
            ),
            None => "Create a starter game with its manifest, Luau entry point, and local SDK."
                .to_owned(),
        };
        alert.setInformativeText(&NSString::from_str(&information));
        let icon = logo_image();
        // SAFETY: `icon` is a valid NSImage retained for the alert's lifetime.
        unsafe { alert.setIcon(Some(&icon)) };

        let accessory = NSView::initWithFrame(
            NSView::alloc(main_thread),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(360.0, 102.0)),
        );
        let title_label = NSTextField::labelWithString(ns_string!("Game title"), main_thread);
        title_label.setFrame(NSRect::new(
            NSPoint::new(0.0, 84.0),
            NSSize::new(360.0, 18.0),
        ));
        let title_field =
            NSTextField::textFieldWithString(&NSString::from_str(&title), main_thread);
        title_field.setPlaceholderString(Some(ns_string!("Game title")));
        title_field.setFrame(NSRect::new(
            NSPoint::new(0.0, 54.0),
            NSSize::new(360.0, 24.0),
        ));
        let location_label = NSTextField::labelWithString(ns_string!("Location"), main_thread);
        location_label.setFrame(NSRect::new(
            NSPoint::new(0.0, 30.0),
            NSSize::new(360.0, 18.0),
        ));
        let path = parent.display().to_string();
        let path_label = NSTextField::labelWithString(&NSString::from_str(&path), main_thread);
        path_label.setFrame(NSRect::new(
            NSPoint::new(0.0, 3.0),
            NSSize::new(266.0, 20.0),
        ));
        path_label.setToolTip(Some(&NSString::from_str(&path)));

        let location_target =
            NewProjectLocationTarget::new(main_thread, parent, path_label.clone());
        let target: &AnyObject = &location_target;
        // SAFETY: `chooseProjectLocation:` is registered on the retained target
        // with the standard one-argument control action signature.
        let choose_button = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!("Choose…"),
                Some(target),
                Some(sel!(chooseProjectLocation:)),
                main_thread,
            )
        };
        choose_button.setFrame(NSRect::new(
            NSPoint::new(274.0, 0.0),
            NSSize::new(86.0, 28.0),
        ));

        accessory.addSubview(&title_label);
        accessory.addSubview(&title_field);
        accessory.addSubview(&location_label);
        accessory.addSubview(&path_label);
        accessory.addSubview(&choose_button);
        alert.setAccessoryView(Some(&accessory));

        alert.addButtonWithTitle(ns_string!("Create project"));
        alert.addButtonWithTitle(ns_string!("Cancel"));
        alert.layout();
        let alert_window = alert.window();
        alert_window.center();
        alert_window.makeFirstResponder(Some(&title_field));

        let response = alert.runModal();
        title = title_field.stringValue().to_string();
        parent = location_target.ivars().parent.borrow().clone();
        if response == NSAlertFirstButtonReturn {
            if !title.trim().is_empty() {
                return Some((title.trim().to_owned(), parent));
            }
            error = Some("Enter a game title to continue.".to_owned());
        } else if response == NSAlertSecondButtonReturn {
            return None;
        } else {
            return None;
        }
    }
}

pub(crate) fn install_native_menu() {
    let main_thread = main_thread_marker();
    let application = NSApplication::sharedApplication(main_thread);
    let Some(main_menu) = application.mainMenu() else {
        log::warn!("macOS menu installation skipped: AppKit has no main menu yet");
        return;
    };
    let Some(application_menu_item) = main_menu.itemAtIndex(0) else {
        log::warn!("macOS menu installation skipped: AppKit has no application menu item yet");
        return;
    };
    if !application_menu_item.hasSubmenu() {
        log::warn!("macOS menu installation skipped: application menu item has no submenu");
        return;
    }
    let Some(application_menu) = application_menu_item.submenu() else {
        log::warn!("macOS menu installation skipped: application menu has no submenu yet");
        return;
    };

    application_menu_item.setTitle(ns_string!("Cubacadabra Studio"));
    application_menu.setTitle(ns_string!("Cubacadabra Studio"));

    MENU_TARGET.with(|target| {
        let target = target.get_or_init(|| MenuTarget::new(main_thread));
        let target: &AnyObject = target;

        configure_application_menu(&application_menu, main_thread, target);
        install_file_menu(&main_menu, main_thread, target);
        install_edit_menu(&main_menu, main_thread, target);
        let window_menu = install_window_menu(&main_menu, main_thread, target);
        application.setWindowsMenu(Some(&window_menu));
    });
}

fn configure_application_menu(menu: &NSMenu, _main_thread: MainThreadMarker, target: &AnyObject) {
    let Some(about_item) = menu.itemAtIndex(0) else {
        log::warn!("macOS menu installation skipped: application menu has no About item");
        return;
    };
    about_item.setTitle(ns_string!("About Cubacadabra Studio"));
    // SAFETY: `showAbout:` is registered on MenuTarget with the standard
    // one-argument menu action signature. MENU_TARGET retains the target.
    unsafe {
        about_item.setTarget(Some(target));
        about_item.setAction(Some(sel!(showAbout:)));
    }

    // Winit supplies the remaining standard application commands. Rename the
    // process-derived labels so unbundled development builds still read like
    // the finished application. Winit's default menu order is About, divider,
    // Services, Hide, Hide Others, Show All, divider, Quit.
    if let Some(hide_item) = menu.itemAtIndex(3) {
        hide_item.setTitle(ns_string!("Hide Cubacadabra Studio"));
    }
    if let Some(quit_item) = menu.itemAtIndex(7) {
        quit_item.setTitle(ns_string!("Quit Cubacadabra Studio"));
    }
}

fn install_file_menu(main_menu: &NSMenu, main_thread: MainThreadMarker, target: &AnyObject) {
    let menu = NSMenu::new(main_thread);
    menu.setTitle(ns_string!("File"));
    menu.addItem(&studio_menu_item(
        main_thread,
        target,
        ns_string!("New Project…"),
        ns_string!("n"),
        NEW_PROJECT_TAG,
        None,
    ));
    menu.addItem(&studio_menu_item(
        main_thread,
        target,
        ns_string!("Open Project…"),
        ns_string!("o"),
        OPEN_PROJECT_TAG,
        None,
    ));
    menu.addItem(&studio_menu_item(
        main_thread,
        target,
        ns_string!("Save"),
        ns_string!("s"),
        SAVE_TAG,
        None,
    ));
    menu.addItem(&NSMenuItem::separatorItem(main_thread));
    menu.addItem(&standard_menu_item(
        main_thread,
        ns_string!("Close"),
        sel!(performClose:),
        ns_string!("w"),
        None,
    ));
    add_top_level_menu(main_menu, main_thread, ns_string!("File"), &menu);
}

fn install_edit_menu(main_menu: &NSMenu, main_thread: MainThreadMarker, target: &AnyObject) {
    let menu = NSMenu::new(main_thread);
    menu.setTitle(ns_string!("Edit"));
    menu.addItem(&standard_menu_item(
        main_thread,
        ns_string!("Undo"),
        sel!(undo:),
        ns_string!("z"),
        None,
    ));
    menu.addItem(&standard_menu_item(
        main_thread,
        ns_string!("Redo"),
        sel!(redo:),
        ns_string!("z"),
        Some(NSEventModifierFlags::Command | NSEventModifierFlags::Shift),
    ));
    menu.addItem(&NSMenuItem::separatorItem(main_thread));
    menu.addItem(&standard_menu_item(
        main_thread,
        ns_string!("Cut"),
        sel!(cut:),
        ns_string!("x"),
        None,
    ));
    menu.addItem(&studio_menu_item(
        main_thread,
        target,
        ns_string!("Copy"),
        ns_string!("c"),
        COPY_TAG,
        None,
    ));
    for (title, action, key) in [
        (ns_string!("Paste"), sel!(paste:), ns_string!("v")),
        (ns_string!("Select All"), sel!(selectAll:), ns_string!("a")),
    ] {
        menu.addItem(&standard_menu_item(main_thread, title, action, key, None));
    }
    add_top_level_menu(main_menu, main_thread, ns_string!("Edit"), &menu);
}

fn install_window_menu(
    main_menu: &NSMenu,
    main_thread: MainThreadMarker,
    _target: &AnyObject,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(main_thread);
    menu.setTitle(ns_string!("Window"));
    menu.addItem(&standard_menu_item(
        main_thread,
        ns_string!("Minimize"),
        sel!(performMiniaturize:),
        ns_string!("m"),
        None,
    ));
    menu.addItem(&standard_menu_item(
        main_thread,
        ns_string!("Zoom"),
        sel!(performZoom:),
        ns_string!(""),
        None,
    ));
    menu.addItem(&NSMenuItem::separatorItem(main_thread));
    menu.addItem(&standard_menu_item(
        main_thread,
        ns_string!("Bring All to Front"),
        sel!(arrangeInFront:),
        ns_string!(""),
        None,
    ));
    add_top_level_menu(main_menu, main_thread, ns_string!("Window"), &menu);
    menu
}

fn add_top_level_menu(
    main_menu: &NSMenu,
    main_thread: MainThreadMarker,
    title: &NSString,
    submenu: &NSMenu,
) {
    let item = NSMenuItem::new(main_thread);
    item.setTitle(title);
    item.setSubmenu(Some(submenu));
    main_menu.addItem(&item);
}

fn studio_menu_item(
    main_thread: MainThreadMarker,
    target: &AnyObject,
    title: &NSString,
    key: &NSString,
    tag: isize,
    modifiers: Option<NSEventModifierFlags>,
) -> Retained<NSMenuItem> {
    let item = standard_menu_item(
        main_thread,
        title,
        sel!(performStudioMenuAction:),
        key,
        modifiers,
    );
    item.setTag(tag);
    // SAFETY: `performStudioMenuAction:` is registered on MenuTarget with the
    // standard one-argument menu action signature. MENU_TARGET retains it.
    unsafe { item.setTarget(Some(target)) };
    item
}

fn standard_menu_item(
    main_thread: MainThreadMarker,
    title: &NSString,
    action: Sel,
    key: &NSString,
    modifiers: Option<NSEventModifierFlags>,
) -> Retained<NSMenuItem> {
    // SAFETY: Every supplied selector has the standard one-argument NSMenuItem
    // action signature, and AppKit retains the returned menu item.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(main_thread),
            title,
            Some(action),
            key,
        )
    };
    if let Some(modifiers) = modifiers {
        item.setKeyEquivalentModifierMask(modifiers);
    }
    item
}

fn logo_image() -> Retained<NSImage> {
    let data = NSData::with_bytes(LOGO_BYTES);
    NSImage::initWithData(NSImage::alloc(), &data)
        .expect("Cubacadabra Studio logo should be a valid image")
}

fn main_thread_marker() -> MainThreadMarker {
    MainThreadMarker::new().expect("Studio's event loop must run on the main thread")
}

fn command_for_tag(tag: isize) -> Option<StudioCommand> {
    match tag {
        NEW_PROJECT_TAG => Some(StudioCommand::NewProject),
        OPEN_PROJECT_TAG => Some(StudioCommand::OpenProject),
        SAVE_TAG => Some(StudioCommand::Save),
        COPY_TAG => Some(StudioCommand::Copy),
        _ => None,
    }
}

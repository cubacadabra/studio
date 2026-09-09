use std::cell::{OnceCell, RefCell};
use std::collections::VecDeque;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationIcon, NSAboutPanelOptionKey, NSApplication, NSEventModifierFlags,
    NSImage, NSMenu, NSMenuItem,
};
use objc2_foundation::{NSData, NSDictionary, NSObject, NSString, ns_string};

use crate::shell::StudioCommand;

const LOGO_BYTES: &[u8] = include_bytes!("../assets/logo.png");
const OPEN_PROJECT_TAG: isize = 1;
const SAVE_TAG: isize = 2;
const REVEAL_PROJECT_TAG: isize = 3;
const PREFERENCES_TAG: isize = 4;
const MAXIMIZE_VIEWPORT_TAG: isize = 5;
const RESET_LAYOUT_TAG: isize = 6;

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
            let options = NSDictionary::<NSAboutPanelOptionKey, AnyObject>::from_slices(
                &[application_icon_key],
                &[image],
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

pub(crate) fn install_native_menu() {
    let main_thread = main_thread_marker();
    let application = NSApplication::sharedApplication(main_thread);
    let main_menu = application
        .mainMenu()
        .expect("Winit should install the macOS application menu before resuming");
    let application_menu_item = main_menu
        .itemAtIndex(0)
        .expect("Winit should install the macOS application menu item");
    let application_menu = application_menu_item
        .submenu()
        .expect("Winit's first macOS menu should be the application menu");

    application_menu_item.setTitle(ns_string!("Cubacadabra Studio"));
    application_menu.setTitle(ns_string!("Cubacadabra Studio"));

    MENU_TARGET.with(|target| {
        let target = target.get_or_init(|| MenuTarget::new(main_thread));
        let target: &AnyObject = target;

        configure_application_menu(&application_menu, main_thread, target);
        install_file_menu(&main_menu, main_thread, target);
        install_edit_menu(&main_menu, main_thread);
        let window_menu = install_window_menu(&main_menu, main_thread, target);
        application.setWindowsMenu(Some(&window_menu));
    });
}

fn configure_application_menu(menu: &NSMenu, main_thread: MainThreadMarker, target: &AnyObject) {
    let about_item = menu
        .itemAtIndex(0)
        .expect("Winit's application menu should contain an About item");
    about_item.setTitle(ns_string!("About Cubacadabra Studio"));
    // SAFETY: `showAbout:` is registered on MenuTarget with the standard
    // one-argument menu action signature. MENU_TARGET retains the target.
    unsafe {
        about_item.setTarget(Some(target));
        about_item.setAction(Some(sel!(showAbout:)));
    }

    let preferences = studio_menu_item(
        main_thread,
        target,
        ns_string!("Settings…"),
        ns_string!(","),
        PREFERENCES_TAG,
        None,
    );
    menu.insertItem_atIndex(&preferences, 1);

    // Winit supplies the remaining standard application commands. Rename the
    // process-derived labels so unbundled development builds still read like
    // the finished application.
    if let Some(hide_item) = menu.itemAtIndex(4) {
        hide_item.setTitle(ns_string!("Hide Cubacadabra Studio"));
    }
    if let Some(quit_item) = menu.itemAtIndex(8) {
        quit_item.setTitle(ns_string!("Quit Cubacadabra Studio"));
    }
}

fn install_file_menu(main_menu: &NSMenu, main_thread: MainThreadMarker, target: &AnyObject) {
    let menu = NSMenu::new(main_thread);
    menu.setTitle(ns_string!("File"));
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
    menu.addItem(&studio_menu_item(
        main_thread,
        target,
        ns_string!("Reveal Project"),
        ns_string!(""),
        REVEAL_PROJECT_TAG,
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

fn install_edit_menu(main_menu: &NSMenu, main_thread: MainThreadMarker) {
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
    for (title, action, key) in [
        (ns_string!("Cut"), sel!(cut:), ns_string!("x")),
        (ns_string!("Copy"), sel!(copy:), ns_string!("c")),
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
    target: &AnyObject,
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
    menu.addItem(&studio_menu_item(
        main_thread,
        target,
        ns_string!("Maximize Viewport"),
        ns_string!(" "),
        MAXIMIZE_VIEWPORT_TAG,
        Some(NSEventModifierFlags::empty()),
    ));
    menu.addItem(&studio_menu_item(
        main_thread,
        target,
        ns_string!("Reset Layout"),
        ns_string!(""),
        RESET_LAYOUT_TAG,
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
        OPEN_PROJECT_TAG => Some(StudioCommand::OpenProject),
        SAVE_TAG => Some(StudioCommand::Save),
        REVEAL_PROJECT_TAG => Some(StudioCommand::RevealProject),
        PREFERENCES_TAG => Some(StudioCommand::Preferences),
        MAXIMIZE_VIEWPORT_TAG => Some(StudioCommand::MaximizeViewport),
        RESET_LAYOUT_TAG => Some(StudioCommand::ResetLayout),
        _ => None,
    }
}

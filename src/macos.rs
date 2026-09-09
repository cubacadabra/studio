use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationIcon, NSAboutPanelOptionKey, NSApplication, NSImage,
};
use objc2_foundation::{NSData, NSDictionary, NSObject};

const LOGO_BYTES: &[u8] = include_bytes!("../assets/logo.png");

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    struct AboutPanelTarget;

    impl AboutPanelTarget {
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
    }
);

thread_local! {
    static ABOUT_PANEL_TARGET: OnceCell<Retained<AboutPanelTarget>> = const { OnceCell::new() };
}

impl AboutPanelTarget {
    fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(());
        // SAFETY: NSObject's initializer is valid for this ivar-free subclass.
        unsafe { msg_send![super(this), init] }
    }
}

fn logo_image() -> Retained<NSImage> {
    let data = NSData::with_bytes(LOGO_BYTES);
    NSImage::initWithData(NSImage::alloc(), &data)
        .expect("Cubacadabra Studio logo should be a valid image")
}

pub(crate) fn set_application_icon() {
    let main_thread =
        MainThreadMarker::new().expect("Studio's event loop must run on the main thread");
    let application = NSApplication::sharedApplication(main_thread);
    let image = logo_image();

    // Winit's native macOS menu already provides the standard About item. Set
    // the application icon before the menu is initialized so AppKit uses the
    // Cubacadabra logo in that panel (and for the app icon generally).
    unsafe { application.setApplicationIconImage(Some(&image)) };
}

pub(crate) fn install_about_panel_handler() {
    let main_thread =
        MainThreadMarker::new().expect("Studio's event loop must run on the main thread");
    let application = NSApplication::sharedApplication(main_thread);
    let main_menu = application
        .mainMenu()
        .expect("Winit should install the macOS application menu before resuming");
    let application_menu = main_menu
        .itemAtIndex(0)
        .and_then(|item| item.submenu())
        .expect("Winit's first macOS menu should be the application menu");
    let about_item = application_menu
        .itemAtIndex(0)
        .expect("Winit's application menu should contain an About item");

    ABOUT_PANEL_TARGET.with(|target| {
        let target = target.get_or_init(|| AboutPanelTarget::new(main_thread));
        let target: &AnyObject = target;
        // SAFETY: `showAbout:` is registered on AboutPanelTarget with the
        // standard one-argument menu action signature, and the retained target
        // outlives the menu item's weak target reference.
        unsafe {
            about_item.setTarget(Some(target));
            about_item.setAction(Some(sel!(showAbout:)));
        }
    });
}

use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::NSData;

const LOGO_BYTES: &[u8] = include_bytes!("../assets/logo.png");

pub(crate) fn set_application_icon() {
    let main_thread =
        MainThreadMarker::new().expect("Studio's event loop must run on the main thread");
    let application = NSApplication::sharedApplication(main_thread);
    let data = NSData::with_bytes(LOGO_BYTES);
    let image = NSImage::initWithData(NSImage::alloc(), &data)
        .expect("Cubacadabra Studio logo should be a valid image");

    // Winit's native macOS menu already provides the standard About item. Set
    // the application icon before the menu is initialized so AppKit uses the
    // Cubacadabra logo in that panel (and for the app icon generally).
    unsafe { application.setApplicationIconImage(Some(&image)) };
}

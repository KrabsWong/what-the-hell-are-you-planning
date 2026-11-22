//! Main-thread access to Sparkle. Source builds without the `sparkle` feature
//! keep a no-op updater so tests and development launches need no framework.

#[cfg(all(feature = "sparkle", target_os = "macos"))]
mod native {
    unsafe extern "C" {
        pub fn planning_updater_set_handler(handler: extern "C" fn(*const std::ffi::c_char));
        pub fn planning_updater_version() -> *const std::ffi::c_char;
        pub fn planning_updater_start() -> bool;
        pub fn planning_updater_available() -> bool;
        pub fn planning_updater_can_check() -> bool;
        pub fn planning_updater_check();
        pub fn planning_updater_automatic() -> bool;
        pub fn planning_updater_set_automatic(enabled: bool);
    }
}

#[cfg(all(feature = "sparkle", target_os = "macos"))]
static SENDER: std::sync::Mutex<Option<std::sync::mpsc::Sender<Option<String>>>> =
    std::sync::Mutex::new(None);
#[cfg(all(feature = "sparkle", target_os = "macos"))]
static RECEIVER: std::sync::Mutex<Option<std::sync::mpsc::Receiver<Option<String>>>> =
    std::sync::Mutex::new(None);

pub fn start() {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    {
        if RECEIVER.lock().unwrap().is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            *SENDER.lock().unwrap() = Some(tx);
            *RECEIVER.lock().unwrap() = Some(rx);
        }
        unsafe {
            native::planning_updater_set_handler(changed);
            native::planning_updater_start();
        }
    }
}

/// Deliver Sparkle's main-thread notification to the GPUI event loop.
pub fn try_recv() -> Option<Option<String>> {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    {
        let guard = RECEIVER.lock().unwrap();
        return guard.as_ref().and_then(|rx| rx.try_recv().ok());
    }
    #[cfg(not(all(feature = "sparkle", target_os = "macos")))]
    None
}

pub fn available() -> bool {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    unsafe {
        return native::planning_updater_available();
    }
    #[cfg(not(all(feature = "sparkle", target_os = "macos")))]
    false
}

pub fn can_check() -> bool {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    unsafe {
        return native::planning_updater_can_check();
    }
    #[cfg(not(all(feature = "sparkle", target_os = "macos")))]
    false
}

pub fn check() {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    unsafe {
        native::planning_updater_check();
    }
}

pub fn automatic() -> bool {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    unsafe {
        return native::planning_updater_automatic();
    }
    #[cfg(not(all(feature = "sparkle", target_os = "macos")))]
    false
}

pub fn set_automatic(enabled: bool) {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    unsafe {
        native::planning_updater_set_automatic(enabled);
    }
    #[cfg(not(all(feature = "sparkle", target_os = "macos")))]
    let _ = enabled;
}

pub fn version() -> Option<String> {
    #[cfg(all(feature = "sparkle", target_os = "macos"))]
    unsafe {
        let value = native::planning_updater_version();
        if !value.is_null() {
            return Some(std::ffi::CStr::from_ptr(value).to_string_lossy().into_owned());
        }
    }
    None
}

#[cfg(all(feature = "sparkle", target_os = "macos"))]
extern "C" fn changed(value: *const std::ffi::c_char) {
    let version = if value.is_null() {
        None
    } else {
        Some(unsafe { std::ffi::CStr::from_ptr(value) }.to_string_lossy().into_owned())
    };
    if let Ok(guard) = SENDER.lock()
        && let Some(sender) = guard.as_ref()
    {
        let _ = sender.send(version);
    }
}

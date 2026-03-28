use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use objc2::msg_send;
use objc2::runtime::{AnyClass, Bool};
use objc2_foundation::NSString;
use std::ffi::c_void;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct PermissionStatus {
    pub microphone_granted: bool,
    pub accessibility_granted: bool,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

const AV_AUTHORIZATION_STATUS_NOT_DETERMINED: isize = 0;
const AV_AUTHORIZATION_STATUS_DENIED: isize = 2;
const AV_AUTHORIZATION_STATUS_AUTHORIZED: isize = 3;

pub fn request_startup_permissions() -> PermissionStatus {
    PermissionStatus {
        microphone_granted: request_microphone_access_if_needed(),
        accessibility_granted: request_accessibility_if_needed(),
    }
}

pub fn has_accessibility_permission() -> bool {
    unsafe { AXIsProcessTrusted() }
}

fn request_microphone_access_if_needed() -> bool {
    let Some(capture_device_class) = AnyClass::get(c"AVCaptureDevice") else {
        eprintln!("[screamer] AVCaptureDevice class unavailable");
        return true;
    };

    let media_type = NSString::from_str("soun");
    let status: isize =
        unsafe { msg_send![capture_device_class, authorizationStatusForMediaType: &*media_type] };

    match status {
        AV_AUTHORIZATION_STATUS_AUTHORIZED => true,
        AV_AUTHORIZATION_STATUS_NOT_DETERMINED => {
            let (tx, rx) = mpsc::channel();
            let block = block2::RcBlock::new(move |granted: Bool| {
                let _ = tx.send(granted.as_bool());
            });

            unsafe {
                let _: () = msg_send![
                    capture_device_class,
                    requestAccessForMediaType: &*media_type,
                    completionHandler: &*block
                ];
            }

            rx.recv_timeout(Duration::from_secs(30)).unwrap_or(false)
        }
        AV_AUTHORIZATION_STATUS_DENIED => false,
        _ => false,
    }
}

fn request_accessibility_if_needed() -> bool {
    if has_accessibility_permission() {
        return true;
    }

    let prompt_key = CFString::new("AXTrustedCheckOptionPrompt");
    let options = CFDictionary::from_CFType_pairs(&[(
        prompt_key.as_CFType(),
        CFBoolean::true_value().as_CFType(),
    )]);

    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void) }
}

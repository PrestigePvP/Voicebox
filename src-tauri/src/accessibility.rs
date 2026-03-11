use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FocusContext {
    pub app_name: String,
    pub bundle_id: String,
    pub element_role: String,
    pub title: String,
    pub placeholder: String,
    pub value: String,
    pub pid: i32,
    #[serde(skip)]
    pub icon_base64: Option<String>,
}

#[cfg(target_os = "macos")]
pub fn get_focus_context() -> FocusContext {
    use std::ffi::{c_char, c_void, CStr};
    use std::ptr;

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> *mut c_void;
        fn AXUIElementCopyAttributeValue(
            element: *mut c_void,
            attribute: *const c_void,
            value: *mut *mut c_void,
        ) -> i32;
    }

    extern "C" {
        fn CFRelease(cf: *const c_void);
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut u8,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
        fn CFStringGetLength(string: *const c_void) -> isize;
        fn CFGetTypeID(cf: *const c_void) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    }

    const AX_ERROR_SUCCESS: i32 = 0;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    fn cfstring_to_string(cfstr: *const c_void) -> String {
        if cfstr.is_null() {
            return String::new();
        }
        unsafe {
            if CFGetTypeID(cfstr) != CFStringGetTypeID() {
                CFRelease(cfstr);
                return String::new();
            }
            let len = CFStringGetLength(cfstr);
            let max_size = CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8) + 1;
            let mut buf = vec![0u8; max_size as usize];
            if CFStringGetCString(cfstr, buf.as_mut_ptr(), max_size, K_CF_STRING_ENCODING_UTF8) {
                let cstr = CStr::from_ptr(buf.as_ptr() as *const i8);
                let s = cstr.to_string_lossy().into_owned();
                CFRelease(cfstr);
                s
            } else {
                CFRelease(cfstr);
                String::new()
            }
        }
    }

    fn get_ax_attribute(element: *mut c_void, attr: &str) -> String {
        use core_foundation::base::TCFType;
        use core_foundation::string::CFString;

        let cf_attr = CFString::new(attr);
        let mut value: *mut c_void = ptr::null_mut();
        unsafe {
            if AXUIElementCopyAttributeValue(element, cf_attr.as_CFTypeRef(), &mut value)
                == AX_ERROR_SUCCESS
                && !value.is_null()
            {
                cfstring_to_string(value)
            } else {
                String::new()
            }
        }
    }

    let (app_name, bundle_id, pid, icon_base64) = unsafe {
        #[link(name = "objc", kind = "dylib")]
        extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *mut c_void;
            fn objc_msgSend(obj: *mut c_void, sel: *mut c_void, ...) -> *mut c_void;
        }

        let workspace_class = objc_getClass(c"NSWorkspace".as_ptr());
        let shared_sel = sel_registerName(c"sharedWorkspace".as_ptr());
        let workspace = objc_msgSend(workspace_class, shared_sel);

        if workspace.is_null() {
            return FocusContext::default();
        }

        let front_app_sel = sel_registerName(c"frontmostApplication".as_ptr());
        let front_app = objc_msgSend(workspace, front_app_sel);

        if front_app.is_null() {
            return FocusContext::default();
        }

        let pid_sel = sel_registerName(c"processIdentifier".as_ptr());
        let pid = objc_msgSend(front_app, pid_sel) as i32;

        let name_sel = sel_registerName(c"localizedName".as_ptr());
        let name_nsstr = objc_msgSend(front_app, name_sel);
        let utf8_sel = sel_registerName(c"UTF8String".as_ptr());
        let name_ptr = objc_msgSend(name_nsstr, utf8_sel) as *const i8;
        let name = if !name_ptr.is_null() {
            CStr::from_ptr(name_ptr).to_string_lossy().into_owned()
        } else {
            String::new()
        };

        let bundle_sel = sel_registerName(c"bundleIdentifier".as_ptr());
        let bundle_nsstr = objc_msgSend(front_app, bundle_sel);
        let bundle_ptr = objc_msgSend(bundle_nsstr, utf8_sel) as *const i8;
        let bundle = if !bundle_ptr.is_null() {
            CStr::from_ptr(bundle_ptr).to_string_lossy().into_owned()
        } else {
            String::new()
        };

        // Capture app icon while we have front_app
        let icon_b64 = extract_app_icon(front_app);

        (name, bundle, pid, icon_b64)
    };

    let mut ctx = FocusContext {
        app_name,
        bundle_id,
        pid,
        icon_base64,
        ..Default::default()
    };

    unsafe {
        let sys_wide = AXUIElementCreateSystemWide();
        if sys_wide.is_null() {
            return ctx;
        }

        let mut focused_el: *mut c_void = ptr::null_mut();
        use core_foundation::base::TCFType;
        let focused_attr = core_foundation::string::CFString::new("AXFocusedUIElement");
        if AXUIElementCopyAttributeValue(sys_wide, focused_attr.as_CFTypeRef(), &mut focused_el)
            != AX_ERROR_SUCCESS
            || focused_el.is_null()
        {
            CFRelease(sys_wide);
            return ctx;
        }

        ctx.element_role = get_ax_attribute(focused_el, "AXRole");
        ctx.title = get_ax_attribute(focused_el, "AXTitle");
        if ctx.title.is_empty() {
            ctx.title = get_ax_attribute(focused_el, "AXDescription");
        }
        ctx.placeholder = get_ax_attribute(focused_el, "AXPlaceholderValue");
        ctx.value = get_ax_attribute(focused_el, "AXValue");

        CFRelease(focused_el);
        CFRelease(sys_wide);
    }

    ctx
}

/// Extract the app icon by reading the .icns file directly from the app bundle.
/// This avoids AppKit (NSBitmapImageRep) which must run on the main thread.
#[cfg(target_os = "macos")]
unsafe fn extract_app_icon(front_app: *mut std::ffi::c_void) -> Option<String> {
    use std::ffi::{c_char, c_void};

    extern "C" {
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend(obj: *mut c_void, sel: *mut c_void, ...) -> *mut c_void;
    }

    // Get bundleURL.path from the running application
    let bundle_url = objc_msgSend(front_app, sel_registerName(c"bundleURL".as_ptr()));
    if bundle_url.is_null() {
        return None;
    }
    let path_nsstr = objc_msgSend(bundle_url, sel_registerName(c"path".as_ptr()));
    if path_nsstr.is_null() {
        return None;
    }
    let utf8 = objc_msgSend(path_nsstr, sel_registerName(c"UTF8String".as_ptr())) as *const i8;
    if utf8.is_null() {
        return None;
    }
    let bundle_path = std::ffi::CStr::from_ptr(utf8).to_string_lossy();

    // Read Info.plist to find the icon file name
    let plist_path = format!("{}/Contents/Info.plist", bundle_path);
    let plist_data = std::fs::read(&plist_path).ok()?;
    let plist_str = String::from_utf8_lossy(&plist_data);

    // Parse icon filename from the plist (look for CFBundleIconFile)
    let icon_name = extract_plist_value(&plist_str, "CFBundleIconFile")?;
    let icon_name = if icon_name.ends_with(".icns") {
        icon_name.to_string()
    } else {
        format!("{}.icns", icon_name)
    };

    let icon_path = format!("{}/Contents/Resources/{}", bundle_path, icon_name);
    let icon_data = std::fs::read(&icon_path).ok()?;

    // Extract a PNG from the ICNS container
    let png = extract_png_from_icns(&icon_data)?;

    log::info!("extract_app_icon: extracted {}b PNG from {}", png.len(), icon_name);
    use base64::Engine;
    Some(base64::engine::general_purpose::STANDARD.encode(&png))
}

fn extract_plist_value<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let key_tag = format!("<key>{}</key>", key);
    let pos = plist.find(&key_tag)?;
    let after_key = &plist[pos + key_tag.len()..];
    let start = after_key.find("<string>")? + 8;
    let end = after_key[start..].find("</string>")?;
    Some(&after_key[start..start + end])
}

const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47];

fn extract_png_from_icns(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 8 || &data[..4] != b"icns" {
        return None;
    }

    // Prefer smaller icons first (32x32@2x, 128x128) to keep data small
    let preferred = [b"ic12", b"ic07", b"ic13", b"ic08", b"ic14", b"ic09", b"ic10"];
    let mut fallback: Option<Vec<u8>> = None;

    let total = u32::from_be_bytes(data[4..8].try_into().ok()?) as usize;
    let mut pos = 8;
    while pos + 8 <= total && pos + 8 <= data.len() {
        let entry_type = &data[pos..pos + 4];
        let entry_len = u32::from_be_bytes(data[pos + 4..pos + 8].try_into().ok()?) as usize;
        if entry_len < 8 || pos + entry_len > data.len() {
            break;
        }

        let entry_data = &data[pos + 8..pos + entry_len];

        // Check if this entry contains PNG data
        if entry_data.len() >= 4 && &entry_data[..4] == PNG_MAGIC {
            for (priority, pref) in preferred.iter().enumerate() {
                if entry_type == *pref {
                    if priority == 0 {
                        return Some(entry_data.to_vec());
                    }
                    if fallback.is_none() {
                        fallback = Some(entry_data.to_vec());
                    }
                    break;
                }
            }
            if fallback.is_none() {
                fallback = Some(entry_data.to_vec());
            }
        }

        pos += entry_len;
    }

    fallback
}

#[cfg(target_os = "macos")]
pub fn paste_into_app(pid: i32) {
    if pid <= 0 {
        return;
    }

    std::thread::spawn(move || unsafe {
        use std::ffi::{c_char, c_void};

        #[link(name = "objc", kind = "dylib")]
        extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *mut c_void;
            fn objc_msgSend(obj: *mut c_void, sel: *mut c_void, ...) -> *mut c_void;
        }

        let app_class = objc_getClass(c"NSRunningApplication".as_ptr());
        let pid_sel =
            sel_registerName(c"runningApplicationWithProcessIdentifier:".as_ptr());
        let app = objc_msgSend(app_class, pid_sel, pid);

        if !app.is_null() {
            let activate_sel = sel_registerName(c"activateWithOptions:".as_ptr());
            let _: *mut c_void = objc_msgSend(app, activate_sel, 2u64);
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
        let key_down =
            CGEvent::new_keyboard_event(source.clone(), 9 as CGKeyCode, true).unwrap();
        let key_up = CGEvent::new_keyboard_event(source, 9 as CGKeyCode, false).unwrap();

        key_down.set_flags(CGEventFlags::CGEventFlagCommand);
        key_up.set_flags(CGEventFlags::CGEventFlagCommand);

        key_down.post(CGEventTapLocation::AnnotatedSession);
        key_up.post(CGEventTapLocation::AnnotatedSession);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn get_focus_context() -> FocusContext {
    FocusContext::default()
}

#[cfg(not(target_os = "macos"))]
pub fn paste_into_app(_pid: i32) {}

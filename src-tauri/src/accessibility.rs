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

    let (app_name, bundle_id, pid) = unsafe {
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

        (name, bundle, pid)
    };

    let mut ctx = FocusContext {
        app_name,
        bundle_id,
        pid,
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

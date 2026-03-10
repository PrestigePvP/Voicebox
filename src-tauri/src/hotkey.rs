use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifier(u64);

impl Modifier {
    pub const CTRL: u64 = 1 << 0;
    pub const SHIFT: u64 = 1 << 1;
    pub const OPTION: u64 = 1 << 2;
    pub const CMD: u64 = 1 << 3;
    pub const FN: u64 = 1 << 4;

    pub fn bits(&self) -> u64 {
        self.0
    }
}

pub fn parse_hotkey(combo: &str) -> Result<Modifier, String> {
    let lower = combo.to_lowercase();
    let parts: Vec<&str> = lower.split('+').collect();
    if parts.is_empty() || (parts.len() == 1 && parts[0].trim().is_empty()) {
        return Err(format!(
            "Hotkey combo must have at least one modifier: {:?}",
            combo
        ));
    }

    let mut mods: u64 = 0;
    for part in &parts {
        let part = part.trim();
        let m = match part {
            "ctrl" | "control" => Modifier::CTRL,
            "shift" => Modifier::SHIFT,
            "alt" | "option" => Modifier::OPTION,
            "cmd" | "command" | "super" => Modifier::CMD,
            "fn" => Modifier::FN,
            _ => {
                return Err(format!(
                    "Unknown modifier: {:?} (supported: ctrl, shift, alt/option, cmd/command, fn)",
                    part
                ))
            }
        };
        mods |= m;
    }

    if mods == 0 {
        return Err(format!(
            "Hotkey combo must have at least one modifier: {:?}",
            combo
        ));
    }

    Ok(Modifier(mods))
}

pub struct HotkeyHandle {
    #[cfg(target_os = "macos")]
    _stop: Option<Box<dyn FnOnce() + Send>>,
}

impl Drop for HotkeyHandle {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(stop) = self._stop.take() {
            stop();
        }
    }
}

#[cfg(target_os = "macos")]
pub fn register(
    combo: &str,
    on_down: Arc<dyn Fn() + Send + Sync>,
    on_up: Arc<dyn Fn() + Send + Sync>,
) -> Result<HotkeyHandle, String> {
    use core_foundation::base::TCFType;
    use core_foundation::runloop::{
        kCFRunLoopCommonModes, CFRunLoop, CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRun,
    };
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    let mods = parse_hotkey(combo)?;

    // Check accessibility permission
    if !check_accessibility() {
        return Err(format!(
            "Failed to create event tap for {:?} — grant Accessibility permission in System Settings > Privacy & Security > Accessibility",
            combo
        ));
    }

    let target_flags = modifier_to_cg_flags(mods);
    let active = Arc::new(AtomicBool::new(false));
    let active_cb = active.clone();
    let on_down_cb = on_down;
    let on_up_cb = on_up;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    let run_loop: Arc<std::sync::Mutex<Option<CFRunLoop>>> =
        Arc::new(std::sync::Mutex::new(None));
    let run_loop_thread = run_loop.clone();

    let thread = std::thread::spawn(move || {
        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::FlagsChanged],
            move |_, _, event| {
                let flags = event.get_flags().bits() & MODIFIER_MASK;
                if (flags & target_flags) == target_flags {
                    if !active_cb.swap(true, Ordering::SeqCst) {
                        let on_down = on_down_cb.clone();
                        std::thread::spawn(move || on_down());
                    }
                } else if active_cb.swap(false, Ordering::SeqCst) {
                    let on_up = on_up_cb.clone();
                    std::thread::spawn(move || on_up());
                }
                None
            },
        );

        let tap = match tap {
            Ok(t) => t,
            Err(_) => {
                log::error!("Failed to create CGEventTap");
                return;
            }
        };

        let source = tap
            .mach_port
            .create_runloop_source(0)
            .expect("Failed to create run loop source");

        unsafe {
            let current = CFRunLoopGetCurrent();
            CFRunLoopAddSource(current, source.as_concrete_TypeRef(), kCFRunLoopCommonModes);
            tap.enable();

            *run_loop_thread.lock().unwrap() = Some(CFRunLoop::get_current());

            if !stop_flag_thread.load(Ordering::Relaxed) {
                CFRunLoopRun();
            }
        }
    });

    // Wait briefly for thread to start and populate run_loop
    std::thread::sleep(std::time::Duration::from_millis(100));

    let handle = HotkeyHandle {
        _stop: Some(Box::new(move || {
            stop_flag.store(true, Ordering::Relaxed);
            if let Some(rl) = run_loop.lock().unwrap().as_ref() {
                rl.stop();
            }
            let _ = thread.join();
        })),
    };

    Ok(handle)
}

#[cfg(target_os = "macos")]
const MODIFIER_MASK: u64 = 0x0000_0000_00FF_0000; // NX_DEVICE_DEPENDENT + modifier bits

#[cfg(target_os = "macos")]
fn modifier_to_cg_flags(m: Modifier) -> u64 {
    use core_graphics::event::CGEventFlags;
    let mut flags: u64 = 0;
    if m.bits() & Modifier::CTRL != 0 {
        flags |= CGEventFlags::CGEventFlagControl.bits();
    }
    if m.bits() & Modifier::SHIFT != 0 {
        flags |= CGEventFlags::CGEventFlagShift.bits();
    }
    if m.bits() & Modifier::OPTION != 0 {
        flags |= CGEventFlags::CGEventFlagAlternate.bits();
    }
    if m.bits() & Modifier::CMD != 0 {
        flags |= CGEventFlags::CGEventFlagCommand.bits();
    }
    if m.bits() & Modifier::FN != 0 {
        flags |= CGEventFlags::CGEventFlagSecondaryFn.bits();
    }
    flags & MODIFIER_MASK
}

#[cfg(target_os = "macos")]
fn check_accessibility() -> bool {
    use std::ffi::c_void;

    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }

    unsafe {
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::true_value();

        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const c_void)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn register(
    _combo: &str,
    _on_down: Arc<dyn Fn() + Send + Sync>,
    _on_up: Arc<dyn Fn() + Send + Sync>,
) -> Result<HotkeyHandle, String> {
    Err("Modifier-only hotkeys are only supported on macOS".into())
}

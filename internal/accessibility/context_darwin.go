//go:build darwin

package accessibility

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Cocoa -framework ApplicationServices -framework CoreGraphics

#import <Cocoa/Cocoa.h>
#import <ApplicationServices/ApplicationServices.h>

typedef struct {
	char *appName;
	char *bundleID;
	char *elementRole;
	char *title;
	char *placeholder;
	char *value;
	int32_t pid;
} FocusInfo;

static char *cfStringToC(CFTypeRef ref) {
	if (!ref) return NULL;
	if (CFGetTypeID(ref) != CFStringGetTypeID()) {
		CFRelease(ref);
		return NULL;
	}
	CFStringRef str = (CFStringRef)ref;
	CFIndex len = CFStringGetLength(str);
	CFIndex maxSize = CFStringGetMaximumSizeForEncoding(len, kCFStringEncodingUTF8) + 1;
	char *buf = (char *)malloc(maxSize);
	if (!CFStringGetCString(str, buf, maxSize, kCFStringEncodingUTF8)) {
		free(buf);
		CFRelease(ref);
		return NULL;
	}
	CFRelease(ref);
	return buf;
}

static FocusInfo getFocusedElementInfo() {
	FocusInfo info = {0};

	NSRunningApplication *frontApp = [[NSWorkspace sharedWorkspace] frontmostApplication];
	if (!frontApp) return info;

	info.pid = frontApp.processIdentifier;
	info.appName = strdup([frontApp.localizedName UTF8String] ?: "");
	info.bundleID = strdup([frontApp.bundleIdentifier UTF8String] ?: "");

	AXUIElementRef sysWide = AXUIElementCreateSystemWide();
	AXUIElementRef focusedEl = NULL;
	AXError err = AXUIElementCopyAttributeValue(sysWide, kAXFocusedUIElementAttribute, (CFTypeRef *)&focusedEl);
	CFRelease(sysWide);

	if (err != kAXErrorSuccess || !focusedEl) return info;

	CFTypeRef val = NULL;

	if (AXUIElementCopyAttributeValue(focusedEl, kAXRoleAttribute, &val) == kAXErrorSuccess)
		info.elementRole = cfStringToC(val);

	if (AXUIElementCopyAttributeValue(focusedEl, kAXTitleAttribute, &val) == kAXErrorSuccess)
		info.title = cfStringToC(val);

	if (!info.title || strlen(info.title) == 0) {
		if (AXUIElementCopyAttributeValue(focusedEl, kAXDescriptionAttribute, &val) == kAXErrorSuccess) {
			char *desc = cfStringToC(val);
			if (desc) {
				free(info.title);
				info.title = desc;
			}
		}
	}

	if (AXUIElementCopyAttributeValue(focusedEl, kAXPlaceholderValueAttribute, &val) == kAXErrorSuccess)
		info.placeholder = cfStringToC(val);

	if (AXUIElementCopyAttributeValue(focusedEl, kAXValueAttribute, &val) == kAXErrorSuccess)
		info.value = cfStringToC(val);

	CFRelease(focusedEl);
	return info;
}

static void freeFocusInfo(FocusInfo *info) {
	free(info->appName);
	free(info->bundleID);
	free(info->elementRole);
	free(info->title);
	free(info->placeholder);
	free(info->value);
}

static void reactivateAndPaste(int32_t pid) {
	dispatch_async(dispatch_get_main_queue(), ^{
		NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
		if (!app) return;

		// NSApplicationActivateIgnoringOtherApps (1 << 1) is deprecated in macOS 14+
		// but still needed for older versions. On 14+ activateWithOptions:0 suffices.
		[app activateWithOptions:(1 << 1)];

		// Delay to let the app come to front before simulating keystrokes
		dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
			CGEventSourceRef src = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
			CGEventRef keyDown = CGEventCreateKeyboardEvent(src, 9, true);  // 9 = 'v'
			CGEventRef keyUp = CGEventCreateKeyboardEvent(src, 9, false);
			CGEventSetFlags(keyDown, kCGEventFlagMaskCommand);
			CGEventSetFlags(keyUp, kCGEventFlagMaskCommand);
			CGEventPost(kCGAnnotatedSessionEventTap, keyDown);
			CGEventPost(kCGAnnotatedSessionEventTap, keyUp);
			CFRelease(keyDown);
			CFRelease(keyUp);
			CFRelease(src);
		});
	});
}
*/
import "C"

import (
	"log"
	"sync"
)

var (
	axWarningOnce sync.Once
)

func cToGoString(cs *C.char) string {
	if cs == nil {
		return ""
	}
	return C.GoString(cs)
}

func GetFocusContext() FocusContext {
	info := C.getFocusedElementInfo()
	defer C.freeFocusInfo(&info)

	ctx := FocusContext{
		AppName:  cToGoString(info.appName),
		BundleID: cToGoString(info.bundleID),
		PID:      int32(info.pid),
	}

	// AX fields will be empty if permission is denied — that's fine
	role := cToGoString(info.elementRole)
	if role == "" {
		axWarningOnce.Do(func() {
			log.Printf("Accessibility permission not granted — context capture limited to app name/PID")
		})
	}

	ctx.ElementRole = role
	ctx.Title = cToGoString(info.title)
	ctx.Placeholder = cToGoString(info.placeholder)
	ctx.Value = cToGoString(info.value)

	return ctx
}

func PasteIntoApp(pid int32) {
	if pid <= 0 {
		return
	}
	C.reactivateAndPaste(C.int32_t(pid))
}

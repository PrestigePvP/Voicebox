//go:build darwin

package main

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Cocoa

#import <Cocoa/Cocoa.h>
#import <objc/runtime.h>

// Forward declaration of the Go callback
extern void goDockClickCallback(void);

static NSWindow* findMainWindow() {
	NSApplication *app = [NSApplication sharedApplication];
	for (NSWindow *window in [app windows]) {
		if (![window isKindOfClass:[NSPanel class]]) {
			return window;
		}
	}
	return nil;
}

static void showOverlayTopCenter() {
	dispatch_async(dispatch_get_main_queue(), ^{
		NSWindow *target = findMainWindow();
		if (!target) return;

		// Transparent background
		[target setOpaque:NO];
		[target setBackgroundColor:[NSColor clearColor]];
		[target setHasShadow:NO];

		NSScreen *screen = [NSScreen mainScreen];
		NSRect visible = screen.visibleFrame;
		CGFloat winW = target.frame.size.width;
		CGFloat winH = target.frame.size.height;

		// Top-center of visible area, 20px from top
		CGFloat x = visible.origin.x + (visible.size.width - winW) / 2;
		CGFloat y = visible.origin.y + visible.size.height - winH - 20;

		[target setFrameOrigin:NSMakePoint(x, y)];
		[target setLevel:NSFloatingWindowLevel];
		[target orderFrontRegardless];
	});
}

static void hideOverlay() {
	dispatch_async(dispatch_get_main_queue(), ^{
		NSWindow *window = findMainWindow();
		if (window) {
			[window orderOut:nil];
		}
	});
}

static void showSettingsWindow() {
	dispatch_async(dispatch_get_main_queue(), ^{
		NSWindow *target = findMainWindow();
		if (!target) return;

		[target setOpaque:YES];
		[target setBackgroundColor:[NSColor windowBackgroundColor]];
		[target setHasShadow:YES];
		[target setLevel:NSNormalWindowLevel];

		CGFloat w = 700;
		CGFloat h = 450;
		NSRect frame = [target frame];
		frame.size = NSMakeSize(w, h);
		[target setFrame:frame display:YES animate:NO];

		NSScreen *screen = [NSScreen mainScreen];
		NSRect screenFrame = screen.visibleFrame;
		CGFloat x = screenFrame.origin.x + (screenFrame.size.width - w) / 2;
		CGFloat y = screenFrame.origin.y + (screenFrame.size.height - h) / 2;
		[target setFrameOrigin:NSMakePoint(x, y)];

		[target makeKeyAndOrderFront:nil];
		[target orderFrontRegardless];

		// Bring app to front
		[NSApp activateIgnoringOtherApps:YES];
	});
}

// Swizzled applicationShouldHandleReopen handler
static BOOL swizzled_handleReopen(id self, SEL _cmd, NSApplication *app, BOOL hasVisibleWindows) {
	if (!hasVisibleWindows) {
		goDockClickCallback();
		return NO;
	}
	return YES;
}

static void setupDockClickHandler() {
	dispatch_async(dispatch_get_main_queue(), ^{
		id delegate = [NSApp delegate];
		if (!delegate) return;

		Class cls = [delegate class];
		SEL sel = @selector(applicationShouldHandleReopen:hasVisibleWindows:);

		class_replaceMethod(cls, sel, (IMP)swizzled_handleReopen, "B@:@B");
	});
}
*/
import "C"

import "context"

//export goDockClickCallback
func goDockClickCallback() {
	if dockClickHandler != nil {
		dockClickHandler()
	}
}

var dockClickHandler func()

func platformShowWindow(_ context.Context) {
	C.showOverlayTopCenter()
}

func platformHideWindow(_ context.Context) {
	C.hideOverlay()
}

func platformShowSettings(_ context.Context) {
	C.showSettingsWindow()
}

func platformSetupDockHandler(callback func()) {
	dockClickHandler = callback
	C.setupDockClickHandler()
}

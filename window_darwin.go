//go:build darwin

package main

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Cocoa

#import <Cocoa/Cocoa.h>

void showOverlayTopCenter() {
	dispatch_async(dispatch_get_main_queue(), ^{
		NSApplication *app = [NSApplication sharedApplication];
		NSWindow *target = nil;
		for (NSWindow *window in [app windows]) {
			if (![window isKindOfClass:[NSPanel class]]) {
				target = window;
				break;
			}
		}
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

void hideOverlay() {
	dispatch_async(dispatch_get_main_queue(), ^{
		NSApplication *app = [NSApplication sharedApplication];
		for (NSWindow *window in [app windows]) {
			if (![window isKindOfClass:[NSPanel class]]) {
				[window orderOut:nil];
				return;
			}
		}
	});
}
*/
import "C"

import "context"

func platformShowWindow(_ context.Context) {
	C.showOverlayTopCenter()
}

func platformHideWindow(_ context.Context) {
	C.hideOverlay()
}

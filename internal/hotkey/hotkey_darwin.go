//go:build darwin

package hotkey

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework CoreGraphics -framework CoreFoundation -framework ApplicationServices -framework Cocoa

#include <CoreGraphics/CoreGraphics.h>
#include <CoreFoundation/CoreFoundation.h>
#include <ApplicationServices/ApplicationServices.h>
#import <Cocoa/Cocoa.h>
#include <pthread.h>

// Forward declarations for Go callbacks
extern void hotkeyDown(int callbackID);
extern void hotkeyUp(int callbackID);

typedef struct {
	CGEventFlags targetFlags;
	int active;
	int callbackID;
	CFMachPortRef tap;
	CFRunLoopSourceRef source;
	CFRunLoopRef runLoop;
	pthread_t thread;
} HotkeyState;

// Mask for the 5 modifier bits we care about
static const CGEventFlags kModifierMask =
	kCGEventFlagMaskControl |
	kCGEventFlagMaskShift |
	kCGEventFlagMaskAlternate |
	kCGEventFlagMaskCommand |
	kCGEventFlagMaskSecondaryFn;

static CGEventRef eventCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
	HotkeyState *state = (HotkeyState *)userInfo;

	if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
		CGEventTapEnable(state->tap, true);
		return event;
	}

	CGEventFlags flags = CGEventGetFlags(event) & kModifierMask;

	if ((flags & state->targetFlags) == state->targetFlags) {
		if (!state->active) {
			state->active = 1;
			hotkeyDown(state->callbackID);
		}
	} else {
		if (state->active) {
			state->active = 0;
			hotkeyUp(state->callbackID);
		}
	}

	return event;
}

static void *runEventTap(void *arg) {
	HotkeyState *state = (HotkeyState *)arg;
	state->runLoop = CFRunLoopGetCurrent();
	CFRunLoopAddSource(state->runLoop, state->source, kCFRunLoopCommonModes);
	CGEventTapEnable(state->tap, true);
	CFRunLoopRun();
	return NULL;
}

static int checkAccessibility() {
	// Prompt the OS to show the accessibility permission dialog
	NSDictionary *opts = @{(__bridge NSString *)kAXTrustedCheckOptionPrompt: @YES};
	return AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)opts);
}

static HotkeyState *createAndStartEventTap(CGEventFlags targetFlags, int callbackID) {
	if (!checkAccessibility()) {
		return NULL;
	}

	HotkeyState *state = (HotkeyState *)malloc(sizeof(HotkeyState));
	state->targetFlags = targetFlags;
	state->active = 0;
	state->callbackID = callbackID;
	state->runLoop = NULL;

	CGEventMask mask = (1 << kCGEventFlagsChanged);
	state->tap = CGEventTapCreate(
		kCGSessionEventTap,
		kCGHeadInsertEventTap,
		kCGEventTapOptionListenOnly,
		mask,
		eventCallback,
		state
	);

	if (!state->tap) {
		free(state);
		return NULL;
	}

	state->source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, state->tap, 0);
	if (!state->source) {
		CFRelease(state->tap);
		free(state);
		return NULL;
	}

	pthread_create(&state->thread, NULL, runEventTap, state);
	return state;
}

static void stopEventTap(HotkeyState *state) {
	if (!state) return;
	CGEventTapEnable(state->tap, false);
	if (state->runLoop) {
		CFRunLoopStop(state->runLoop);
	}
	pthread_join(state->thread, NULL);
	CFRelease(state->source);
	CFRelease(state->tap);
	free(state);
}
*/
import "C"

import (
	"fmt"
	"sync"
)

type callbacks struct {
	onDown func()
	onUp   func()
}

var (
	cbMu      sync.Mutex
	cbMap     = make(map[int]callbacks)
	cbNextID  int
)

//export hotkeyDown
func hotkeyDown(callbackID C.int) {
	cbMu.Lock()
	cb, ok := cbMap[int(callbackID)]
	cbMu.Unlock()
	if ok && cb.onDown != nil {
		go cb.onDown()
	}
}

//export hotkeyUp
func hotkeyUp(callbackID C.int) {
	cbMu.Lock()
	cb, ok := cbMap[int(callbackID)]
	cbMu.Unlock()
	if ok && cb.onUp != nil {
		go cb.onUp()
	}
}

func modifierToCGFlags(m Modifier) C.CGEventFlags {
	var flags C.CGEventFlags
	if m&ModCtrl != 0 {
		flags |= C.kCGEventFlagMaskControl
	}
	if m&ModShift != 0 {
		flags |= C.kCGEventFlagMaskShift
	}
	if m&ModOption != 0 {
		flags |= C.kCGEventFlagMaskAlternate
	}
	if m&ModCmd != 0 {
		flags |= C.kCGEventFlagMaskCommand
	}
	if m&ModFn != 0 {
		flags |= C.kCGEventFlagMaskSecondaryFn
	}
	return flags
}

func RegisterHotkey(combo string, onDown func(), onUp func()) (func(), error) {
	mods, err := ParseHotkey(combo)
	if err != nil {
		return nil, err
	}

	targetFlags := modifierToCGFlags(mods)

	cbMu.Lock()
	id := cbNextID
	cbNextID++
	cbMap[id] = callbacks{onDown: onDown, onUp: onUp}
	cbMu.Unlock()

	state := C.createAndStartEventTap(targetFlags, C.int(id))
	if state == nil {
		cbMu.Lock()
		delete(cbMap, id)
		cbMu.Unlock()
		return nil, fmt.Errorf("failed to create event tap for %q — grant Accessibility permission in System Settings > Privacy & Security > Accessibility", combo)
	}

	cleanup := func() {
		C.stopEventTap(state)
		cbMu.Lock()
		delete(cbMap, id)
		cbMu.Unlock()
	}

	return cleanup, nil
}

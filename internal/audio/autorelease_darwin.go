//go:build darwin

package audio

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Foundation

#import <Foundation/Foundation.h>

// Drains the current thread's implicit autorelease pool by pushing and
// popping a new one.  Call this once after stopping a CoreAudio device
// so the IO thread's pool is flushed before the thread exits.
static void drainAutoreleasePool() {
	@autoreleasepool {}
}
*/
import "C"

func drainAutorelease() {
	C.drainAutoreleasePool()
}

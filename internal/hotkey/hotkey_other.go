//go:build !darwin

package hotkey

import "fmt"

func RegisterHotkey(combo string, onDown func(), onUp func()) (func(), error) {
	return nil, fmt.Errorf("hotkey registration is not supported on this platform")
}

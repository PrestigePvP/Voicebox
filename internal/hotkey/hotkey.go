package hotkey

import (
	"fmt"
	"strings"

	"golang.design/x/hotkey"
)

var modifierMap = map[string]hotkey.Modifier{
	"ctrl":   hotkey.ModCtrl,
	"shift":  hotkey.ModShift,
	"alt":    hotkey.ModOption,
	"option": hotkey.ModOption,
	"cmd":    hotkey.ModCmd,
	"super":  hotkey.ModCmd,
}

var keyMap = map[string]hotkey.Key{
	"a": hotkey.KeyA, "b": hotkey.KeyB, "c": hotkey.KeyC, "d": hotkey.KeyD,
	"e": hotkey.KeyE, "f": hotkey.KeyF, "g": hotkey.KeyG, "h": hotkey.KeyH,
	"i": hotkey.KeyI, "j": hotkey.KeyJ, "k": hotkey.KeyK, "l": hotkey.KeyL,
	"m": hotkey.KeyM, "n": hotkey.KeyN, "o": hotkey.KeyO, "p": hotkey.KeyP,
	"q": hotkey.KeyQ, "r": hotkey.KeyR, "s": hotkey.KeyS, "t": hotkey.KeyT,
	"u": hotkey.KeyU, "v": hotkey.KeyV, "w": hotkey.KeyW, "x": hotkey.KeyX,
	"y": hotkey.KeyY, "z": hotkey.KeyZ,
	"0": hotkey.Key0, "1": hotkey.Key1, "2": hotkey.Key2, "3": hotkey.Key3,
	"4": hotkey.Key4, "5": hotkey.Key5, "6": hotkey.Key6, "7": hotkey.Key7,
	"8": hotkey.Key8, "9": hotkey.Key9,
	"space": hotkey.KeySpace,
}

func ParseHotkey(combo string) ([]hotkey.Modifier, hotkey.Key, error) {
	parts := strings.Split(strings.ToLower(strings.TrimSpace(combo)), "+")
	if len(parts) < 2 {
		return nil, 0, fmt.Errorf("hotkey combo must have at least a modifier and a key: %q", combo)
	}

	var mods []hotkey.Modifier
	for _, part := range parts[:len(parts)-1] {
		mod, ok := modifierMap[strings.TrimSpace(part)]
		if !ok {
			return nil, 0, fmt.Errorf("unknown modifier: %q", part)
		}
		mods = append(mods, mod)
	}

	keyStr := strings.TrimSpace(parts[len(parts)-1])
	key, ok := keyMap[keyStr]
	if !ok {
		return nil, 0, fmt.Errorf("unknown key: %q", keyStr)
	}

	return mods, key, nil
}

func RegisterHotkey(combo string, onDown func(), onUp func()) (func(), error) {
	mods, key, err := ParseHotkey(combo)
	if err != nil {
		return nil, err
	}

	hk := hotkey.New(mods, key)
	if err := hk.Register(); err != nil {
		return nil, fmt.Errorf("register hotkey %q: %w", combo, err)
	}

	go func() {
		for range hk.Keydown() {
			onDown()
		}
	}()

	go func() {
		for range hk.Keyup() {
			onUp()
		}
	}()

	return func() { hk.Unregister() }, nil
}

package hotkey

import (
	"fmt"
	"strings"
)

type Modifier uint32

const (
	ModCtrl   Modifier = 1 << 0
	ModShift  Modifier = 1 << 1
	ModOption Modifier = 1 << 2
	ModCmd    Modifier = 1 << 3
	ModFn    Modifier = 1 << 4
)

var modifierMap = map[string]Modifier{
	"ctrl":    ModCtrl,
	"control": ModCtrl,
	"shift":   ModShift,
	"alt":     ModOption,
	"option":  ModOption,
	"cmd":     ModCmd,
	"command": ModCmd,
	"super":   ModCmd,
	"fn":      ModFn,
}

func ParseHotkey(combo string) (Modifier, error) {
	parts := strings.Split(strings.ToLower(strings.TrimSpace(combo)), "+")
	if len(parts) == 0 || (len(parts) == 1 && parts[0] == "") {
		return 0, fmt.Errorf("hotkey combo must have at least one modifier: %q", combo)
	}

	var mods Modifier
	for _, part := range parts {
		part = strings.TrimSpace(part)
		mod, ok := modifierMap[part]
		if !ok {
			return 0, fmt.Errorf("unknown modifier: %q (supported: ctrl, shift, alt/option, cmd/command, fn)", part)
		}
		mods |= mod
	}

	if mods == 0 {
		return 0, fmt.Errorf("hotkey combo must have at least one modifier: %q", combo)
	}

	return mods, nil
}

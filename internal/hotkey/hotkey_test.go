package hotkey

import "testing"

func TestParseHotkey(t *testing.T) {
	tests := []struct {
		combo    string
		wantMods Modifier
	}{
		{"ctrl+cmd", ModCtrl | ModCmd},
		{"Ctrl+Cmd", ModCtrl | ModCmd},
		{"fn", ModFn},
		{"ctrl+shift+cmd", ModCtrl | ModShift | ModCmd},
		{"alt+cmd", ModOption | ModCmd},
		{"option+cmd", ModOption | ModCmd},
		{"command+shift", ModCmd | ModShift},
		{"super+ctrl", ModCmd | ModCtrl},
		{"ctrl+shift+alt+cmd", ModCtrl | ModShift | ModOption | ModCmd},
		{"control+command", ModCtrl | ModCmd},
	}

	for _, tt := range tests {
		t.Run(tt.combo, func(t *testing.T) {
			mods, err := ParseHotkey(tt.combo)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if mods != tt.wantMods {
				t.Errorf("got 0x%x, want 0x%x", mods, tt.wantMods)
			}
		})
	}
}

func TestParseHotkeyErrors(t *testing.T) {
	tests := []struct {
		combo string
		desc  string
	}{
		{"", "empty string"},
		{"r", "non-modifier key"},
		{"ctrl+r", "modifier plus non-modifier"},
		{"fake+cmd", "unknown modifier"},
		{"ctrl+unknown", "unknown token"},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			_, err := ParseHotkey(tt.combo)
			if err == nil {
				t.Errorf("expected error for %q", tt.combo)
			}
		})
	}
}

func TestParseHotkeyDuplicateModifiers(t *testing.T) {
	mods, err := ParseHotkey("ctrl+ctrl")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if mods != ModCtrl {
		t.Errorf("duplicate modifier should collapse: got 0x%x, want 0x%x", mods, ModCtrl)
	}
}

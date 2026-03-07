package hotkey

import (
	"testing"

	"golang.design/x/hotkey"
)

func TestParseHotkey(t *testing.T) {
	tests := []struct {
		combo    string
		wantMods []hotkey.Modifier
		wantKey  hotkey.Key
	}{
		{"ctrl+shift+r", []hotkey.Modifier{hotkey.ModCtrl, hotkey.ModShift}, hotkey.KeyR},
		{"Ctrl+Shift+R", []hotkey.Modifier{hotkey.ModCtrl, hotkey.ModShift}, hotkey.KeyR},
		{"alt+a", []hotkey.Modifier{hotkey.ModOption}, hotkey.KeyA},
		{"option+z", []hotkey.Modifier{hotkey.ModOption}, hotkey.KeyZ},
		{"cmd+space", []hotkey.Modifier{hotkey.ModCmd}, hotkey.KeySpace},
		{"super+0", []hotkey.Modifier{hotkey.ModCmd}, hotkey.Key0},
		{"ctrl+shift+alt+9", []hotkey.Modifier{hotkey.ModCtrl, hotkey.ModShift, hotkey.ModOption}, hotkey.Key9},
	}

	for _, tt := range tests {
		t.Run(tt.combo, func(t *testing.T) {
			mods, key, err := ParseHotkey(tt.combo)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if len(mods) != len(tt.wantMods) {
				t.Fatalf("got %d modifiers, want %d", len(mods), len(tt.wantMods))
			}
			for i, m := range mods {
				if m != tt.wantMods[i] {
					t.Errorf("modifier[%d] = %v, want %v", i, m, tt.wantMods[i])
				}
			}
			if key != tt.wantKey {
				t.Errorf("key = %v, want %v", key, tt.wantKey)
			}
		})
	}
}

func TestParseHotkeyErrors(t *testing.T) {
	tests := []struct {
		combo string
		desc  string
	}{
		{"r", "no modifier"},
		{"", "empty string"},
		{"ctrl+", "missing key"},
		{"ctrl+shift+unknown", "unknown key"},
		{"fake+r", "unknown modifier"},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			_, _, err := ParseHotkey(tt.combo)
			if err == nil {
				t.Errorf("expected error for %q", tt.combo)
			}
		})
	}
}

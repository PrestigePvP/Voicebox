package main

import (
	"context"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/PrestigePvP/voicebox/internal/accessibility"
	"github.com/PrestigePvP/voicebox/internal/audio"
	"github.com/PrestigePvP/voicebox/internal/config"
	hk "github.com/PrestigePvP/voicebox/internal/hotkey"
	"github.com/PrestigePvP/voicebox/internal/pipeline"
	"github.com/wailsapp/wails/v2/pkg/runtime"
)

type App struct {
	ctx           context.Context
	cfg           *config.Config
	cleanupHotkey func()
	mu            sync.Mutex
	recording     bool
	capture       *audio.Capture
}

func NewApp() *App {
	return &App{}
}

func initLog() {
	dir := filepath.Join(os.Getenv("HOME"), ".config", "voicebox")
	os.MkdirAll(dir, 0o755)
	f, err := os.OpenFile(filepath.Join(dir, "voicebox.log"), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		log.Printf("Failed to open log file: %v", err)
		return
	}
	log.SetOutput(f)
	log.SetFlags(log.Ldate | log.Ltime | log.Lmicroseconds)
}

func (a *App) startup(ctx context.Context) {
	initLog()
	a.ctx = ctx
	a.cfg = loadConfig()

	cleanup, err := hk.RegisterHotkey(a.cfg.Hotkey.Record,
		func() { a.onHotkeyDown() },
		func() { a.onHotkeyUp() },
	)
	if err != nil {
		log.Printf("Failed to register hotkey %q: %v", a.cfg.Hotkey.Record, err)
		return
	}
	a.cleanupHotkey = cleanup

	log.Printf("VoiceBox ready (hotkey: %s)", a.cfg.Hotkey.Record)
}

func (a *App) shutdown(ctx context.Context) {
	if a.cleanupHotkey != nil {
		a.cleanupHotkey()
	}
}

func (a *App) showWindow() {
	if a.ctx != nil {
		platformShowWindow(a.ctx)
	}
}

func (a *App) quit() {
	if a.ctx != nil {
		runtime.Quit(a.ctx)
	}
}

func (a *App) toggleRecording() {
	a.mu.Lock()
	recording := a.recording
	a.mu.Unlock()

	if recording {
		a.onHotkeyUp()
	} else {
		a.onHotkeyDown()
	}
}

func (a *App) onHotkeyDown() {
	a.mu.Lock()
	if a.recording {
		a.mu.Unlock()
		return
	}
	a.recording = true
	a.mu.Unlock()

	focusCtx := accessibility.GetFocusContext()

	capture, err := audio.NewCapture(a.cfg.Audio.SampleRate, a.cfg.Audio.Channels, a.cfg.Audio.ChunkSize, func(level float64) {
		runtime.EventsEmit(a.ctx, "voicebox:level", level)
	})
	if err != nil {
		log.Printf("Failed to init audio capture: %v", err)
		a.mu.Lock()
		a.recording = false
		a.mu.Unlock()
		return
	}

	if err := capture.Start(); err != nil {
		capture.Close()
		log.Printf("Failed to start audio capture: %v", err)
		a.mu.Lock()
		a.recording = false
		a.mu.Unlock()
		return
	}

	a.mu.Lock()
	a.capture = capture
	a.mu.Unlock()

	platformShowWindow(a.ctx)
	runtime.EventsEmit(a.ctx, "voicebox:state", map[string]interface{}{
		"state":     "recording",
		"startTime": time.Now().UnixMilli(),
	})

	go a.runPipeline(capture, focusCtx)
}

func (a *App) onHotkeyUp() {
	a.mu.Lock()
	if !a.recording {
		a.mu.Unlock()
		return
	}
	capture := a.capture
	a.capture = nil
	a.recording = false
	a.mu.Unlock()

	if capture == nil {
		return
	}

	capture.Stop()

	runtime.EventsEmit(a.ctx, "voicebox:state", map[string]interface{}{
		"state": "processing",
		"stage": "stt",
	})
}

func (a *App) runPipeline(capture *audio.Capture, focusCtx accessibility.FocusContext) {
	defer capture.Close()

	result, err := pipeline.Run(
		context.Background(),
		a.cfg.Cloud.WorkerURL,
		a.cfg.Cloud.Token,
		pipeline.AudioParams{
			SampleRate: a.cfg.Audio.SampleRate,
			Channels:   a.cfg.Audio.Channels,
			Encoding:   "pcm_s16le",
		},
		pipeline.FocusContext{
			AppName:     focusCtx.AppName,
			BundleID:    focusCtx.BundleID,
			ElementRole: focusCtx.ElementRole,
			Title:       focusCtx.Title,
			Placeholder: focusCtx.Placeholder,
			Value:       focusCtx.Value,
		},
		capture.Chunks(),
		func(stage string) {
			runtime.EventsEmit(a.ctx, "voicebox:state", map[string]interface{}{
				"state": "processing",
				"stage": stage,
			})
		},
	)

	if err != nil {
		log.Printf("Pipeline error: %v", err)
		runtime.EventsEmit(a.ctx, "voicebox:state", map[string]interface{}{
			"state":   "error",
			"message": err.Error(),
		})
		time.Sleep(3 * time.Second)
		platformHideWindow(a.ctx)
		runtime.EventsEmit(a.ctx, "voicebox:state", map[string]interface{}{
			"state": "idle",
		})
		return
	}

	if err := copyToClipboard(result.Formatted); err != nil {
		log.Printf("Clipboard error: %v", err)
	}

	if focusCtx.PID > 0 {
		accessibility.PasteIntoApp(focusCtx.PID)
	}

	runtime.EventsEmit(a.ctx, "voicebox:state", map[string]interface{}{
		"state": "copied",
	})
	time.Sleep(1500 * time.Millisecond)
	runtime.WindowHide(a.ctx)
	runtime.EventsEmit(a.ctx, "voicebox:state", map[string]interface{}{
		"state": "idle",
	})
}

func copyToClipboard(text string) error {
	cmd := exec.Command("pbcopy")
	cmd.Stdin = strings.NewReader(text)
	return cmd.Run()
}

func loadConfig() *config.Config {
	var paths []string
	if home, err := os.UserHomeDir(); err == nil {
		paths = append(paths, filepath.Join(home, ".config", "voicebox", "voicebox.toml"))
	}
	if exe, err := os.Executable(); err == nil {
		paths = append(paths, filepath.Join(filepath.Dir(exe), "voicebox.toml"))
	}
	paths = append(paths, "voicebox.toml")

	for _, p := range paths {
		cfg, err := config.Load(p)
		if err == nil {
			log.Printf("Loaded config from %s", p)
			return cfg
		}
	}

	log.Fatal("Config not found. Create ~/.config/voicebox/voicebox.toml")
	return nil
}

//go:build !darwin

package main

import (
	"context"

	"github.com/wailsapp/wails/v2/pkg/runtime"
)

func platformShowWindow(ctx context.Context) {
	runtime.WindowShow(ctx)
	runtime.WindowSetAlwaysOnTop(ctx, true)
}

func platformHideWindow(ctx context.Context) {
	runtime.WindowHide(ctx)
}

func platformShowSettings(ctx context.Context) {
	runtime.WindowSetAlwaysOnTop(ctx, false)
	runtime.WindowSetSize(ctx, 700, 450)
	runtime.WindowCenter(ctx)
	runtime.WindowShow(ctx)
}

func platformSetupDockHandler(_ func()) {}

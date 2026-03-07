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

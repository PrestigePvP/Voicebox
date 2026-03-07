package stt

import "context"

type Result struct {
	Text     string
	Language string
}

type Provider interface {
	Transcribe(ctx context.Context, audio []byte) (*Result, error)
}

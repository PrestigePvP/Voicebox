package stt

import "context"

type LocalProvider struct{}

func (l *LocalProvider) Transcribe(_ context.Context, _ []byte) (*Result, error) {
	return nil, nil
}

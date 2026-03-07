package stt

import "context"

type CloudProvider struct{}

func (c *CloudProvider) Transcribe(_ context.Context, _ []byte) (*Result, error) {
	return nil, nil
}

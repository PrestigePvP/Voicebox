package formatter

import "context"

type LocalProvider struct{}

func (l *LocalProvider) Format(_ context.Context, _ string) (string, error) {
	return "", nil
}

package formatter

import "context"

type CloudProvider struct{}

func (c *CloudProvider) Format(_ context.Context, _ string) (string, error) {
	return "", nil
}

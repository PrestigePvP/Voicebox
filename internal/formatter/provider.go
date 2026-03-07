package formatter

import "context"

type Provider interface {
	Format(ctx context.Context, rawText string) (string, error)
}

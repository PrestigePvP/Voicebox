package main

import (
	"fmt"
	"strings"
)

const systemPrompt = "You are a text formatter. Take the raw speech-to-text transcription and return it with proper punctuation, capitalization, and paragraph breaks. Do not change the words, only fix formatting. Return only the formatted text."

const contextRules = `<rules>
- Chat/messaging apps (Slack, Discord, Messages): casual tone, minimal punctuation, no greeting/signature
- Email apps (Mail, Outlook): proper sentences, appropriate formality
- Code editors (VS Code, Xcode): preserve technical terms and code references exactly, this is often used when chatting to an AI assistant about code, so formatting should be clear and precise
- Search fields: concise, no punctuation unless necessary
- General text fields: standard formatting
- If existing text is present, format the new text so it flows naturally as a continuation
</rules>`

func buildSystemPrompt(ctx focusContext) string {
	hasContext := ctx.AppName != "" || ctx.ElementRole != "" || ctx.Title != "" || ctx.Placeholder != "" || ctx.Value != ""
	if !hasContext {
		return systemPrompt
	}
	return systemPrompt + "\n\n" + contextRules
}

func buildUserMessage(transcription string, ctx focusContext) string {
	var contextEntries []string
	fields := []struct {
		key, value string
	}{
		{"appName", ctx.AppName},
		{"bundleID", ctx.BundleID},
		{"elementRole", ctx.ElementRole},
		{"title", ctx.Title},
		{"placeholder", ctx.Placeholder},
		{"value", ctx.Value},
	}

	for _, f := range fields {
		if f.value != "" {
			contextEntries = append(contextEntries, fmt.Sprintf("  <%s>%s</%s>", f.key, f.value, f.key))
		}
	}

	transcriptionEntry := fmt.Sprintf("  <transcription>%s</transcription>", transcription)

	if len(contextEntries) == 0 {
		return transcriptionEntry
	}

	contextBlock := "<context>" + strings.Join(contextEntries, "\n") + "</context>"
	return contextBlock + "\n\n" + transcriptionEntry
}

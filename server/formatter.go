package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"regexp"
)

var thinkTagRegex = regexp.MustCompile(`(?s)<think>.*?</think>\s*`)

func format(endpoint, model, systemPrompt, userMsg string) (string, error) {
	reqBody, _ := json.Marshal(map[string]any{
		"model": model,
		"messages": []map[string]string{
			{"role": "system", "content": systemPrompt},
			{"role": "user", "content": userMsg},
		},
		"stream": false,
	})

	resp, err := http.Post(endpoint+"/api/chat", "application/json", bytes.NewReader(reqBody))
	if err != nil {
		return "", fmt.Errorf("formatter request failed: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", fmt.Errorf("reading formatter response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("formatter returned %d: %s", resp.StatusCode, string(respBody))
	}

	var result struct {
		Message struct {
			Content string `json:"content"`
		} `json:"message"`
	}
	if err := json.Unmarshal(respBody, &result); err != nil {
		return "", fmt.Errorf("parsing formatter response: %w", err)
	}

	content := thinkTagRegex.ReplaceAllString(result.Message.Content, "")
	return content, nil
}

package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
)

func transcribe(endpoint, model string, wavData []byte) (string, error) {
	var body bytes.Buffer
	writer := multipart.NewWriter(&body)

	part, err := writer.CreateFormFile("file", "audio.wav")
	if err != nil {
		return "", fmt.Errorf("creating form file: %w", err)
	}
	if _, err := part.Write(wavData); err != nil {
		return "", fmt.Errorf("writing wav data: %w", err)
	}

	if err := writer.WriteField("model", model); err != nil {
		return "", fmt.Errorf("writing model field: %w", err)
	}
	if err := writer.WriteField("response_format", "json"); err != nil {
		return "", fmt.Errorf("writing response_format field: %w", err)
	}

	writer.Close()

	resp, err := http.Post(endpoint+"/v1/audio/transcriptions", writer.FormDataContentType(), &body)
	if err != nil {
		return "", fmt.Errorf("STT request failed: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", fmt.Errorf("reading STT response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("STT returned %d: %s", resp.StatusCode, string(respBody))
	}

	var result struct {
		Text string `json:"text"`
	}
	if err := json.Unmarshal(respBody, &result); err != nil {
		return "", fmt.Errorf("parsing STT response: %w", err)
	}

	return result.Text, nil
}

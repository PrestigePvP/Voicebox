package pipeline

import (
	"context"
	"encoding/json"
	"fmt"
	"net/url"
	"strconv"
	"strings"

	"github.com/gorilla/websocket"
)

type Result struct {
	Raw       string
	Formatted string
}

type AudioParams struct {
	SampleRate int
	Channels   int
	Encoding   string
}

type FocusContext struct {
	AppName     string
	BundleID    string
	ElementRole string
	Title       string
	Placeholder string
	Value       string
}

type serverMessage struct {
	Type      string `json:"type"`
	Stage     string `json:"stage,omitempty"`
	Raw       string `json:"raw,omitempty"`
	Formatted string `json:"formatted,omitempty"`
	Code      string `json:"code,omitempty"`
	Message   string `json:"message,omitempty"`
}

func Run(ctx context.Context, workerURL, token string, params AudioParams, focus FocusContext, chunks <-chan []byte, onStage func(string)) (*Result, error) {
	wsURL := strings.Replace(workerURL, "https://", "wss://", 1)
	wsURL = strings.Replace(wsURL, "http://", "ws://", 1)

	u, err := url.Parse(wsURL + "/ws")
	if err != nil {
		return nil, fmt.Errorf("invalid worker URL: %w", err)
	}

	q := u.Query()
	q.Set("token", token)
	q.Set("sampleRate", strconv.Itoa(params.SampleRate))
	q.Set("channels", strconv.Itoa(params.Channels))
	q.Set("encoding", params.Encoding)
	if focus.AppName != "" {
		q.Set("appName", focus.AppName)
	}
	if focus.BundleID != "" {
		q.Set("bundleID", focus.BundleID)
	}
	if focus.ElementRole != "" {
		q.Set("elementRole", focus.ElementRole)
	}
	if focus.Title != "" {
		q.Set("title", focus.Title)
	}
	if focus.Placeholder != "" {
		q.Set("placeholder", focus.Placeholder)
	}
	if focus.Value != "" {
		q.Set("value", focus.Value)
	}
	u.RawQuery = q.Encode()

	conn, _, err := websocket.DefaultDialer.DialContext(ctx, u.String(), nil)
	if err != nil {
		return nil, fmt.Errorf("websocket connect: %w", err)
	}
	defer conn.Close()

	var msg serverMessage
	if err := conn.ReadJSON(&msg); err != nil {
		return nil, fmt.Errorf("reading ready: %w", err)
	}
	if msg.Type != "ready" {
		return nil, fmt.Errorf("expected ready, got %s", msg.Type)
	}

	for chunk := range chunks {
		if err := conn.WriteMessage(websocket.BinaryMessage, chunk); err != nil {
			return nil, fmt.Errorf("sending audio: %w", err)
		}
	}

	end, _ := json.Marshal(map[string]string{"type": "audio_end"})
	if err := conn.WriteMessage(websocket.TextMessage, end); err != nil {
		return nil, fmt.Errorf("sending audio_end: %w", err)
	}

	for {
		msg = serverMessage{}
		if err := conn.ReadJSON(&msg); err != nil {
			return nil, fmt.Errorf("reading response: %w", err)
		}
		switch msg.Type {
		case "processing":
			if onStage != nil {
				onStage(msg.Stage)
			}
		case "result":
			return &Result{Raw: msg.Raw, Formatted: msg.Formatted}, nil
		case "error":
			return nil, fmt.Errorf("%s: %s", msg.Code, msg.Message)
		}
	}
}

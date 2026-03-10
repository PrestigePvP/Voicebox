package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gorilla/websocket"
)

func startTestServer(cfg *ServerConfig) (*httptest.Server, string) {
	mux := http.NewServeMux()
	mux.HandleFunc("/ws", func(w http.ResponseWriter, r *http.Request) {
		handleWebSocket(w, r, cfg)
	})
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"status":"ok"}`))
	})
	ts := httptest.NewServer(mux)
	wsURL := "ws" + ts.URL[4:] + "/ws"
	return ts, wsURL
}

func TestHealthEndpoint(t *testing.T) {
	ts, _ := startTestServer(&ServerConfig{})
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/health")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		t.Fatalf("expected 200, got %d", resp.StatusCode)
	}
}

func TestAuthRejection(t *testing.T) {
	ts, wsURL := startTestServer(&ServerConfig{Token: "secret"})
	defer ts.Close()

	_, resp, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err == nil {
		t.Fatal("expected auth rejection")
	}
	if resp != nil && resp.StatusCode != 401 {
		t.Fatalf("expected 401, got %d", resp.StatusCode)
	}
}

func TestAuthAccepted(t *testing.T) {
	ts, wsURL := startTestServer(&ServerConfig{Token: "secret"})
	defer ts.Close()

	header := http.Header{}
	header.Set("Authorization", "Bearer secret")
	conn, _, err := websocket.DefaultDialer.Dial(wsURL, header)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()

	var msg serverMessage
	if err := conn.ReadJSON(&msg); err != nil {
		t.Fatal(err)
	}
	if msg.Type != "ready" {
		t.Fatalf("expected ready, got %s", msg.Type)
	}
}

func TestNoAuthRequired(t *testing.T) {
	ts, wsURL := startTestServer(&ServerConfig{})
	defer ts.Close()

	conn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()

	var msg serverMessage
	if err := conn.ReadJSON(&msg); err != nil {
		t.Fatal(err)
	}
	if msg.Type != "ready" {
		t.Fatalf("expected ready, got %s", msg.Type)
	}
}

func TestWebSocketProtocol(t *testing.T) {
	// Mock STT server
	sttServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"text":"hello world"}`))
	}))
	defer sttServer.Close()

	// Mock Ollama server
	ollamaServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"message":{"content":"Hello world."}}`))
	}))
	defer ollamaServer.Close()

	ts, wsURL := startTestServer(&ServerConfig{
		STTEndpoint:       sttServer.URL,
		STTModel:          "test-model",
		FormatterEndpoint: ollamaServer.URL,
		FormatterModel:    "test-model",
	})
	defer ts.Close()

	conn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()

	// 1. Receive ready
	var msg serverMessage
	if err := conn.ReadJSON(&msg); err != nil {
		t.Fatal(err)
	}
	if msg.Type != "ready" {
		t.Fatalf("expected ready, got %s", msg.Type)
	}

	// 2. Send configure
	configure, _ := json.Marshal(map[string]any{
		"type":  "configure",
		"audio": map[string]any{"sampleRate": 16000, "channels": 1, "encoding": "pcm_s16le"},
		"context": map[string]any{
			"appName":     "Slack",
			"bundleID":    "com.tinyspeck.slackmacgap",
			"elementRole": "AXTextField",
		},
	})
	if err := conn.WriteMessage(websocket.TextMessage, configure); err != nil {
		t.Fatal(err)
	}

	// 3. Send PCM audio (fake data)
	pcm := make([]byte, 4096)
	if err := conn.WriteMessage(websocket.BinaryMessage, pcm); err != nil {
		t.Fatal(err)
	}

	// 4. Send audio_end
	end, _ := json.Marshal(map[string]string{"type": "audio_end"})
	if err := conn.WriteMessage(websocket.TextMessage, end); err != nil {
		t.Fatal(err)
	}

	// 5. Receive processing (stt)
	if err := conn.ReadJSON(&msg); err != nil {
		t.Fatal(err)
	}
	if msg.Type != "processing" || msg.Stage != "stt" {
		t.Fatalf("expected processing/stt, got %s/%s", msg.Type, msg.Stage)
	}

	// 6. Receive processing (format)
	if err := conn.ReadJSON(&msg); err != nil {
		t.Fatal(err)
	}
	if msg.Type != "processing" || msg.Stage != "format" {
		t.Fatalf("expected processing/format, got %s/%s", msg.Type, msg.Stage)
	}

	// 7. Receive result
	if err := conn.ReadJSON(&msg); err != nil {
		t.Fatal(err)
	}
	if msg.Type != "result" {
		t.Fatalf("expected result, got %s", msg.Type)
	}
	if msg.Raw != "hello world" {
		t.Fatalf("expected raw 'hello world', got %q", msg.Raw)
	}
	if msg.Formatted != "Hello world." {
		t.Fatalf("expected formatted 'Hello world.', got %q", msg.Formatted)
	}
}

func TestThinkTagStripping(t *testing.T) {
	sttServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"text":"test"}`))
	}))
	defer sttServer.Close()

	ollamaServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"message":{"content":"<think>reasoning here</think>\nFormatted text."}}`))
	}))
	defer ollamaServer.Close()

	ts, wsURL := startTestServer(&ServerConfig{
		STTEndpoint:       sttServer.URL,
		STTModel:          "test",
		FormatterEndpoint: ollamaServer.URL,
		FormatterModel:    "test",
	})
	defer ts.Close()

	conn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()

	var msg serverMessage
	conn.ReadJSON(&msg) // ready

	configure, _ := json.Marshal(map[string]string{"type": "configure"})
	conn.WriteMessage(websocket.TextMessage, configure)

	conn.WriteMessage(websocket.BinaryMessage, make([]byte, 100))

	end, _ := json.Marshal(map[string]string{"type": "audio_end"})
	conn.WriteMessage(websocket.TextMessage, end)

	conn.ReadJSON(&msg) // processing stt
	conn.ReadJSON(&msg) // processing format
	conn.ReadJSON(&msg) // result

	if msg.Formatted != "Formatted text." {
		t.Fatalf("expected think tags stripped, got %q", msg.Formatted)
	}
}

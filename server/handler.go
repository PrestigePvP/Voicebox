package main

import (
	"encoding/json"
	"log"
	"net/http"

	"github.com/gorilla/websocket"
)

const maxAudioBytes = 25 * 1024 * 1024

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true },
}

type audioConfig struct {
	SampleRate int    `json:"sampleRate"`
	Channels   int    `json:"channels"`
	Encoding   string `json:"encoding"`
}

type focusContext struct {
	AppName     string `json:"appName"`
	BundleID    string `json:"bundleID"`
	ElementRole string `json:"elementRole"`
	Title       string `json:"title"`
	Placeholder string `json:"placeholder"`
	Value       string `json:"value"`
}

type clientMessage struct {
	Type    string        `json:"type"`
	Audio   *audioConfig  `json:"audio,omitempty"`
	Context *focusContext `json:"context,omitempty"`
}

type serverMessage struct {
	Type      string `json:"type"`
	Stage     string `json:"stage,omitempty"`
	Raw       string `json:"raw,omitempty"`
	Formatted string `json:"formatted,omitempty"`
	Code      string `json:"code,omitempty"`
	Message   string `json:"message,omitempty"`
}

func handleWebSocket(w http.ResponseWriter, r *http.Request, cfg *ServerConfig) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	if cfg.Token != "" {
		auth := r.Header.Get("Authorization")
		expected := "Bearer " + cfg.Token
		if auth != expected {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusUnauthorized)
			w.Write([]byte(`{"error":"auth_failed"}`))
			return
		}
	}

	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("WebSocket upgrade failed: %v", err)
		return
	}
	defer conn.Close()

	sendJSON(conn, serverMessage{Type: "ready"})

	audioCfg := audioConfig{SampleRate: 16000, Channels: 1, Encoding: "pcm_s16le"}
	focusCtx := focusContext{}
	var pcmBuf []byte

	for {
		msgType, data, err := conn.ReadMessage()
		if err != nil {
			log.Printf("Read error: %v", err)
			return
		}

		if msgType == websocket.BinaryMessage {
			if len(pcmBuf)+len(data) > maxAudioBytes {
				sendJSON(conn, serverMessage{Type: "error", Code: "audio_too_large", Message: "Audio exceeds 25MB limit"})
				conn.WriteMessage(websocket.CloseMessage,
					websocket.FormatCloseMessage(1009, "Audio too large"))
				return
			}
			pcmBuf = append(pcmBuf, data...)
			continue
		}

		var msg clientMessage
		if err := json.Unmarshal(data, &msg); err != nil {
			log.Printf("Invalid JSON: %v", err)
			continue
		}

		switch msg.Type {
		case "configure":
			if msg.Audio != nil {
				if msg.Audio.SampleRate > 0 {
					audioCfg.SampleRate = msg.Audio.SampleRate
				}
				if msg.Audio.Channels > 0 {
					audioCfg.Channels = msg.Audio.Channels
				}
				if msg.Audio.Encoding != "" {
					audioCfg.Encoding = msg.Audio.Encoding
				}
			}
			if msg.Context != nil {
				focusCtx = *msg.Context
			}

		case "audio_end":
			processAudio(conn, cfg, audioCfg, focusCtx, pcmBuf)
			return

		case "cancel":
			conn.WriteMessage(websocket.CloseMessage,
				websocket.FormatCloseMessage(websocket.CloseNormalClosure, "Cancelled"))
			return
		}
	}
}

func processAudio(conn *websocket.Conn, cfg *ServerConfig, audioCfg audioConfig, focusCtx focusContext, pcm []byte) {
	sendJSON(conn, serverMessage{Type: "processing", Stage: "stt"})

	wavData := wrapPCMAsWAV(pcm, audioCfg.SampleRate, audioCfg.Channels, 16)

	transcription, err := transcribe(cfg.STTEndpoint, cfg.STTModel, wavData)
	if err != nil {
		log.Printf("STT error: %v", err)
		sendJSON(conn, serverMessage{Type: "error", Code: "stt_failed", Message: err.Error()})
		conn.WriteMessage(websocket.CloseMessage,
			websocket.FormatCloseMessage(1011, "STT failed"))
		return
	}

	sendJSON(conn, serverMessage{Type: "processing", Stage: "format"})

	systemPrompt := buildSystemPrompt(focusCtx)
	userMsg := buildUserMessage(transcription, focusCtx)

	formatted, err := format(cfg.FormatterEndpoint, cfg.FormatterModel, systemPrompt, userMsg)
	if err != nil {
		log.Printf("Format error: %v", err)
		sendJSON(conn, serverMessage{Type: "error", Code: "format_failed", Message: err.Error()})
		conn.WriteMessage(websocket.CloseMessage,
			websocket.FormatCloseMessage(1011, "Format failed"))
		return
	}

	sendJSON(conn, serverMessage{Type: "result", Raw: transcription, Formatted: formatted})
	conn.WriteMessage(websocket.CloseMessage,
		websocket.FormatCloseMessage(websocket.CloseNormalClosure, "Complete"))
}

func sendJSON(conn *websocket.Conn, msg serverMessage) {
	data, _ := json.Marshal(msg)
	conn.WriteMessage(websocket.TextMessage, data)
}

package main

import (
	"flag"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
)

func main() {
	port := flag.String("port", "9090", "Listen port")
	sttEndpoint := flag.String("stt-endpoint", "http://localhost:8000", "faster-whisper-server URL")
	sttModel := flag.String("stt-model", "Systran/faster-whisper-small", "STT model name")
	formatterEndpoint := flag.String("formatter-endpoint", "http://localhost:11434", "Ollama URL")
	formatterModel := flag.String("formatter-model", "llama3.2:3b", "Ollama model name")
	token := flag.String("token", "", "Auth token for clients (empty = no auth)")
	flag.Parse()

	cfg := &ServerConfig{
		STTEndpoint:       *sttEndpoint,
		STTModel:          *sttModel,
		FormatterEndpoint: *formatterEndpoint,
		FormatterModel:    *formatterModel,
		Token:             *token,
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/ws", func(w http.ResponseWriter, r *http.Request) {
		handleWebSocket(w, r, cfg)
	})
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"status":"ok"}`))
	})

	go func() {
		log.Printf("voicebox-server listening on :%s", *port)
		if err := http.ListenAndServe(":"+*port, mux); err != nil {
			log.Fatal(err)
		}
	}()

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	<-sig
	log.Println("Shutting down")
}

type ServerConfig struct {
	STTEndpoint       string
	STTModel          string
	FormatterEndpoint string
	FormatterModel    string
	Token             string
}

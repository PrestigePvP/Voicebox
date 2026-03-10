package main

import (
	"encoding/binary"
	"testing"
)

func TestWrapPCMAsWAV(t *testing.T) {
	pcm := make([]byte, 3200) // 100ms of 16kHz mono 16-bit
	wav := wrapPCMAsWAV(pcm, 16000, 1, 16)

	if len(wav) != 44+3200 {
		t.Fatalf("expected %d bytes, got %d", 44+3200, len(wav))
	}

	if string(wav[0:4]) != "RIFF" {
		t.Fatal("missing RIFF header")
	}
	if string(wav[8:12]) != "WAVE" {
		t.Fatal("missing WAVE format")
	}
	if string(wav[12:16]) != "fmt " {
		t.Fatal("missing fmt chunk")
	}
	if string(wav[36:40]) != "data" {
		t.Fatal("missing data chunk")
	}

	fileSize := binary.LittleEndian.Uint32(wav[4:8])
	if fileSize != uint32(36+3200) {
		t.Fatalf("RIFF size: expected %d, got %d", 36+3200, fileSize)
	}

	channels := binary.LittleEndian.Uint16(wav[22:24])
	if channels != 1 {
		t.Fatalf("channels: expected 1, got %d", channels)
	}

	sampleRate := binary.LittleEndian.Uint32(wav[24:28])
	if sampleRate != 16000 {
		t.Fatalf("sample rate: expected 16000, got %d", sampleRate)
	}

	dataSize := binary.LittleEndian.Uint32(wav[40:44])
	if dataSize != 3200 {
		t.Fatalf("data size: expected 3200, got %d", dataSize)
	}
}

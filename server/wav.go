package main

import "encoding/binary"

func wrapPCMAsWAV(pcm []byte, sampleRate, channels, bitsPerSample int) []byte {
	dataLen := len(pcm)
	byteRate := sampleRate * channels * (bitsPerSample / 8)
	blockAlign := channels * (bitsPerSample / 8)

	wav := make([]byte, 44+dataLen)

	copy(wav[0:4], "RIFF")
	binary.LittleEndian.PutUint32(wav[4:8], uint32(36+dataLen))
	copy(wav[8:12], "WAVE")
	copy(wav[12:16], "fmt ")
	binary.LittleEndian.PutUint32(wav[16:20], 16)
	binary.LittleEndian.PutUint16(wav[20:22], 1)
	binary.LittleEndian.PutUint16(wav[22:24], uint16(channels))
	binary.LittleEndian.PutUint32(wav[24:28], uint32(sampleRate))
	binary.LittleEndian.PutUint32(wav[28:32], uint32(byteRate))
	binary.LittleEndian.PutUint16(wav[32:34], uint16(blockAlign))
	binary.LittleEndian.PutUint16(wav[34:36], uint16(bitsPerSample))
	copy(wav[36:40], "data")
	binary.LittleEndian.PutUint32(wav[40:44], uint32(dataLen))

	copy(wav[44:], pcm)
	return wav
}

import { describe, expect, it } from "vitest";
import { buildWavHeader, wrapPcmAsWav } from "../src/wav";

describe("buildWavHeader", () => {
  it("produces a 44-byte header", () => {
    const header = buildWavHeader(0, 16000, 1, 16);
    expect(header.byteLength).toBe(44);
  });

  it("has correct RIFF/WAVE/fmt/data markers", () => {
    const header = buildWavHeader(100, 16000, 1, 16);
    const view = new DataView(header);
    const decoder = new TextDecoder("ascii");

    expect(decoder.decode(new Uint8Array(header, 0, 4))).toBe("RIFF");
    expect(decoder.decode(new Uint8Array(header, 8, 4))).toBe("WAVE");
    expect(decoder.decode(new Uint8Array(header, 12, 4))).toBe("fmt ");
    expect(decoder.decode(new Uint8Array(header, 36, 4))).toBe("data");

    // RIFF chunk size = 36 + dataLength
    expect(view.getUint32(4, true)).toBe(136);
    // data chunk size
    expect(view.getUint32(40, true)).toBe(100);
  });

  it("encodes audio format fields correctly for 16kHz mono 16-bit", () => {
    const header = buildWavHeader(3200, 16000, 1, 16);
    const view = new DataView(header);

    // fmt chunk size
    expect(view.getUint32(16, true)).toBe(16);
    // PCM format (1)
    expect(view.getUint16(20, true)).toBe(1);
    // channels
    expect(view.getUint16(22, true)).toBe(1);
    // sample rate
    expect(view.getUint32(24, true)).toBe(16000);
    // byte rate: 16000 * 1 * 2 = 32000
    expect(view.getUint32(28, true)).toBe(32000);
    // block align: 1 * 2 = 2
    expect(view.getUint16(32, true)).toBe(2);
    // bits per sample
    expect(view.getUint16(34, true)).toBe(16);
  });

  it("encodes stereo 44100Hz correctly", () => {
    const header = buildWavHeader(0, 44100, 2, 16);
    const view = new DataView(header);

    expect(view.getUint16(22, true)).toBe(2);
    expect(view.getUint32(24, true)).toBe(44100);
    // byte rate: 44100 * 2 * 2 = 176400
    expect(view.getUint32(28, true)).toBe(176400);
    // block align: 2 * 2 = 4
    expect(view.getUint16(32, true)).toBe(4);
  });
});

describe("wrapPcmAsWav", () => {
  it("prepends 44-byte header to PCM data", () => {
    const pcm = new Uint8Array([0x01, 0x02, 0x03, 0x04]);
    const wav = wrapPcmAsWav(pcm, 16000, 1, 16);

    expect(wav.byteLength).toBe(48);
    // PCM data preserved after header
    expect(wav[44]).toBe(0x01);
    expect(wav[45]).toBe(0x02);
    expect(wav[46]).toBe(0x03);
    expect(wav[47]).toBe(0x04);
  });

  it("sets data length in header to match PCM size", () => {
    const pcm = new Uint8Array(3200);
    const wav = wrapPcmAsWav(pcm, 16000, 1, 16);
    const view = new DataView(wav.buffer, wav.byteOffset);

    // data chunk size
    expect(view.getUint32(40, true)).toBe(3200);
    // RIFF chunk size
    expect(view.getUint32(4, true)).toBe(36 + 3200);
  });

  it("handles empty PCM", () => {
    const pcm = new Uint8Array(0);
    const wav = wrapPcmAsWav(pcm, 16000, 1, 16);

    expect(wav.byteLength).toBe(44);
    const view = new DataView(wav.buffer, wav.byteOffset);
    expect(view.getUint32(40, true)).toBe(0);
  });
});

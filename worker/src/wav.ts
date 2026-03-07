export const buildWavHeader = (
  dataLength: number,
  sampleRate: number,
  channels: number,
  bitsPerSample: number,
): ArrayBuffer => {
  const header = new ArrayBuffer(44);
  const view = new DataView(header);

  const writeString = (offset: number, str: string) => {
    for (let i = 0; i < str.length; i++) {
      view.setUint8(offset + i, str.charCodeAt(i));
    }
  };

  const byteRate = sampleRate * channels * (bitsPerSample / 8);
  const blockAlign = channels * (bitsPerSample / 8);

  writeString(0, "RIFF");
  view.setUint32(4, 36 + dataLength, true);
  writeString(8, "WAVE");
  writeString(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, channels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, byteRate, true);
  view.setUint16(32, blockAlign, true);
  view.setUint16(34, bitsPerSample, true);
  writeString(36, "data");
  view.setUint32(40, dataLength, true);

  return header;
};

export const wrapPcmAsWav = (
  pcm: Uint8Array,
  sampleRate: number,
  channels: number,
  bitsPerSample: number,
): Uint8Array => {
  const header = buildWavHeader(pcm.byteLength, sampleRate, channels, bitsPerSample);
  const wav = new Uint8Array(44 + pcm.byteLength);
  wav.set(new Uint8Array(header), 0);
  wav.set(pcm, 44);
  return wav;
};

use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

/// Incremental WAV writer for long recordings. The header is patched in place
/// on every `flush_header`, so the file on disk is a valid, playable WAV up to
/// the last flush even if the process dies mid-recording.
pub struct WavWriter {
    file: File,
    data_bytes: u64,
}

const HEADER_LEN: u64 = 44;

impl WavWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> io::Result<Self> {
        let mut file = File::create(path)?;
        file.write_all(&header(sample_rate, channels, 0))?;
        Ok(Self { file, data_bytes: 0 })
    }

    pub fn write(&mut self, pcm: &[u8]) -> io::Result<()> {
        self.file.write_all(pcm)?;
        self.data_bytes += pcm.len() as u64;
        Ok(())
    }

    pub fn flush_header(&mut self) -> io::Result<()> {
        let riff_size = (HEADER_LEN - 8 + self.data_bytes) as u32;
        let data_size = self.data_bytes as u32;
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&riff_size.to_le_bytes())?;
        self.file.seek(SeekFrom::Start(40))?;
        self.file.write_all(&data_size.to_le_bytes())?;
        self.file.seek(SeekFrom::End(0))?;
        self.file.sync_data()
    }

    pub fn finalize(mut self) -> io::Result<u64> {
        self.flush_header()?;
        self.file.sync_all()?;
        Ok(self.data_bytes)
    }
}

fn header(sample_rate: u32, channels: u16, data_bytes: u32) -> [u8; HEADER_LEN as usize] {
    let bits_per_sample: u16 = 16;
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;

    let mut h = [0u8; HEADER_LEN as usize];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("voicebox-wav-test-{}-{}.wav", name, std::process::id()))
    }

    #[test]
    fn writes_valid_header_and_patches_sizes() {
        let path = temp_path("patch");
        let mut w = WavWriter::create(&path, 16000, 1).unwrap();
        w.write(&[0u8; 32000]).unwrap();
        w.flush_header().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36 + 32000);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 16000);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 32000);
        assert_eq!(bytes.len() as u64, HEADER_LEN + 32000);

        let total = w.write(&[0u8; 4096]).and_then(|_| w.finalize()).unwrap();
        assert_eq!(total, 36096);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 36096);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_is_valid_before_any_flush() {
        let path = temp_path("early");
        let w = WavWriter::create(&path, 16000, 1).unwrap();
        drop(w);

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, HEADER_LEN);
        assert_eq!(&bytes[36..40], b"data");
        let _ = std::fs::remove_file(&path);
    }
}

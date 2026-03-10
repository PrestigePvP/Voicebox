use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri::Emitter;
use tokio::sync::mpsc;

pub struct AudioCaptureHandle {
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AudioCaptureHandle {
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

impl Drop for AudioCaptureHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn start_capture(
    target_sample_rate: u32,
    _channels: u16,
    chunk_size: usize,
    app_handle: AppHandle,
) -> Result<(AudioCaptureHandle, mpsc::Receiver<Vec<u8>>), String> {
    let (chunk_tx, chunk_rx) = mpsc::channel::<Vec<u8>>(64);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    let thread = std::thread::spawn(move || {
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                let _ = ready_tx.send(Err("No input device available".into()));
                return;
            }
        };

        // Query device's supported configs and pick the best one
        let supported = match device.supported_input_configs() {
            Ok(s) => s.collect::<Vec<_>>(),
            Err(e) => {
                let _ = ready_tx.send(Err(format!("Failed to query input configs: {}", e)));
                return;
            }
        };

        // Try to find a config: prefer target rate mono, fall back to device default
        let (stream_config, device_rate, device_channels) = {
            // First try: exact target rate, mono
            let exact = supported.iter().find(|c| {
                c.channels() == 1
                    && c.min_sample_rate().0 <= target_sample_rate
                    && c.max_sample_rate().0 >= target_sample_rate
            });

            if let Some(cfg) = exact {
                let sc = cfg.with_sample_rate(cpal::SampleRate(target_sample_rate));
                (sc.into(), target_sample_rate, 1u16)
            } else {
                // Use the device's default config
                match device.default_input_config() {
                    Ok(cfg) => {
                        let rate = cfg.sample_rate().0;
                        let ch = cfg.channels();
                        log::info!(
                            "Device doesn't support {}Hz mono, using native {}Hz {}ch (will resample)",
                            target_sample_rate,
                            rate,
                            ch
                        );
                        (cfg.into(), rate, ch)
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("No supported input config: {}", e)));
                        return;
                    }
                }
            }
        };

        let needs_conversion = device_rate != target_sample_rate || device_channels != 1;
        let downsample_ratio = if needs_conversion {
            device_rate as f64 / target_sample_rate as f64
        } else {
            1.0
        };

        let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let buf_cb = buf.clone();
        let stop_cb = stop_flag_thread.clone();
        let last_level_at = Arc::new(Mutex::new(std::time::Instant::now()));

        let stream = match device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if stop_cb.load(Ordering::Relaxed) {
                    return;
                }

                // Convert to mono i16 at target sample rate
                let mono_samples: Vec<i16> = if needs_conversion {
                    resample_to_mono_i16(data, device_channels, downsample_ratio)
                } else {
                    data.iter()
                        .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
                        .collect()
                };

                // RMS level emission (~30fps)
                {
                    let mut last = last_level_at.lock().unwrap();
                    if last.elapsed().as_millis() >= 33 {
                        *last = std::time::Instant::now();
                        let rms = rms_level(&mono_samples);
                        let _ = app_handle.emit("voicebox:level", rms);
                    }
                }

                let bytes: Vec<u8> = mono_samples
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();

                let mut buffer = buf_cb.lock().unwrap();
                buffer.extend_from_slice(&bytes);

                while buffer.len() >= chunk_size {
                    let chunk: Vec<u8> = buffer.drain(..chunk_size).collect();
                    let _ = chunk_tx.try_send(chunk);
                }
            },
            move |err| {
                log::error!("Audio input error: {}", err);
            },
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                let _ = ready_tx.send(Err(format!("Failed to build input stream: {}", e)));
                return;
            }
        };

        if let Err(e) = stream.play() {
            let _ = ready_tx.send(Err(format!("Failed to start stream: {}", e)));
            return;
        }

        let _ = ready_tx.send(Ok(()));

        while !stop_flag_thread.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        drop(stream);
    });

    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Audio thread died before reporting ready".into()),
    }

    Ok((
        AudioCaptureHandle {
            stop_flag,
            thread: Some(thread),
        },
        chunk_rx,
    ))
}

/// Downmix to mono and resample from device rate to target rate using linear interpolation
fn resample_to_mono_i16(data: &[f32], channels: u16, downsample_ratio: f64) -> Vec<i16> {
    let ch = channels as usize;
    let frame_count = data.len() / ch;
    let output_len = (frame_count as f64 / downsample_ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f64 * downsample_ratio;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f64;

        let sample_a = if src_idx < frame_count {
            let start = src_idx * ch;
            let sum: f32 = data[start..start + ch].iter().sum();
            sum / ch as f32
        } else {
            0.0
        };

        let sample_b = if src_idx + 1 < frame_count {
            let start = (src_idx + 1) * ch;
            let sum: f32 = data[start..start + ch].iter().sum();
            sum / ch as f32
        } else {
            sample_a
        };

        let interpolated = sample_a as f64 * (1.0 - frac) + sample_b as f64 * frac;
        let clamped = (interpolated * 32767.0).clamp(-32768.0, 32767.0) as i16;
        output.push(clamped);
    }

    output
}

fn rms_level(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / samples.len() as f64).sqrt() / 32768.0
}

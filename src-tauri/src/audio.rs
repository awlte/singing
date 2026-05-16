use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

pub struct RingBuffer {
    samples: Vec<i16>,
    write_pos: usize,
    is_full: bool,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0i16; capacity],
            write_pos: 0,
            is_full: false,
        }
    }

    pub fn capacity(&self) -> usize {
        self.samples.len()
    }

    pub fn len(&self) -> usize {
        if self.is_full {
            self.samples.len()
        } else {
            self.write_pos
        }
    }

    pub fn extend_from(&mut self, src: &[i16]) {
        let cap = self.samples.len();
        if cap == 0 {
            return;
        }
        for &s in src {
            self.samples[self.write_pos] = s;
            self.write_pos += 1;
            if self.write_pos >= cap {
                self.write_pos = 0;
                self.is_full = true;
            }
        }
    }

    /// Wipe the buffer (used when switching devices, so old samples don't
    /// linger on the timeline after the source changed).
    pub fn clear(&mut self) {
        for s in self.samples.iter_mut() {
            *s = 0;
        }
        self.write_pos = 0;
        self.is_full = false;
    }

    /// Reallocate to a new capacity, discarding existing content.
    pub fn resize(&mut self, new_capacity: usize) {
        self.samples = vec![0i16; new_capacity];
        self.write_pos = 0;
        self.is_full = false;
    }

    pub fn snapshot(&self) -> Vec<i16> {
        if self.is_full {
            let mut out = Vec::with_capacity(self.samples.len());
            out.extend_from_slice(&self.samples[self.write_pos..]);
            out.extend_from_slice(&self.samples[..self.write_pos]);
            out
        } else {
            self.samples[..self.write_pos].to_vec()
        }
    }
}

pub fn peaks(samples: &[i16], width: usize) -> Vec<i16> {
    if samples.is_empty() || width == 0 {
        return vec![0; width];
    }
    let n = samples.len();
    (0..width)
        .map(|i| {
            let start = (i * n) / width;
            let end = ((i + 1) * n) / width;
            if start >= end {
                return 0;
            }
            samples[start..end]
                .iter()
                .map(|s| s.saturating_abs())
                .max()
                .unwrap_or(0)
        })
        .collect()
}

pub fn build_wav_bytes(samples: &[i16]) -> std::io::Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        for &s in samples {
            writer
                .write_sample(s)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

pub fn save_wav(path: &std::path::Path, samples: &[i16]) -> std::io::Result<()> {
    let bytes = build_wav_bytes(samples)?;
    std::fs::write(path, bytes)
}

pub enum AudioCommand {
    /// Switch to the named input device. `None` means "system default".
    SetDevice(Option<String>),
}

#[derive(Clone)]
pub struct AudioControl {
    pub command_tx: Sender<AudioCommand>,
    pub current_device: Arc<Mutex<Option<String>>>,
}

impl AudioControl {
    pub fn current(&self) -> Option<String> {
        self.current_device.lock().unwrap().clone()
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());
    match host.input_devices() {
        Ok(iter) => iter
            .filter_map(|d| {
                let name = d.name().ok()?;
                let is_default = Some(&name) == default_name.as_ref();
                Some(DeviceInfo { name, is_default })
            })
            .collect(),
        Err(_) => vec![],
    }
}

/// Spawns a dedicated thread that runs the capture loop.
/// The loop builds an input stream, plays it, and waits for commands.
/// On `SetDevice`, it tears down the current stream and rebuilds with the new device.
pub fn start_capture(buffer: Arc<Mutex<RingBuffer>>) -> AudioControl {
    let (tx, rx) = channel::<AudioCommand>();
    let current_device = Arc::new(Mutex::new(None::<String>));
    let current_clone = current_device.clone();
    std::thread::Builder::new()
        .name("audio-capture".into())
        .spawn(move || capture_loop(buffer, rx, current_clone))
        .expect("failed to spawn audio-capture thread");
    AudioControl {
        command_tx: tx,
        current_device,
    }
}

fn capture_loop(
    buffer: Arc<Mutex<RingBuffer>>,
    rx: Receiver<AudioCommand>,
    current_device: Arc<Mutex<Option<String>>>,
) {
    let mut requested: Option<String> = None;
    loop {
        let stream = match build_stream(&buffer, requested.as_deref()) {
            Ok((stream, resolved_name)) => {
                *current_device.lock().unwrap() = Some(resolved_name);
                Some(stream)
            }
            Err(e) => {
                eprintln!("[audio] build_stream failed: {e}");
                *current_device.lock().unwrap() = None;
                None
            }
        };

        // Block on the next command. When it arrives, drop the stream
        // (stops capture) and rebuild on the next loop iteration.
        match rx.recv() {
            Ok(AudioCommand::SetDevice(name)) => {
                requested = name;
                drop(stream);
                buffer.lock().unwrap().clear();
            }
            Err(_) => {
                // Sender hung up — process is shutting down. Park forever
                // so the stream keeps capturing until the OS reaps us.
                if let Some(s) = stream {
                    std::mem::forget(s);
                }
                loop {
                    std::thread::park();
                }
            }
        }
    }
}

fn build_stream(
    buffer: &Arc<Mutex<RingBuffer>>,
    requested: Option<&str>,
) -> Result<(cpal::Stream, String), String> {
    let host = cpal::default_host();
    let device = match requested {
        Some(name) => host
            .input_devices()
            .map_err(|e| format!("input_devices: {e}"))?
            .find(|d| d.name().ok().as_deref() == Some(name))
            .ok_or_else(|| format!("input device not found: {name}"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string())?,
    };
    let resolved_name = device.name().unwrap_or_default();
    let config = device
        .default_input_config()
        .map_err(|e| format!("default_input_config: {e}"))?;

    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.clone().into();
    let in_rate = stream_config.sample_rate.0;
    let channels = stream_config.channels as usize;

    eprintln!(
        "[audio] device={:?} in_rate={} channels={} format={:?}",
        resolved_name, in_rate, channels, sample_format
    );

    let ratio = in_rate as f64 / TARGET_SAMPLE_RATE as f64;
    let resampler = Arc::new(Mutex::new(Resampler::new(ratio)));
    let err_fn = |e| eprintln!("[audio] stream error: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let buf = buffer.clone();
            let rs = resampler.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[f32], _| process_f32(data, channels, &buf, &rs),
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build_input_stream(f32): {e}"))?
        }
        cpal::SampleFormat::I16 => {
            let buf = buffer.clone();
            let rs = resampler.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        let floats: Vec<f32> =
                            data.iter().map(|s| *s as f32 / 32768.0).collect();
                        process_f32(&floats, channels, &buf, &rs)
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build_input_stream(i16): {e}"))?
        }
        cpal::SampleFormat::U16 => {
            let buf = buffer.clone();
            let rs = resampler.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        let floats: Vec<f32> = data
                            .iter()
                            .map(|s| (*s as f32 - 32768.0) / 32768.0)
                            .collect();
                        process_f32(&floats, channels, &buf, &rs)
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build_input_stream(u16): {e}"))?
        }
        other => return Err(format!("unsupported sample format: {other:?}")),
    };

    stream.play().map_err(|e| format!("stream.play: {e}"))?;
    Ok((stream, resolved_name))
}

struct Resampler {
    ratio: f64,
    pos: f64,
}

impl Resampler {
    fn new(ratio: f64) -> Self {
        Self { ratio, pos: 0.0 }
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<i16>) {
        if input.is_empty() {
            return;
        }
        while (self.pos as usize) < input.len() {
            let idx = self.pos as usize;
            let s = input[idx].clamp(-1.0, 1.0);
            out.push((s * 32767.0) as i16);
            self.pos += self.ratio;
        }
        self.pos -= input.len() as f64;
    }
}

fn process_f32(
    data: &[f32],
    channels: usize,
    buffer: &Arc<Mutex<RingBuffer>>,
    resampler: &Arc<Mutex<Resampler>>,
) {
    if channels == 0 || data.is_empty() {
        return;
    }
    let mono: Vec<f32> = if channels == 1 {
        data.to_vec()
    } else {
        data.chunks_exact(channels)
            .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
            .collect()
    };

    let mut out = Vec::with_capacity(mono.len() / 2 + 8);
    {
        let mut rs = resampler.lock().unwrap();
        rs.process(&mono, &mut out);
    }
    if !out.is_empty() {
        let mut buf = buffer.lock().unwrap();
        buf.extend_from(&out);
    }
}

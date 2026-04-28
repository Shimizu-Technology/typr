use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::logger;
use crate::mic_permission;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MicDevice {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputStatus {
    pub input_device_count: usize,
    pub has_default_input: bool,
}

pub fn list_microphones() -> Vec<MicDevice> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(name) = device.name() {
                devices.push(MicDevice {
                    is_default: name == default_name,
                    name,
                });
            }
        }
    }
    devices
}

pub fn audio_input_status() -> AudioInputStatus {
    let host = cpal::default_host();
    let has_default_input = host.default_input_device().is_some();
    let input_device_count = host
        .input_devices()
        .map(|devices| devices.count())
        .unwrap_or_default();

    AudioInputStatus {
        input_device_count,
        has_default_input,
    }
}

/// Wrapper to make cpal::Stream usable across threads.
/// SAFETY: cpal::Stream on macOS (CoreAudio) is thread-safe in practice;
/// we only access it behind a Mutex to start/stop recording.
struct SendStream(#[allow(dead_code)] cpal::Stream);
unsafe impl Send for SendStream {}
unsafe impl Sync for SendStream {}

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<SendStream>,
    source_device_name: String,
    source_sample_format: String,
    source_sample_rate: u32,
    source_channels: u16,
}

#[derive(Debug, Clone)]
pub struct AudioCaptureSummary {
    pub path: PathBuf,
    pub device_name: String,
    pub sample_format: String,
    pub duration_seconds: f32,
    pub rms: f32,
    pub peak: f32,
    pub raw_sample_count: usize,
    pub resampled_sample_count: usize,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            source_device_name: "unknown".to_string(),
            source_sample_format: "unknown".to_string(),
            source_sample_rate: 48000,
            source_channels: 1,
        }
    }

    pub fn start(&mut self, mic_name: &str) -> Result<(), String> {
        mic_permission::ensure_microphone_permission()?;

        // Clear any leftover samples from previous recording
        self.samples.lock().unwrap().clear();

        let host = cpal::default_host();

        let normalized_mic_name = mic_name.trim();

        let default_device_name = host
            .default_input_device()
            .and_then(|device| device.name().ok())
            .unwrap_or_else(|| "unknown".to_string());

        let device = if normalized_mic_name.is_empty() || normalized_mic_name == "default" {
            host.default_input_device()
                .ok_or("No default input device found")?
        } else {
            host.input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().map(|n| n == normalized_mic_name).unwrap_or(false))
                .ok_or(format!("Microphone '{}' not found", normalized_mic_name))?
        };
        let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());

        let default_config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        let sample_format = default_config.sample_format();
        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels();

        logger::info(&format!(
            "[Typr] Selected microphone '{}' (requested '{}', macOS default '{}')",
            device_name,
            if normalized_mic_name.is_empty() {
                "default"
            } else {
                normalized_mic_name
            },
            default_device_name
        ));
        logger::info(&format!(
            "[Typr] Mic config: {}Hz, {} channels, format {:?}",
            sample_rate, channels, sample_format
        ));

        self.source_device_name = device_name;
        self.source_sample_format = format!("{:?}", sample_format);
        self.source_sample_rate = sample_rate;
        self.source_channels = channels;

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let samples = self.samples.clone();
        let stream = build_input_stream(&device, &config, sample_format, samples)?;

        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(SendStream(stream));
        logger::info("[Typr] Audio recording started");
        Ok(())
    }

    pub fn stop_and_save(&mut self, output_path: &PathBuf) -> Result<AudioCaptureSummary, String> {
        self.stream = None; // Drop stops the stream
        logger::info("[Typr] Audio recording stopped");

        let samples = self.samples.lock().unwrap();
        if samples.is_empty() {
            return Err("No audio captured".to_string());
        }

        let raw_sample_count = samples.len();
        logger::info(&format!("[Typr] Captured {} raw samples", raw_sample_count));

        // Convert to mono if multi-channel
        let mono: Vec<f32> = if self.source_channels > 1 {
            samples
                .chunks(self.source_channels as usize)
                .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
                .collect()
        } else {
            samples.clone()
        };

        // Downsample to 16kHz for whisper.cpp
        let resampled = resample(&mono, self.source_sample_rate, 16000);
        let duration_seconds = resampled.len() as f32 / 16000.0;
        let rms = calculate_rms(&resampled);
        let peak = calculate_peak(&resampled);
        logger::info(&format!(
            "[Typr] Resampled to {} samples at 16kHz; duration={:.2}s rms={:.6} peak={:.6}",
            resampled.len(),
            duration_seconds,
            rms,
            peak
        ));

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = WavWriter::create(output_path, spec).map_err(|e| e.to_string())?;
        for &sample in resampled.iter() {
            let amplitude = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(amplitude).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;

        drop(samples);
        self.samples.lock().unwrap().clear();

        logger::info(&format!("[Typr] WAV saved to {:?}", output_path));
        Ok(AudioCaptureSummary {
            path: output_path.clone(),
            device_name: self.source_device_name.clone(),
            sample_format: self.source_sample_format.clone(),
            duration_seconds,
            rms,
            peak,
            raw_sample_count,
            resampled_sample_count: resampled.len(),
        })
    }
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_squares = samples
        .iter()
        .map(|sample| {
            let sample = sample.clamp(-1.0, 1.0);
            sample * sample
        })
        .sum::<f32>();
    (sum_squares / samples.len() as f32).sqrt()
}

fn calculate_peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max)
}

/// Simple linear interpolation resampler
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < samples.len() {
            samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac
        } else {
            samples[idx.min(samples.len() - 1)] as f64
        };

        output.push(sample as f32);
    }

    output
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    samples: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String> {
    let err_fn = |err| {
        eprintln!("[Typr] Audio stream error: {}", err);
    };

    match sample_format {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    push_f32_samples(&samples, data.iter().copied());
                },
                err_fn,
                None,
            )
            .map_err(|e| e.to_string()),
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    push_f32_samples(
                        &samples,
                        data.iter().map(|sample| *sample as f32 / i16::MAX as f32),
                    );
                },
                err_fn,
                None,
            )
            .map_err(|e| e.to_string()),
        cpal::SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    push_f32_samples(
                        &samples,
                        data.iter()
                            .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0),
                    );
                },
                err_fn,
                None,
            )
            .map_err(|e| e.to_string()),
        other => Err(format!("Unsupported audio sample format: {:?}", other)),
    }
}

fn push_f32_samples<I>(samples: &Arc<Mutex<Vec<f32>>>, input: I)
where
    I: Iterator<Item = f32>,
{
    let mut buf = samples.lock().unwrap();
    buf.extend(input);
}

//! Captures one audio source (a microphone, or the system output via a CoreAudio
//! process tap) and delivers it as 30 ms mono frames at 16 kHz.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SizedSample};

use super::device::{describe, find_input_device};
use super::FrameResampler;
use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;

pub const FRAME_MS: u64 = 30;

#[derive(Clone, Debug)]
pub enum CaptureSource {
    /// An input device by name; `None` = system default input.
    Input(Option<String>),
    /// Everything the Mac plays through its default output (macOS 14.6+).
    SystemOutput,
}

pub struct AudioCapture {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    pub device_name: String,
}

impl AudioCapture {
    /// Opens the source and starts calling `on_frame` from a worker thread.
    /// Returns once the stream is actually running, or with the error that prevented it.
    pub fn start<F>(source: CaptureSource, on_frame: F) -> Result<Self>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();

        let stop_worker = stop.clone();
        let handle = std::thread::spawn(move || {
            run_worker(source, on_frame, stop_worker, ready_tx);
        });

        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(device_name)) => Ok(Self {
                stop,
                handle: Some(handle),
                device_name,
            }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(anyhow!(e))
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err(anyhow!("Timed out opening audio device"))
            }
        }
    }

    /// Stops the stream and waits until the last frame has been delivered.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run_worker<F>(
    source: CaptureSource,
    mut on_frame: F,
    stop: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<Result<String, String>>,
) where
    F: FnMut(&[f32]) + Send + 'static,
{
    let (sample_tx, sample_rx) = mpsc::channel::<Vec<f32>>();

    let opened = open_stream(&source, sample_tx);
    let (stream, sample_rate, name) = match opened {
        Ok(v) => v,
        Err(e) => {
            let _ = ready_tx.send(Err(e.to_string()));
            return;
        }
    };
    eprintln!("[capture] {:?} -> '{}' @ {} Hz", source, name, sample_rate);
    let _ = ready_tx.send(Ok(name));

    let mut resampler = FrameResampler::new(
        sample_rate as usize,
        WHISPER_SAMPLE_RATE as usize,
        Duration::from_millis(FRAME_MS),
    );

    while !stop.load(Ordering::Relaxed) {
        match sample_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(buf) => resampler.push(&buf, &mut on_frame),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    drop(stream);
    while let Ok(buf) = sample_rx.try_recv() {
        resampler.push(&buf, &mut on_frame);
    }
    resampler.finish(&mut on_frame);
}

fn open_stream(
    source: &CaptureSource,
    sample_tx: mpsc::Sender<Vec<f32>>,
) -> Result<(cpal::Stream, u32, String)> {
    let host = crate::audio_toolkit::get_cpal_host();

    let (device, config) = match source {
        CaptureSource::Input(name) => {
            let device = name
                .as_deref()
                .and_then(find_input_device)
                .or_else(|| host.default_input_device())
                .ok_or_else(|| anyhow!("No input device found"))?;
            let config = preferred_input_config(&device)?;
            (device, config)
        }
        CaptureSource::SystemOutput => {
            let device = host
                .default_output_device()
                .ok_or_else(|| anyhow!("No output device found"))?;
            // Building an *input* stream on an output device makes cpal create a
            // CoreAudio process tap + private aggregate device (loopback recording).
            let config = device.default_output_config()?;
            (device, config)
        }
    };

    let name = describe(&device);
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    let stream_config = config.config();

    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => build::<i8>(&device, stream_config, sample_tx, channels)?,
        cpal::SampleFormat::I16 => build::<i16>(&device, stream_config, sample_tx, channels)?,
        cpal::SampleFormat::I32 => build::<i32>(&device, stream_config, sample_tx, channels)?,
        cpal::SampleFormat::U8 => build::<u8>(&device, stream_config, sample_tx, channels)?,
        cpal::SampleFormat::F32 => build::<f32>(&device, stream_config, sample_tx, channels)?,
        other => return Err(anyhow!("Unsupported sample format {:?}", other)),
    };
    stream.play()?;

    Ok((stream, sample_rate, name))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    sample_tx: mpsc::Sender<Vec<f32>>,
    channels: usize,
) -> Result<cpal::Stream>
where
    T: Sample + SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let stream = device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mono: Vec<f32> = if channels == 1 {
                data.iter().map(|s| s.to_sample::<f32>()).collect()
            } else {
                data.chunks_exact(channels)
                    .map(|f| f.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32)
                    .collect()
            };
            let _ = sample_tx.send(mono);
        },
        |err| eprintln!("[capture] stream error: {}", err),
        None,
    )?;
    Ok(stream)
}

fn preferred_input_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig> {
    // Capture at 16 kHz directly when the device supports it, avoiding a resample.
    for range in device.supported_input_configs()? {
        if range.min_sample_rate() <= WHISPER_SAMPLE_RATE
            && range.max_sample_rate() >= WHISPER_SAMPLE_RATE
        {
            return Ok(range.with_sample_rate(WHISPER_SAMPLE_RATE));
        }
    }
    Ok(device.default_input_config()?)
}

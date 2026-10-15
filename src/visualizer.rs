//! Visualizador de espectro: captura el monitor del sink por defecto de
//! PipeWire/PulseAudio y calcula bandas log-espaciadas con FFT.
//!
//! Solo Linux: en otros SO la captura no está implementada y la app muestra
//! un aviso. Alcance MVP: el monitor captura TODO el audio del sistema,
//! no solo el de ShellMusic.

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use rustfft::Fft;
use rustfft::num_complex::Complex32;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

pub const SAMPLE_RATE: u32 = 44100;
pub const FFT_SIZE: usize = 2048;
const LOW_HZ: f32 = 30.0;
const HIGH_HZ: f32 = 16_000.0;
const DB_FLOOR: f32 = -55.0;
/// Subida rápida, caída lenta: barras vivas pero sin parpadeo nervioso.
const ATTACK: f32 = 0.6;
const RELEASE: f32 = 0.15;

/// Rangos de bins FFT por banda, log-espaciados entre LOW_HZ y HIGH_HZ.
pub fn band_ranges(bands: usize, fft_size: usize, sample_rate: u32) -> Vec<(usize, usize)> {
    let max_bin = fft_size / 2;
    let hz_per_bin = sample_rate as f32 / fft_size as f32;
    let ratio = HIGH_HZ / LOW_HZ;
    (0..bands)
        .map(|i| {
            let lo_hz = LOW_HZ * ratio.powf(i as f32 / bands as f32);
            let hi_hz = LOW_HZ * ratio.powf((i + 1) as f32 / bands as f32);
            let lo = ((lo_hz / hz_per_bin) as usize).min(max_bin - 1);
            // Cada banda cubre al menos un bin y no se solapa con la siguiente.
            let hi = ((hi_hz / hz_per_bin) as usize).max(lo + 1).min(max_bin);
            (lo, hi)
        })
        .collect()
}

pub fn hann(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            let t = i as f32 / (size - 1) as f32;
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * t).cos())
        })
        .collect()
}

pub struct SpectrumProcessor {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    pending: Vec<i16>,
    smoothed: Vec<f32>,
    ranges: Vec<(usize, usize)>,
}

impl SpectrumProcessor {
    pub fn new(bands: usize) -> Self {
        Self {
            fft: rustfft::FftPlanner::new().plan_fft_forward(FFT_SIZE),
            window: hann(FFT_SIZE),
            pending: Vec::with_capacity(FFT_SIZE * 2),
            smoothed: vec![0.0; bands],
            ranges: band_ranges(bands, FFT_SIZE, SAMPLE_RATE),
        }
    }

    /// Acumula muestras y, cuando completa una ventana, devuelve las bandas
    /// suavizadas (valores 0.0..=1.0).
    pub fn push_samples(&mut self, samples: &[i16]) -> Option<&[f32]> {
        self.pending.extend_from_slice(samples);
        if self.pending.len() < FFT_SIZE {
            return None;
        }
        let mut frame: Vec<Complex32> = self
            .pending
            .drain(..FFT_SIZE)
            .zip(&self.window)
            .map(|(s, w)| Complex32::new(f32::from(s) * w, 0.0))
            .collect();
        self.fft.process(&mut frame);
        // Escala de referencia: seno a plena escala con Hann → pico ≈ N/4 * 32767.
        let reference = FFT_SIZE as f32 / 4.0 * 32767.0;
        for ((lo, hi), out) in self.ranges.iter().zip(&mut self.smoothed) {
            let magnitude = frame[*lo..*hi]
                .iter()
                .map(|c| c.norm())
                .sum::<f32>()
                / (hi - lo) as f32;
            let db = 20.0 * (magnitude / reference).max(1e-9).log10();
            let level = ((db - DB_FLOOR) / -DB_FLOOR).clamp(0.0, 1.0);
            let k = if level > *out { ATTACK } else { RELEASE };
            *out += (level - *out) * k;
        }
        Some(&self.smoothed)
    }
}

/// Visualizador activo: subproceso de captura + tarea DSP. Al hacer Drop se
/// mata el subproceso; sin visualizador no corre nada.
pub struct Visualizer {
    bands: Arc<Mutex<Vec<f32>>>,
    alive: Arc<AtomicBool>,
    child: Child,
}

impl Visualizer {
    pub fn start(bands: usize) -> Result<Self> {
        let (mut command, name) = capture_command()?;
        command.kill_on_drop(true);
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("no se pudo lanzar {name}"))?;
        let stdout = child.stdout.take().context("sin stdout de captura")?;

        let shared = Arc::new(Mutex::new(vec![0.0; bands]));
        let alive = Arc::new(AtomicBool::new(true));
        tokio::spawn(capture_task(
            stdout,
            bands,
            Arc::clone(&shared),
            Arc::clone(&alive),
        ));
        Ok(Self {
            bands: shared,
            alive,
            child,
        })
    }

    /// Último frame de bandas (0.0..=1.0 por banda).
    pub fn bands(&self) -> Vec<f32> {
        self.bands
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    /// True mientras la captura siga viva (false si parec/pw-cat murió).
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn capture_command() -> Result<(Command, &'static str)> {
    for (binary, args) in [
        (
            "parec",
            vec![
                "--device=@DEFAULT_MONITOR@",
                "--format=s16le",
                "--rate=44100",
                "--channels=1",
            ],
        ),
        (
            "pw-cat",
            vec![
                "--record",
                "--target=@DEFAULT_MONITOR@",
                "--rate=44100",
                "--channels=1",
                "--format=s16",
                "-",
            ],
        ),
    ] {
        if binary_available(binary) {
            let mut command = Command::new(binary);
            command.args(args);
            return Ok((command, binary));
        }
    }
    bail!("ni parec ni pw-cat están disponibles")
}

fn binary_available(binary: &str) -> bool {
    std::process::Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn capture_task(
    mut stdout: tokio::process::ChildStdout,
    bands: usize,
    shared: Arc<Mutex<Vec<f32>>>,
    alive: Arc<AtomicBool>,
) {
    let mut processor = SpectrumProcessor::new(bands);
    let mut bytes = vec![0u8; 8192];
    loop {
        match stdout.read(&mut bytes).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let samples: Vec<i16> = bytes[..n]
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect();
                if let Some(frame) = processor.push_samples(&samples) {
                    *shared.lock().unwrap_or_else(|err| err.into_inner()) = frame.to_vec();
                }
            }
        }
    }
    alive.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amplitude: f32, frames: usize) -> Vec<i16> {
        (0..frames)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                (amplitude * 32767.0 * (2.0 * std::f32::consts::PI * freq * t).sin()) as i16
            })
            .collect()
    }

    #[test]
    fn band_ranges_are_log_spaced_and_cover_spectrum() {
        let ranges = band_ranges(24, FFT_SIZE, SAMPLE_RATE);
        assert_eq!(ranges.len(), 24);
        let bin_hz = SAMPLE_RATE as f32 / FFT_SIZE as f32;
        // Primera banda empieza cerca de 30 Hz, última termina cerca de 16 kHz.
        assert!((ranges[0].0 as f32 * bin_hz - LOW_HZ).abs() < bin_hz);
        assert!((ranges[23].1 as f32 * bin_hz - HIGH_HZ).abs() < 2.0 * bin_hz);
        // Monótonas y sin huecos grandes.
        for pair in ranges.windows(2) {
            assert!(pair[1].0 >= pair[0].0);
            assert!(pair[1].1 > pair[1].0);
        }
    }

    #[test]
    fn sine_440_dominates_its_band() {
        let bands = 24;
        let mut processor = SpectrumProcessor::new(bands);
        let samples = sine(440.0, 0.5, FFT_SIZE * 4);
        let mut last = None;
        for chunk in samples.chunks(FFT_SIZE) {
            if let Some(frame) = processor.push_samples(chunk) {
                last = Some(frame.to_vec());
            }
        }
        let frame = last.expect("debe producir al menos un frame");
        let dominant = frame
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let ranges = band_ranges(bands, FFT_SIZE, SAMPLE_RATE);
        let bin_hz = SAMPLE_RATE as f32 / FFT_SIZE as f32;
        let (lo, hi) = ranges[dominant];
        let (lo_hz, hi_hz) = (lo as f32 * bin_hz, hi as f32 * bin_hz);
        assert!(
            lo_hz <= 440.0 && 440.0 <= hi_hz,
            "440 Hz debe caer en la banda dominante {dominant} ({lo_hz:.0}..{hi_hz:.0} Hz)"
        );
        assert!(frame[dominant] > 0.5, "nivel alto: {}", frame[dominant]);
    }

    #[test]
    fn silence_stays_flat_and_smoothing_decays() {
        let bands = 24;
        let mut processor = SpectrumProcessor::new(bands);
        // Primero energía, luego silencio: las bandas deben decaer, no caer de golpe.
        let loud = sine(1000.0, 0.8, FFT_SIZE);
        processor.push_samples(&loud);
        let level = processor.push_samples(&loud).unwrap().to_vec();
        let peak = level.iter().cloned().fold(0.0f32, f32::max);
        assert!(peak > 0.5);
        let silence = vec![0i16; FFT_SIZE];
        let after1 = processor.push_samples(&silence).unwrap().to_vec();
        let max1 = after1.iter().cloned().fold(0.0f32, f32::max);
        assert!(max1 < peak, "debe decaer: {max1} < {peak}");
        assert!(max1 > 0.0, "pero con caída suave, no instantánea");
        for _ in 0..40 {
            processor.push_samples(&silence);
        }
        let settled = processor.push_samples(&silence).unwrap().to_vec();
        let max_settled = settled.iter().cloned().fold(0.0f32, f32::max);
        assert!(max_settled < 0.02, "silencio sostenido → ~0: {max_settled}");
    }
}

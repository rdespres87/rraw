#![forbid(unsafe_code)]

//! Multi-threaded batch export queue.

use crate::error::ExportError;
use crate::ExportFormat;
use byteorder::{BigEndian, ByteOrder};
use image::ImageEncoder;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A single image export task queued for processing.
#[derive(Debug, Clone)]
pub struct BatchExportJob {
    pub output_path: PathBuf,
    pub width: usize,
    pub height: usize,
    pub format: ExportFormat,
    pub rgb_linear: Vec<f32>,
    pub jpeg_quality: u8,
}

impl BatchExportJob {
    /// Creates a new export job with default JPEG quality (90).
    #[must_use]
    pub fn new(
        output_path: PathBuf,
        width: usize,
        height: usize,
        format: ExportFormat,
        rgb_linear: Vec<f32>,
    ) -> Self {
        Self {
            output_path,
            width,
            height,
            format,
            rgb_linear,
            jpeg_quality: 90,
        }
    }

    /// Sets the JPEG compression quality (1-100).
    #[must_use]
    pub fn with_jpeg_quality(mut self, quality: u8) -> Self {
        self.jpeg_quality = quality.clamp(1, 100);
        self
    }
}

/// Thread-safe batch export queue executing across thread pools.
#[derive(Debug, Default)]
pub struct BatchExportQueue {
    jobs: Arc<Mutex<Vec<BatchExportJob>>>,
}

impl BatchExportQueue {
    /// Initializes an empty batch export queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Enqueues an export job.
    pub fn push_job(&self, job: BatchExportJob) {
        if let Ok(mut guard) = self.jobs.lock() {
            guard.push(job);
        }
    }

    /// Returns the number of currently queued jobs.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.jobs.lock().map_or(0, |g| g.len())
    }

    /// Executes all queued export jobs across `max_threads` parallel worker threads.
    pub fn execute_all(&self, max_threads: usize) -> Vec<(PathBuf, Result<(), ExportError>)> {
        let queued_jobs = match self.jobs.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => return Vec::new(),
        };

        if queued_jobs.is_empty() {
            return Vec::new();
        }

        let num_threads = max_threads.clamp(1, 16);
        let jobs_arc = Arc::new(Mutex::new(queued_jobs));
        let results = Arc::new(Mutex::new(Vec::new()));

        std::thread::scope(|s| {
            for _ in 0..num_threads {
                let j_clone = Arc::clone(&jobs_arc);
                let r_clone = Arc::clone(&results);

                s.spawn(move || loop {
                    let maybe_job = {
                        let mut guard = match j_clone.lock() {
                            Ok(g) => g,
                            Err(_) => break,
                        };
                        guard.pop()
                    };

                    let job = match maybe_job {
                        Some(j) => j,
                        None => break,
                    };

                    let path = job.output_path.clone();
                    let outcome = Self::process_single_job(&job);

                    if let Ok(mut r_guard) = r_clone.lock() {
                        r_guard.push((path, outcome));
                    }
                });
            }
        });

        let out = match results.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => Vec::new(),
        };
        out
    }

    fn process_single_job(job: &BatchExportJob) -> Result<(), ExportError> {
        let count = job.width.saturating_mul(job.height);
        let expected = count.saturating_mul(3);
        if job.rgb_linear.len() != expected {
            return Err(ExportError::BufferLengthMismatch {
                expected,
                actual: job.rgb_linear.len(),
            });
        }

        if let Some(parent) = job.output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match job.format {
            ExportFormat::Png16 => {
                let file = File::create(&job.output_path)?;
                let w = BufWriter::new(file);

                let mut encoder = png::Encoder::new(w, job.width as u32, job.height as u32);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Sixteen);

                let mut writer = encoder
                    .write_header()
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;

                let mut u16_bytes = vec![0_u8; count * 3 * 2];
                for i in 0..(count * 3) {
                    let v = job.rgb_linear[i].clamp(0.0, 1.0);
                    let val_u16 = (v * 65535.0).round() as u16;
                    BigEndian::write_u16(&mut u16_bytes[i * 2..i * 2 + 2], val_u16);
                }

                writer
                    .write_image_data(&u16_bytes)
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;
                Ok(())
            }
            ExportFormat::Png8 => {
                let file = File::create(&job.output_path)?;
                let w = BufWriter::new(file);

                let mut encoder = png::Encoder::new(w, job.width as u32, job.height as u32);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);

                let mut writer = encoder
                    .write_header()
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;

                let u8_bytes: Vec<u8> = job
                    .rgb_linear
                    .iter()
                    .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    .collect();

                writer
                    .write_image_data(&u8_bytes)
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;
                Ok(())
            }
            ExportFormat::Jpeg | ExportFormat::Jpeg8Chroma444 => {
                let file = File::create(&job.output_path)?;
                let mut w = BufWriter::new(file);

                let u8_bytes: Vec<u8> = job
                    .rgb_linear
                    .iter()
                    .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    .collect();

                let quality = job.jpeg_quality.clamp(1, 100);
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, quality)
                    .write_image(
                        &u8_bytes,
                        job.width as u32,
                        job.height as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;
                Ok(())
            }
            ExportFormat::Tiff16 => {
                let file = File::create(&job.output_path)?;
                let mut w = BufWriter::new(file);
                let u16_samples: Vec<u16> = job
                    .rgb_linear
                    .iter()
                    .map(|&v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
                    .collect();
                image::codecs::tiff::TiffEncoder::new(&mut w)
                    .write_image(
                        bytemuck::cast_slice(&u16_samples),
                        job.width as u32,
                        job.height as u32,
                        image::ExtendedColorType::Rgb16,
                    )
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;
                Ok(())
            }
            ExportFormat::Tiff8 => {
                let file = File::create(&job.output_path)?;
                let mut w = BufWriter::new(file);
                let u8_samples: Vec<u8> = job
                    .rgb_linear
                    .iter()
                    .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    .collect();
                image::codecs::tiff::TiffEncoder::new(&mut w)
                    .write_image(
                        &u8_samples,
                        job.width as u32,
                        job.height as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;
                Ok(())
            }
            ExportFormat::Avif10 | ExportFormat::Avif12 => {
                use std::io::Write;
                let file = File::create(&job.output_path)?;
                let mut w = BufWriter::new(file);

                let u8_bytes: Vec<u8> = job
                    .rgb_linear
                    .iter()
                    .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    .collect();

                let quality = job.jpeg_quality.clamp(1, 100);
                let encoder =
                    image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut w, 10, quality);
                encoder
                    .write_image(
                        &u8_bytes,
                        job.width as u32,
                        job.height as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| ExportError::EncodingError(e.to_string()))?;
                w.flush()?;
                Ok(())
            }
            _ => Err(ExportError::UnsupportedFormat(format!(
                "{:?} is not supported for batch raster export",
                job.format
            ))),
        }
    }
}

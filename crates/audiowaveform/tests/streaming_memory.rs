//! Runs in its own test binary so heap measurements exclude other test suites.
#![cfg(feature = "format-wav")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicUsize, Ordering};

use audiowaveform::{
    AudioFormat, GenerateOptions, RawAudioConfig, RawSampleFormat, ScaleSpec,
    generate_waveform_from_raw_reader, generate_waveform_from_reader,
};

struct TrackingAllocator;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn allocated(size: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forward the allocator's original layout to System.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            allocated(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: pointer and layout came from this allocator, which uses System.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: forward the original pointer/layout and requested size to System.
        let result = unsafe { System.realloc(ptr, layout, size) };
        if !result.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            allocated(size);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

/// Synthesizes a stereo WAV on demand without allocating its audio payload.
struct SilenceWav {
    header: [u8; 44],
    position: u64,
    length: u64,
}

impl SilenceWav {
    fn new(frames: u32) -> Self {
        let bytes = frames * 4;
        let mut header = [0; 44];
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&(36 + bytes).to_le_bytes());
        header[8..16].copy_from_slice(b"WAVEfmt ");
        header[16..20].copy_from_slice(&16_u32.to_le_bytes());
        header[20..22].copy_from_slice(&1_u16.to_le_bytes());
        header[22..24].copy_from_slice(&2_u16.to_le_bytes());
        header[24..28].copy_from_slice(&48_000_u32.to_le_bytes());
        header[28..32].copy_from_slice(&192_000_u32.to_le_bytes());
        header[32..34].copy_from_slice(&4_u16.to_le_bytes());
        header[34..36].copy_from_slice(&16_u16.to_le_bytes());
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&bytes.to_le_bytes());
        Self {
            header,
            position: 0,
            length: u64::from(bytes) + 44,
        }
    }
}

impl Read for SilenceWav {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output
            .len()
            .min(self.length.saturating_sub(self.position) as usize);
        output[..count].fill(0);
        if self.position < 44 {
            let header_count = count.min(44 - self.position as usize);
            output[..header_count].copy_from_slice(
                &self.header[self.position as usize..self.position as usize + header_count],
            );
        }
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for SilenceWav {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let position = match from {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.length) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        self.position = u64::try_from(position)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        Ok(self.position)
    }
}

#[test]
fn decoded_pcm_memory_does_not_grow_with_recording_length() {
    // Known-size output needs one allocation plus the two extrema buffers,
    // independent of point count, channel mixing, or identity scaling.
    for (channels, split_channels) in [(1, false), (2, false), (2, true)] {
        let pcm = audiowaveform::PcmAudio::new(
            48_000,
            channels,
            vec![1000; 1_000_000 * usize::from(channels)],
        )
        .unwrap();
        for (scale, points) in [
            (ScaleSpec::Points(110), 110),
            (ScaleSpec::SamplesPerPixel(3), 333_334),
        ] {
            for amplitude_scale in [None, Some(audiowaveform::AmplitudeScale::Fixed(1.0))] {
                let baseline = LIVE.load(Ordering::Relaxed);
                let allocations = ALLOCATIONS.load(Ordering::Relaxed);
                PEAK.store(baseline, Ordering::Relaxed);
                let waveform = audiowaveform::generate_waveform_from_pcm(
                    &pcm,
                    &GenerateOptions {
                        scale,
                        split_channels,
                        amplitude_scale,
                    },
                )
                .unwrap();
                let count = ALLOCATIONS.load(Ordering::Relaxed) - allocations;
                let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
                assert_eq!(waveform.len(), points);
                assert!(count <= 3, "waveform output reallocated: {count}");
                assert!(
                    peak <= waveform.allocated_bytes() + 1024,
                    "output was cloned: {peak}"
                );
            }
        }
    }

    for raw in [false, true] {
        for exact in [true, false] {
            let mut peaks = Vec::new();
            for frames in [100_000, 10_000_000] {
                let baseline = LIVE.load(Ordering::Relaxed);
                PEAK.store(baseline, Ordering::Relaxed);
                let options = GenerateOptions {
                    scale: if exact {
                        ScaleSpec::Points(110)
                    } else {
                        ScaleSpec::SamplesPerPixel(frames / 110)
                    },
                    ..Default::default()
                };
                let waveform = if raw {
                    generate_waveform_from_raw_reader(
                        std::io::repeat(0).take(u64::from(frames) * 4),
                        &RawAudioConfig::new(48_000, 2, RawSampleFormat::S16Le).unwrap(),
                        &options,
                    )
                } else {
                    generate_waveform_from_reader(
                        SilenceWav::new(frames),
                        Some(AudioFormat::Wav),
                        &options,
                    )
                }
                .unwrap();
                let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
                assert_eq!(waveform.len(), if exact { 110 } else { 111 });
                assert!(
                    waveform
                        .interleaved_samples()
                        .iter()
                        .all(|&value| value == 0)
                );
                peaks.push(peak);
            }
            eprintln!(
                "raw={raw}, exact={exact}: peak heap bytes short={}, long={}",
                peaks[0], peaks[1]
            );
            assert!(
                peaks[1] < 2_000_000,
                "decoded audio was retained: {peaks:?}"
            );
            assert!(
                peaks[1] <= peaks[0] + 256_000,
                "heap grew with duration: {peaks:?}"
            );
        }
    }
}

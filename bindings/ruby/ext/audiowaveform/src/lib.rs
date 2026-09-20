use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{ffi::c_void, ptr};

use audiowaveform_core::{
    AmplitudeScale, Error as CoreError, GenerateOptions, ScaleSpec, Waveform, WaveformFormat,
    generate_waveform_from_path,
};
use magnus::{
    DataTypeFunctions, Error, ExceptionClass, Module, Object, RString, Ruby, TypedData, function,
    method, rb_sys::protect,
};

#[derive(TypedData)]
#[magnus(class = "AudioWaveform::Waveform", free_immediately, size)]
struct RubyWaveform(Waveform);

impl DataTypeFunctions for RubyWaveform {
    fn size(&self) -> usize {
        std::mem::size_of_val(self) + self.0.allocated_bytes()
    }
}

struct NoGvlTask<F, T> {
    function: Option<F>,
    result: Option<T>,
    panicked: bool,
}

unsafe extern "C" fn call_without_gvl<F, T>(data: *mut c_void) -> *mut c_void
where
    F: FnOnce() -> T,
{
    // SAFETY: `data` points to a live `NoGvlTask` for the duration of the
    // synchronous `rb_thread_call_without_gvl` call, and Ruby cannot access it.
    let task = unsafe { &mut *data.cast::<NoGvlTask<F, T>>() };
    let Some(function) = task.function.take() else {
        return ptr::null_mut();
    };
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(result) => task.result = Some(result),
        Err(_) => task.panicked = true,
    }
    ptr::null_mut()
}

fn without_gvl<F, T>(ruby: &Ruby, function: F) -> Result<T, Error>
where
    F: FnOnce() -> T,
{
    let mut task = NoGvlTask {
        function: Some(function),
        result: None,
        panicked: false,
    };
    // Ruby can raise while checking interrupts before or after the callback.
    // Keep the task outside `protect` so its closure/result is dropped normally
    // even when Ruby exits the protected call with a non-local jump.
    protect(|| {
        // SAFETY: the callback only accesses the live stack-allocated task,
        // does not invoke Ruby methods, and catches Rust panics before they
        // cross the C ABI boundary. rb-sys tracks its allocations for Ruby GC.
        unsafe {
            rb_sys::rb_thread_call_without_gvl(
                Some(call_without_gvl::<F, T>),
                (&mut task as *mut NoGvlTask<F, T>).cast(),
                None,
                ptr::null_mut(),
            );
        }
        rb_sys::Qnil as rb_sys::VALUE
    })?;

    if task.panicked {
        Err(ruby_error(
            ruby,
            "native waveform operation failed unexpectedly",
        ))
    } else {
        task.result
            .ok_or_else(|| ruby_error(ruby, "native waveform operation did not complete"))
    }
}

impl RubyWaveform {
    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    fn samples_per_pixel(&self) -> u32 {
        self.0.samples_per_pixel()
    }

    fn channels(&self) -> u16 {
        self.0.channels()
    }

    fn storage_bits(&self) -> u8 {
        self.0.storage_bits()
    }

    fn length(&self) -> usize {
        self.0.len()
    }

    fn empty(&self) -> bool {
        self.0.is_empty()
    }

    fn duration(&self) -> f64 {
        self.0.duration_seconds()
    }

    fn data(ruby: &Ruby, waveform: &Self, bits: u8) -> Result<Vec<i16>, Error> {
        without_gvl(ruby, || waveform.0.data(bits))?.map_err(|error| core_error(ruby, error))
    }

    fn point(&self, channel: u16, index: usize) -> Option<(i16, i16)> {
        self.0
            .point(channel, index)
            .map(|point| (point.min, point.max))
    }

    fn save(
        ruby: &Ruby,
        waveform: &Self,
        path: String,
        format: String,
        bits: u8,
    ) -> Result<(), Error> {
        let format = parse_format(ruby, &format)?;
        let result = without_gvl(ruby, || {
            waveform.0.write_to_path(path, Some(format), Some(bits))
        })?;
        result.map_err(|error| core_error(ruby, error))
    }

    fn serialize(ruby: &Ruby, waveform: &Self, format: String, bits: u8) -> Result<RString, Error> {
        let format = parse_format(ruby, &format)?;
        let result = without_gvl(ruby, || {
            let mut bytes = Vec::new();
            waveform
                .0
                .write_to_writer(&mut bytes, format, Some(bits))
                .map(|()| bytes)
        })?;
        let bytes = result.map_err(|error| core_error(ruby, error))?;
        Ok(ruby.str_from_slice(&bytes))
    }
}

fn generate(
    ruby: &Ruby,
    input: String,
    scale_kind: String,
    scale_value: u32,
    split_channels: bool,
    amplitude_kind: String,
    amplitude_value: f64,
) -> Result<RubyWaveform, Error> {
    let scale = match scale_kind.as_str() {
        "samples_per_pixel" => ScaleSpec::SamplesPerPixel(scale_value),
        "pixels_per_second" => ScaleSpec::PixelsPerSecond(scale_value),
        "points" => ScaleSpec::Points(scale_value),
        _ => return Err(argument_error(ruby, "unsupported waveform scale")),
    };
    let amplitude_scale = match amplitude_kind.as_str() {
        "none" => None,
        "auto" => Some(AmplitudeScale::Auto),
        "fixed" => Some(AmplitudeScale::Fixed(amplitude_value)),
        _ => return Err(argument_error(ruby, "unsupported amplitude scale")),
    };
    let options = GenerateOptions {
        scale,
        split_channels,
        amplitude_scale,
    };

    without_gvl(ruby, || generate_waveform_from_path(input, &options))?
        .map(RubyWaveform)
        .map_err(|error| core_error(ruby, error))
}

fn parse_format(ruby: &Ruby, format: &str) -> Result<WaveformFormat, Error> {
    format
        .parse()
        .map_err(|error: CoreError| core_error(ruby, error))
}

fn core_error(ruby: &Ruby, error: CoreError) -> Error {
    if matches!(error, CoreError::InvalidArgument { .. }) {
        argument_error(ruby, error.to_string())
    } else {
        ruby_error(ruby, error.to_string())
    }
}

fn argument_error(ruby: &Ruby, message: impl AsRef<str>) -> Error {
    Error::new(ruby.exception_arg_error(), message.as_ref().to_owned())
}

fn ruby_error(ruby: &Ruby, message: impl AsRef<str>) -> Error {
    let error_class = ruby
        .eval::<ExceptionClass>("AudioWaveform::Error")
        .unwrap_or_else(|_| ruby.exception_standard_error());
    Error::new(error_class, message.as_ref().to_owned())
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("AudioWaveform")?;
    module.define_error("Error", ruby.exception_standard_error())?;

    let native = module.define_module("Native")?;
    native.define_singleton_method("generate", function!(generate, 6))?;

    let waveform = module.define_class("Waveform", ruby.class_object())?;
    waveform.define_method("sample_rate", method!(RubyWaveform::sample_rate, 0))?;
    waveform.define_method(
        "samples_per_pixel",
        method!(RubyWaveform::samples_per_pixel, 0),
    )?;
    waveform.define_method("channels", method!(RubyWaveform::channels, 0))?;
    waveform.define_method("storage_bits", method!(RubyWaveform::storage_bits, 0))?;
    waveform.define_method("length", method!(RubyWaveform::length, 0))?;
    waveform.define_method("empty?", method!(RubyWaveform::empty, 0))?;
    waveform.define_method("duration", method!(RubyWaveform::duration, 0))?;
    waveform.define_private_method("__data", method!(RubyWaveform::data, 1))?;
    waveform.define_private_method("__point", method!(RubyWaveform::point, 2))?;
    waveform.define_private_method("__save", method!(RubyWaveform::save, 3))?;
    waveform.define_private_method("__serialize", method!(RubyWaveform::serialize, 2))?;
    Ok(())
}

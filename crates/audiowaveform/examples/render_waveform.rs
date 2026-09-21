use std::env;

use audiowaveform::{RenderOptions, Waveform, render_waveform_to_path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let input = args
        .next()
        .unwrap_or_else(|| "fixtures/test_file_stereo_8bit_64spp_wav.dat".to_string());
    let output = args.next().unwrap_or_else(|| "output.png".to_string());

    let waveform = Waveform::load_from_path(&input, None)?;
    render_waveform_to_path(&waveform, &RenderOptions::default(), output)?;

    Ok(())
}

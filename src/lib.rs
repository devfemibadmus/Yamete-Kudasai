use rodio::{Decoder, OutputStreamBuilder, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn play_file(path: &Path, volume: f32) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Sound file not found: {}", path.display()));
    }

    let mut stream = OutputStreamBuilder::open_default_stream()
        .map_err(|err| format!("No audio output stream available: {err}"))?;
    stream.log_on_drop(false);
    let sink = Sink::connect_new(stream.mixer());
    sink.set_volume(volume.clamp(0.0, 2.0));

    let file = File::open(path).map_err(|err| format!("Failed to open sound file: {err}"))?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|err| format!("Failed to decode audio: {err}"))?;

    sink.append(source);
    sink.sleep_until_end();
    Ok(())
}

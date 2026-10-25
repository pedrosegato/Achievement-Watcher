use std::path::{Path, PathBuf};

use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

pub fn duration_ms(path: &Path) -> Option<u64> {
    let decoder = Decoder::try_from(std::fs::File::open(path).ok()?).ok()?;
    if let Some(total) = decoder.total_duration() {
        return u64::try_from(total.as_millis()).ok();
    }
    let rate = u64::from(decoder.sample_rate().get());
    let channels = u64::from(decoder.channels().get());
    let samples = u64::try_from(decoder.count()).ok()?;
    Some(samples * 1000 / (rate * channels).max(1))
}

pub fn play(volume: f32, source: Option<PathBuf>) {
    let Some(source) = source else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("aw-sound".into())
        .spawn(move || {
            if let Err(error) = play_blocking(volume, &source) {
                tracing::warn!("sound failed: {error}");
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("sound thread failed: {error}");
    }
}

fn play_blocking(volume: f32, source: &Path) -> Result<(), String> {
    let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
    sink.log_on_drop(false);
    let player = Player::connect_new(sink.mixer());
    player.set_volume(volume.clamp(0.0, 1.0));
    append_file(&player, source)?;
    player.sleep_until_end();
    Ok(())
}

fn append_file(player: &Player, path: &Path) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    player.append(Decoder::try_from(file).map_err(|e| e.to_string())?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_of_builtin_sounds() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("sounds")
            .join("achievement-watcher");
        let unlock = duration_ms(&dir.join("unlock.mp3")).unwrap();
        let rare = duration_ms(&dir.join("rare.mp3")).unwrap();
        assert!((5300..5700).contains(&unlock), "{unlock}");
        assert!((9800..10_200).contains(&rare), "{rare}");
        assert_eq!(duration_ms(&dir.join("missing.mp3")), None);
    }

    #[test]
    fn builtin_mp3_sounds_decode() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("sounds");
        for file in [
            dir.join("achievement-watcher").join("unlock.mp3"),
            dir.join("achievement-watcher").join("rare.mp3"),
            dir.join("achievement-watcher").join("platinum.mp3"),
            dir.join("xbox").join("unlock.mp3"),
            dir.join("xbox").join("rare.mp3"),
            dir.join("xbox").join("platinum.mp3"),
        ] {
            let opened = std::fs::File::open(&file).unwrap();
            assert!(Decoder::try_from(opened).is_ok(), "{}", file.display());
        }
    }
}

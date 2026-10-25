use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::app::AppState;
use crate::loopback::{self, AudioFormat, Feed};
use crate::settings::{Settings, VideoQuality};
use crate::shot::{sanitize, unique_path};

const FFMPEG_ZIP: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
const FFMPEG_SHA: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip.sha256";
const SEGMENT_SECONDS: u32 = 1;
const SEGMENT_LIST: &str = "segments.csv";
const RING_SLACK: u32 = 3;
const SAVE_SLACK: Duration = Duration::from_secs(1);
const ENCODERS: [&str; 8] = [
    "av1_nvenc",
    "av1_amf",
    "hevc_nvenc",
    "hevc_amf",
    "hevc_qsv",
    "h264_nvenc",
    "h264_amf",
    "h264_qsv",
];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const COPY_BLOCK: usize = 64 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
const CANCELLED: &str = "cancelled";
pub const FFMPEG_PROGRESS: &str = "ffmpeg-progress";

pub struct CaptureOptions {
    pub fps: u32,
    pub quality: VideoQuality,
    pub wrap: u32,
}

impl CaptureOptions {
    pub fn from_settings(settings: &Settings) -> CaptureOptions {
        CaptureOptions {
            fps: settings.video_fps,
            quality: settings.video_quality,
            wrap: ring_size(settings.video_before, settings.video_after),
        }
    }
}

pub fn capture_changed(old: &Settings, new: &Settings) -> bool {
    let shape = |s: &Settings| (s.video_fps, s.video_quality, s.video_before, s.video_after);
    shape(old) != shape(new)
}

pub fn ring_size(before: u32, after: u32) -> u32 {
    before + after + RING_SLACK
}

struct Recorder {
    child: Child,
    dir: PathBuf,
    app_id: u32,
    encoder: &'static str,
    audio: Option<Feed>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub available: bool,
    pub path: Option<String>,
    pub downloading: bool,
    pub error: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Default)]
pub struct VideoState {
    recorder: Mutex<Option<Recorder>>,
    encoder: Mutex<Option<&'static str>>,
    download: Mutex<FfmpegStatus>,
    cancel: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn run(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg exited with {status}"))
    }
}

pub fn ffmpeg_path(state: &AppState) -> Option<PathBuf> {
    state
        .settings_snapshot()
        .ffmpeg_path
        .filter(|p| p.is_file())
        .or_else(|| Some(bundled_ffmpeg(state)).filter(|p| p.is_file()))
}

fn bundled_ffmpeg(state: &AppState) -> PathBuf {
    state.paths.data_dir.join("ffmpeg").join("ffmpeg.exe")
}

pub fn status(state: &AppState) -> FfmpegStatus {
    let mut status = lock(&state.video.download).clone();
    let path = ffmpeg_path(state);
    status.available = path.is_some();
    status.path = path.map(|p| p.display().to_string());
    status
}

pub fn pick_encoder(available: &str, works: impl Fn(&str) -> bool) -> &'static str {
    ENCODERS
        .into_iter()
        .find(|encoder| available.contains(encoder) && works(encoder))
        .unwrap_or("libx264")
}

fn encoder_works(ffmpeg: &Path, encoder: &str) -> bool {
    command(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x360:r=30",
            "-frames:v",
            "1",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn encoder(state: &AppState, ffmpeg: &Path) -> &'static str {
    let mut cached = lock(&state.video.encoder);
    if let Some(encoder) = *cached {
        return encoder;
    }
    let listed = command(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let picked = pick_encoder(&listed, |candidate| encoder_works(ffmpeg, candidate));
    tracing::info!("video encoder: {picked}");
    *cached = Some(picked);
    picked
}

pub fn prepare(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("aw-video-encoder".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            if let Some(ffmpeg) = ffmpeg_path(&state) {
                encoder(&state, &ffmpeg);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("encoder probe thread failed: {error}");
    }
}

pub fn capture_args(
    encoder: &str,
    segment_dir: &Path,
    options: &CaptureOptions,
    audio: Option<AudioFormat>,
) -> Vec<String> {
    let scale = match options.quality {
        VideoQuality::Light => ",scale=-2:'min(ih,720)'",
        VideoQuality::Balanced => ",scale=-2:'min(ih,1080)'",
        VideoQuality::High => "",
    };
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-y"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    if let Some(audio) = audio {
        args.extend([
            "-thread_queue_size".into(),
            "1024".into(),
            "-f".into(),
            "f32le".into(),
            "-ar".into(),
            audio.rate.to_string(),
            "-ac".into(),
            audio.channels.to_string(),
            "-i".into(),
            "pipe:0".into(),
        ]);
    }
    args.push("-filter_complex".into());
    args.push(format!(
        "ddagrab=output_idx=0:framerate={},hwdownload,format=bgra{scale}[v]",
        options.fps
    ));
    args.extend(["-map".into(), "[v]".into()]);
    if audio.is_some() {
        args.extend(["-map".into(), "0:a".into()]);
    }
    args.push("-c:v".into());
    args.push(encoder.into());
    args.extend(encoder_args(encoder, options.quality));
    args.extend([
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-g".into(),
        options.fps.to_string(),
    ]);
    if audio.is_some() {
        args.extend(
            ["-c:a", "aac", "-b:a", "160k", "-ac", "2"]
                .iter()
                .map(|s| (*s).to_owned()),
        );
    }
    args.extend(
        [
            "-f",
            "segment",
            "-segment_format",
            "matroska",
            "-segment_time",
        ]
        .iter()
        .map(|s| (*s).to_owned()),
    );
    args.push(SEGMENT_SECONDS.to_string());
    args.push("-segment_wrap".into());
    args.push(options.wrap.to_string());
    args.push("-reset_timestamps".into());
    args.push("1".into());
    args.push("-segment_list".into());
    args.push(segment_dir.join(SEGMENT_LIST).display().to_string());
    args.extend(["-segment_list_type".into(), "csv".into()]);
    args.push("-segment_list_size".into());
    args.push(options.wrap.to_string());
    args.push(segment_dir.join("seg%02d.mkv").display().to_string());
    args
}

fn encoder_args(encoder: &str, quality: VideoQuality) -> Vec<String> {
    let level = |high: u8, balanced: u8, light: u8| {
        match quality {
            VideoQuality::High => high,
            VideoQuality::Balanced => balanced,
            VideoQuality::Light => light,
        }
        .to_string()
    };
    let nvenc: &[&str] = &["-preset", "p5", "-tune", "hq", "-rc", "constqp"];
    let (fixed, flags, value): (&[&str], &[&str], String) = match encoder {
        "av1_nvenc" => (nvenc, &["-qp"], level(56, 64, 76)),
        "av1_amf" => (
            &["-usage", "transcoding", "-quality", "quality", "-rc", "cqp"],
            &["-qp_i", "-qp_p"],
            level(56, 64, 76),
        ),
        "hevc_nvenc" => (nvenc, &["-qp"], level(22, 24, 27)),
        "hevc_amf" => (
            &["-quality", "quality", "-rc", "cqp"],
            &["-qp_i", "-qp_p"],
            level(22, 24, 27),
        ),
        "hevc_qsv" => (
            &["-preset", "medium"],
            &["-global_quality"],
            level(22, 24, 27),
        ),
        "h264_nvenc" => (
            &["-preset", "p4", "-tune", "hq", "-rc", "constqp"],
            &["-qp"],
            level(20, 22, 25),
        ),
        "h264_amf" => (
            &["-quality", "balanced", "-rc", "cqp"],
            &["-qp_i", "-qp_p"],
            level(20, 22, 25),
        ),
        "h264_qsv" => (
            &["-preset", "medium"],
            &["-global_quality"],
            level(20, 22, 25),
        ),
        _ => (
            &["-preset", "ultrafast", "-tune", "zerolatency"],
            &["-crf"],
            level(20, 22, 25),
        ),
    };
    let mut args: Vec<String> = fixed.iter().map(|s| (*s).to_owned()).collect();
    for flag in flags {
        args.push((*flag).to_owned());
        args.push(value.clone());
    }
    args
}

pub fn save_args(list: &Path, target: &Path, encoder: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    args.push(list.display().to_string());
    args.extend(["-c:v".to_owned(), "copy".to_owned()]);
    if encoder.starts_with("hevc") {
        args.extend(["-tag:v".to_owned(), "hvc1".to_owned()]);
    }
    args.extend(
        ["-c:a", "aac", "-b:a", "160k", "-af", "aresample=async=1"]
            .iter()
            .map(|s| (*s).to_owned()),
    );
    args.extend(["-movflags".to_owned(), "+faststart".to_owned()]);
    args.push(target.display().to_string());
    args
}

pub fn clip_segments(files: &[(PathBuf, SystemTime)], since: SystemTime) -> Vec<PathBuf> {
    let mut recent: Vec<&(PathBuf, SystemTime)> = files
        .iter()
        .filter(|(_, modified)| *modified > since)
        .collect();
    recent.sort_by_key(|(_, modified)| *modified);
    recent.into_iter().map(|(path, _)| path.clone()).collect()
}

pub fn segment_durations(list: &str) -> HashMap<String, f64> {
    list.lines()
        .filter_map(|line| {
            let mut fields = line.split(',');
            let name = fields.next()?.trim();
            let start: f64 = fields.next()?.trim().parse().ok()?;
            let end: f64 = fields.next()?.trim().parse().ok()?;
            Some((name.to_owned(), end - start))
        })
        .collect()
}

pub fn concat_list(entries: &[(PathBuf, Option<f64>)]) -> String {
    entries
        .iter()
        .map(|(path, duration)| {
            let text = path
                .display()
                .to_string()
                .replace('\\', "/")
                .replace('\'', "'\\''");
            match duration {
                Some(seconds) => format!("file '{text}'\nduration {seconds:.6}\n"),
                None => format!("file '{text}'\n"),
            }
        })
        .collect()
}

pub fn start(app: &AppHandle, app_id: u32) {
    let state = app.state::<AppState>();
    if !state.settings_snapshot().video {
        return;
    }
    let Some(ffmpeg) = ffmpeg_path(&state) else {
        tracing::warn!("video is on but ffmpeg is not available");
        return;
    };
    let mut slot = lock(&state.video.recorder);
    if slot.is_some() {
        return;
    }
    let dir = std::env::temp_dir().join("achievement-watcher-video");
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let encoder = encoder(&state, &ffmpeg);
    let options = CaptureOptions::from_settings(&state.settings_snapshot());
    let format = loopback::format();
    match command(&ffmpeg)
        .args(capture_args(encoder, &dir, &options, format))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            let audio = match format {
                Some(format) => child
                    .stdin
                    .take()
                    .map(|stdin| loopback::start(format, stdin)),
                None => None,
            };
            let sound = if audio.is_some() { "with" } else { "without" };
            tracing::info!("recording replay buffer for {app_id} with {encoder}, {sound} audio");
            *slot = Some(Recorder {
                child,
                dir,
                app_id,
                encoder,
                audio,
            });
        }
        Err(error) => tracing::warn!("cannot start ffmpeg: {error}"),
    }
}

pub fn stop(app: &AppHandle, app_id: Option<u32>) {
    let state = app.state::<AppState>();
    let mut slot = lock(&state.video.recorder);
    if slot
        .as_ref()
        .is_some_and(|r| app_id.is_none_or(|id| id == r.app_id))
        && let Some(mut recorder) = slot.take()
    {
        if recorder.audio.take().is_none()
            && let Some(stdin) = recorder.child.stdin.as_mut()
        {
            let _ = stdin.write_all(b"q");
        }
        std::thread::sleep(Duration::from_millis(500));
        let _ = recorder.child.kill();
        let _ = recorder.child.wait();
    }
}

pub fn clip(app: &AppHandle, app_id: u32, game: String, achievement: String) {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    if !settings.video {
        return;
    }
    let Some(ffmpeg) = ffmpeg_path(&state) else {
        return;
    };
    let (dir, encoder) = match lock(&state.video.recorder).as_ref() {
        Some(recorder) if recorder.app_id == app_id => (recorder.dir.clone(), recorder.encoder),
        _ => return,
    };
    let root = settings.video_dir.clone().or_else(|| {
        app.path()
            .video_dir()
            .ok()
            .map(|d| crate::shot::default_capture_root(&d))
    });
    let Some(root) = root else {
        return;
    };
    let since = SystemTime::now() - Duration::from_secs(u64::from(settings.video_before));
    let wait = Duration::from_secs(u64::from(settings.video_after)) + SAVE_SLACK;
    let spawned = std::thread::Builder::new()
        .name("aw-video-clip".into())
        .spawn(move || {
            std::thread::sleep(wait);
            let clip = Clip {
                game: &game,
                achievement: &achievement,
                since,
                encoder,
            };
            if let Err(error) = save_clip(&ffmpeg, &dir, &root, &clip) {
                tracing::warn!("video clip failed: {error}");
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("video thread failed: {error}");
    }
}

struct Clip<'a> {
    game: &'a str,
    achievement: &'a str,
    since: SystemTime,
    encoder: &'a str,
}

fn save_clip(ffmpeg: &Path, dir: &Path, root: &Path, clip: &Clip) -> Result<(), String> {
    let files: Vec<(PathBuf, SystemTime)> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "mkv"))
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect();
    let segments = clip_segments(&files, clip.since);
    if segments.is_empty() {
        return Err("no recorded segments yet".into());
    }
    let durations = std::fs::read_to_string(dir.join(SEGMENT_LIST))
        .map(|list| segment_durations(&list))
        .unwrap_or_default();
    let entries: Vec<(PathBuf, Option<f64>)> = segments
        .into_iter()
        .map(|path| {
            let duration = path
                .file_name()
                .and_then(|name| durations.get(&*name.to_string_lossy()))
                .copied();
            (path, duration)
        })
        .collect();
    let list = dir.join("clip.txt");
    std::fs::write(&list, concat_list(&entries)).map_err(|e| e.to_string())?;
    let out_dir = root.join(sanitize(clip.game));
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let target = unique_path(&out_dir, &sanitize(clip.achievement), "mp4");
    run(command(ffmpeg).args(save_args(&list, &target, clip.encoder)))?;
    tracing::info!("video saved to {}", target.display());
    Ok(())
}

pub fn copy_with_progress(
    mut reader: impl Read,
    mut writer: impl Write,
    total: Option<u64>,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u64),
) -> Result<u64, String> {
    let mut buffer = vec![0u8; COPY_BLOCK];
    let mut copied: u64 = 0;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(CANCELLED.into());
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        writer
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
        copied += u64::try_from(read).unwrap_or(u64::MAX);
        on_progress(copied);
    }
    match total {
        Some(total) if total != copied => {
            Err(format!("download incomplete: {copied} of {total} bytes"))
        }
        _ => Ok(copied),
    }
}

pub fn cancel_download(state: &AppState) {
    state.video.cancel.store(true, Ordering::SeqCst);
}

fn report(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _ = app.emit(FFMPEG_PROGRESS, status(&state));
}

pub fn download(app: AppHandle) {
    {
        let state = app.state::<AppState>();
        let mut status = lock(&state.video.download);
        if status.downloading {
            return;
        }
        status.downloading = true;
        status.error = None;
        status.downloaded = 0;
        status.total = None;
        state.video.cancel.store(false, Ordering::SeqCst);
    }
    let spawned = std::thread::Builder::new()
        .name("aw-ffmpeg-download".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let mut last = Instant::now();
            let result = download_blocking(
                &bundled_ffmpeg(&state),
                &state.video.cancel,
                |downloaded, total| {
                    {
                        let mut status = lock(&state.video.download);
                        status.downloaded = downloaded;
                        status.total = total;
                    }
                    if last.elapsed() >= PROGRESS_EVERY {
                        last = Instant::now();
                        report(&app);
                    }
                },
            );
            let installed = result.is_ok();
            {
                let mut status = lock(&state.video.download);
                status.downloading = false;
                status.error = result.err().filter(|error| error != CANCELLED);
            }
            report(&app);
            if installed {
                prepare(&app);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("download thread failed: {error}");
    }
}

fn download_blocking(
    target: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into();
    let expected = agent
        .get(FFMPEG_SHA)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut response = agent.get(FFMPEG_ZIP).call().map_err(|e| e.to_string())?;
    let total = response.body().content_length();
    let mut bytes = Vec::new();
    copy_with_progress(
        response.body_mut().as_reader(),
        &mut bytes,
        total,
        cancel,
        |done| on_progress(done, total),
    )?;
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if expected.len() != 64 || actual != expected {
        return Err("downloaded ffmpeg does not match the published checksum".into());
    }
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let index = (0..archive.len())
        .find(|&i| {
            archive.by_index(i).is_ok_and(|f| {
                f.name()
                    .is_ok_and(|name| name.replace('\\', "/").ends_with("/bin/ffmpeg.exe"))
            })
        })
        .ok_or("ffmpeg.exe not found in the archive")?;
    let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
    let mut exe = Vec::new();
    entry.read_to_end(&mut exe).map_err(|e| e.to_string())?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = target.with_extension("part");
    std::fs::write(&temp, exe).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, target).map_err(|e| e.to_string())?;
    tracing::info!("ffmpeg installed at {}", target.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::UNIX_EPOCH;

    #[test]
    fn copy_with_progress_reports_and_cancels() {
        let data = vec![7u8; 1 << 20];
        let total = u64::try_from(data.len()).unwrap();
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let mut out = Vec::new();
        let result = copy_with_progress(&data[..], &mut out, Some(total), &cancel, |done| {
            seen.push(done);
            if seen.len() == 2 {
                cancel.store(true, Ordering::SeqCst);
            }
        });
        assert_eq!(result, Err("cancelled".to_string()));
        assert_eq!(seen.len(), 2);
        assert!(seen[0] > 0 && seen[1] > seen[0]);
        assert!(out.len() < data.len());
    }

    #[test]
    fn copy_with_progress_rejects_short_body() {
        let cancel = AtomicBool::new(false);
        let mut out = Vec::new();
        assert!(copy_with_progress(&b"abc"[..], &mut out, Some(10), &cancel, |_| {}).is_err());
        out.clear();
        assert_eq!(
            copy_with_progress(&b"abc"[..], &mut out, None, &cancel, |_| {}),
            Ok(3)
        );
    }

    #[test]
    fn capture_args_use_ring_of_segments() {
        let options = CaptureOptions {
            fps: 30,
            quality: VideoQuality::Balanced,
            wrap: 18,
        };
        let args = capture_args("h264_amf", Path::new(r"C:\tmp\rec"), &options, None);
        let joined = args.join(" ");
        assert!(joined.contains(
            "ddagrab=output_idx=0:framerate=30,hwdownload,format=bgra,scale=-2:'min(ih,1080)'"
        ));
        assert!(joined.contains("-c:v h264_amf -quality balanced -rc cqp -qp_i 22 -qp_p 22"));
        assert!(joined.contains("-g 30"));
        assert!(
            joined.contains("-f segment -segment_format matroska -segment_time 1 -segment_wrap 18")
        );
        let list = Path::new(r"C:\tmp\rec")
            .join("segments.csv")
            .display()
            .to_string();
        assert!(joined.contains(&format!(
            "-reset_timestamps 1 -segment_list {list} -segment_list_type csv -segment_list_size 18"
        )));
        assert!(args.last().unwrap().ends_with("seg%02d.mkv"));
    }

    fn encoder_line(encoder: &str, quality: VideoQuality) -> String {
        let options = CaptureOptions {
            fps: 60,
            quality,
            wrap: 10,
        };
        capture_args(encoder, Path::new("rec"), &options, None).join(" ")
    }

    #[test]
    fn capture_args_follow_fps_and_quality() {
        let light = encoder_line("h264_nvenc", VideoQuality::Light);
        assert!(light.contains("framerate=60,hwdownload,format=bgra,scale=-2:'min(ih,720)'"));
        assert!(light.contains("-c:v h264_nvenc -preset p4 -tune hq -rc constqp -qp 25"));
        assert!(light.contains("-g 60"));
        let high = encoder_line("libx264", VideoQuality::High);
        assert!(high.contains("framerate=60,hwdownload,format=bgra[v] -map [v] -c:v"));
        assert!(high.contains("-preset ultrafast -tune zerolatency -crf 20"));
        assert!(!high.contains("-c:a"));
        assert!(
            encoder_line("h264_qsv", VideoQuality::High)
                .contains("-c:v h264_qsv -preset medium -global_quality 20")
        );
    }

    #[test]
    fn av1_and_hevc_encoders_record_at_their_own_scales() {
        assert!(encoder_line("av1_amf", VideoQuality::High).contains(
            "-c:v av1_amf -usage transcoding -quality quality -rc cqp -qp_i 56 -qp_p 56"
        ));
        assert!(encoder_line("av1_amf", VideoQuality::Light).contains("-qp_i 76 -qp_p 76"));
        assert!(
            encoder_line("av1_nvenc", VideoQuality::Balanced)
                .contains("-c:v av1_nvenc -preset p5 -tune hq -rc constqp -qp 64")
        );
        assert!(
            encoder_line("hevc_amf", VideoQuality::Balanced)
                .contains("-c:v hevc_amf -quality quality -rc cqp -qp_i 24 -qp_p 24")
        );
        assert!(
            encoder_line("hevc_nvenc", VideoQuality::High)
                .contains("-c:v hevc_nvenc -preset p5 -tune hq -rc constqp -qp 22")
        );
        assert!(
            encoder_line("hevc_qsv", VideoQuality::Light)
                .contains("-c:v hevc_qsv -preset medium -global_quality 27")
        );
    }

    #[test]
    fn saving_joins_segments_without_recompressing() {
        let av1 = save_args(Path::new("clip.txt"), Path::new("Mesa.mp4"), "av1_amf").join(" ");
        assert_eq!(
            av1,
            "-hide_banner -loglevel error -y -f concat -safe 0 -i clip.txt -c:v copy -c:a aac -b:a 160k -af aresample=async=1 -movflags +faststart Mesa.mp4"
        );
        let hevc = save_args(Path::new("clip.txt"), Path::new("Mesa.mp4"), "hevc_amf").join(" ");
        assert!(hevc.contains("-c:v copy -tag:v hvc1 -c:a aac"));
    }

    #[test]
    fn segment_durations_come_from_the_segment_list() {
        let durations = segment_durations(
            "seg00.mkv,0.000000,1.021000\nseg01.mkv,1.021000,2.021000\n\nbroken\n",
        );
        assert_eq!(durations.len(), 2);
        assert!((durations["seg00.mkv"] - 1.021).abs() < 1e-9);
        assert!((durations["seg01.mkv"] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn capture_args_take_loopback_audio_from_stdin() {
        let options = CaptureOptions {
            fps: 60,
            quality: VideoQuality::Balanced,
            wrap: 13,
        };
        let audio = AudioFormat {
            rate: 48_000,
            channels: 8,
        };
        let args = capture_args("h264_amf", Path::new("rec"), &options, Some(audio));
        let joined = args.join(" ");
        assert!(joined.starts_with(
            "-hide_banner -loglevel error -y -thread_queue_size 1024 -f f32le -ar 48000 -ac 8 -i pipe:0 -filter_complex"
        ));
        assert!(joined.contains("[v] -map [v] -map 0:a -c:v h264_amf"));
        assert!(joined.contains("-c:a aac -b:a 160k -ac 2"));
        assert!(args.last().unwrap().ends_with("seg%02d.mkv"));
    }

    #[test]
    fn capture_restarts_only_for_capture_changes() {
        let old = Settings {
            video: true,
            ..Settings::default()
        };
        assert!(!capture_changed(&old, &old.clone()));
        let renamed = Settings {
            video_dir: Some(PathBuf::from("D:/clips")),
            ..old.clone()
        };
        assert!(!capture_changed(&old, &renamed));
        for changed in [
            Settings {
                video_fps: 60,
                ..old.clone()
            },
            Settings {
                video_quality: VideoQuality::High,
                ..old.clone()
            },
            Settings {
                video_before: 30,
                ..old.clone()
            },
            Settings {
                video_after: 0,
                ..old.clone()
            },
        ] {
            assert!(capture_changed(&old, &changed));
        }
    }

    #[test]
    fn ring_holds_the_whole_clip_window() {
        assert_eq!(ring_size(10, 5), 18);
        assert_eq!(ring_size(60, 15), 78);
    }

    #[test]
    fn encoder_preference() {
        let works = |_: &str| true;
        assert_eq!(
            pick_encoder(" V..... h264_amf  AMD\n V..... libx264", works),
            "h264_amf"
        );
        assert_eq!(pick_encoder("h264_qsv h264_nvenc", works), "h264_nvenc");
        assert_eq!(pick_encoder("libx264 only", works), "libx264");
        assert_eq!(pick_encoder("h264_amf hevc_amf av1_amf", works), "av1_amf");
        assert_eq!(
            pick_encoder("h264_amf hevc_amf av1_amf", |e| e != "av1_amf"),
            "hevc_amf"
        );
    }

    #[test]
    fn encoder_skips_listed_hardware_that_cannot_open() {
        let listed = " V....D h264_nvenc\n V....D h264_amf\n V....D h264_qsv";
        assert_eq!(pick_encoder(listed, |e| e == "h264_amf"), "h264_amf");
        assert_eq!(pick_encoder(listed, |e| e == "h264_qsv"), "h264_qsv");
        assert_eq!(pick_encoder(listed, |_| false), "libx264");
    }

    #[test]
    fn clip_takes_the_segments_since_the_window_start() {
        let t = |s: u64| UNIX_EPOCH + Duration::from_secs(s);
        let files = vec![
            (PathBuf::from("seg03.ts"), t(30)),
            (PathBuf::from("seg00.ts"), t(60)),
            (PathBuf::from("seg01.ts"), t(70)),
            (PathBuf::from("seg02.ts"), t(20)),
        ];
        assert_eq!(
            clip_segments(&files, t(25)),
            vec![
                PathBuf::from("seg03.ts"),
                PathBuf::from("seg00.ts"),
                PathBuf::from("seg01.ts")
            ]
        );
        assert!(clip_segments(&files, t(80)).is_empty());
    }

    #[test]
    fn concat_list_escapes_quotes_and_slashes() {
        let list = concat_list(&[
            (PathBuf::from(r"C:\it's\seg01.mkv"), Some(1.0)),
            (PathBuf::from("seg02.mkv"), None),
        ]);
        assert_eq!(
            list,
            "file 'C:/it'\\''s/seg01.mkv'\nduration 1.000000\nfile 'seg02.mkv'\n"
        );
    }
}

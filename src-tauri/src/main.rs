#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use zip::write::FileOptions;

struct AppState { root: PathBuf }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaItem {
    media_id: String,
    filename: String,
    path: String,
    media_type: String,
    metadata: Value,
    thumbnail_path: Option<String>,
    #[serde(default)]
    filmstrip: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CropRect { x: i64, y: i64, width: i64, height: i64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Segment { start: f64, end: f64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdaptiveWindow { start: f64, end: f64, x: i64, y: i64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdaptivePlan { width: i64, height: i64, windows: Vec<AdaptiveWindow> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessRequest {
    media_id: String,
    #[serde(default)] trim_start: f64,
    trim_end: Option<f64>,
    #[serde(default)] segments: Vec<Segment>,
    crop: Option<CropRect>,
    adaptive_plan: Option<AdaptivePlan>,
    output_format: Option<String>,
    #[serde(default)] adjustments: Option<Adjustments>,
    #[serde(default)] export: Option<ExportSettings>,
    #[serde(default)] watermark: Option<String>,
    #[serde(default)] lut: Option<String>,
    #[serde(default)] lut_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Adjustments {
    #[serde(default)] brightness: f32,
    #[serde(default = "default_contrast")] contrast: f32,
    #[serde(default = "default_saturation")] saturation: f32,
    #[serde(default)] sharpness: f32,
}
fn default_contrast() -> f32 { 1.0 }
fn default_saturation() -> f32 { 1.0 }
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExportSettings {
    resolution: Option<String>,
    bitrate: Option<String>,
    frame_rate: Option<String>,
    encoder: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutoCropRequest { mode: String, #[serde(default = "default_window")] window_seconds: f64, #[serde(default = "default_aspect")] aspect_ratio: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputItem { output_id: String, filename: String, path: String, media_type: String, metadata: Value }

fn default_window() -> f64 { 8.0 }
fn default_aspect() -> String { "free".into() }
fn safe_stem(name: &str) -> String {
    let stem = Path::new(name).file_stem().and_then(|v| v.to_str()).unwrap_or("media");
    let cleaned: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() { "media".into() } else { trimmed.chars().take(80).collect() }
}
fn run_capture(program: &str, args: &[String]) -> Result<std::process::Output, String> {
    Command::new(program).args(args).output().map_err(|e| format!("Could not start {program}. Install FFmpeg and make sure {program} is on PATH. Details: {e}"))
}
fn probe(path: &Path) -> Result<(String, Value), String> {
    let args = vec!["-v".into(), "error".into(), "-show_streams".into(), "-show_format".into(), "-of".into(), "json".into(), path.to_string_lossy().into_owned()];
    let output = run_capture("ffprobe", &args)?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    let root: Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("Could not parse media metadata: {e}"))?;
    let streams = root.get("streams").and_then(Value::as_array).cloned().unwrap_or_default();
    let video = streams.iter().find(|s| s.get("codec_type").and_then(Value::as_str) == Some("video"));
    let audio = streams.iter().find(|s| s.get("codec_type").and_then(Value::as_str) == Some("audio"));
    let ext = path.extension().and_then(|v| v.to_str()).unwrap_or("").to_ascii_lowercase();
    let image_ext = ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "gif"].contains(&ext.as_str());
    let media_type = if image_ext && ext != "gif" { "image" } else if video.is_some() { "video" } else if audio.is_some() { "audio" } else { return Err("Unsupported media type. Use an image, video, or audio file.".into()); };
    let format = root.get("format").cloned().unwrap_or_else(|| json!({}));
    let duration = format.get("duration").and_then(|v| v.as_str()).and_then(|v| v.parse::<f64>().ok()).unwrap_or_else(|| video.and_then(|s| s.get("duration")).and_then(|v| v.as_str()).and_then(|v| v.parse().ok()).unwrap_or(0.0));
    let width = video.and_then(|s| s.get("width")).and_then(Value::as_i64).unwrap_or(0);
    let height = video.and_then(|s| s.get("height")).and_then(Value::as_i64).unwrap_or(0);
    let frame_rate = video.and_then(|s| s.get("avg_frame_rate")).and_then(Value::as_str).and_then(parse_fraction).unwrap_or(0.0);
    let metadata = json!({
        "width": width, "height": height, "duration": duration,
        "frame_rate": frame_rate,
        "has_audio": audio.is_some(),
        "codec": video.and_then(|s| s.get("codec_name")).and_then(Value::as_str).or_else(|| audio.and_then(|s| s.get("codec_name")).and_then(Value::as_str)).unwrap_or("unknown"),
        "sample_rate": audio.and_then(|s| s.get("sample_rate")).and_then(Value::as_str).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0),
        "channels": audio.and_then(|s| s.get("channels")).and_then(Value::as_i64).unwrap_or(0),
        "format": format.get("format_name").and_then(Value::as_str).unwrap_or("unknown")
    });
    Ok((media_type.into(), metadata))
}
fn parse_fraction(raw: &str) -> Option<f64> { let mut p = raw.split('/'); let a = p.next()?.parse::<f64>().ok()?; let b = p.next()?.parse::<f64>().ok()?; if b == 0.0 { None } else { Some(a / b) } }

fn generate_thumbnail(source: &Path, folder: &Path, media_type: &str, metadata: &Value) -> Option<String> {
    let target = folder.join("thumbnail.jpg");
    match media_type {
        "image" => {
            let image = image::open(source).ok()?;
            let preview = image.thumbnail(480, 320).to_rgb8();
            image::DynamicImage::ImageRgb8(preview)
                .save_with_format(&target, image::ImageFormat::Jpeg).ok()?;
        }
        "video" => {
            let duration = metadata.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
            let seek = if duration > 0.5 { (duration * 0.12).min(2.5) } else { 0.0 };
            let args = vec![
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-ss".into(), format!("{seek:.3}"), "-i".into(), source.to_string_lossy().into_owned(),
                "-frames:v".into(), "1".into(),
                "-vf".into(), "scale=480:320:force_original_aspect_ratio=decrease,pad=480:320:(ow-iw)/2:(oh-ih)/2:color=0x171714".into(),
                "-q:v".into(), "4".into(), target.to_string_lossy().into_owned()
            ];
            let output = run_capture("ffmpeg", &args).ok()?;
            if !output.status.success() || !target.is_file() {
                let _ = fs::remove_file(&target);
                return None;
            }
        }
        "audio" => {
            // A locally generated waveform tile gives audio items a consistent
            // visual identity without decoding or uploading the source.
            let mut canvas = image::RgbaImage::from_pixel(480, 320, image::Rgba([23, 24, 22, 255]));
            let duration = metadata.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
            let seed = source.file_name().and_then(|s| s.to_str()).unwrap_or("audio")
                .bytes().fold(0u32, |a,b| a.wrapping_mul(33).wrapping_add(b as u32));
            for i in 0..42u32 {
                let wave = ((seed.rotate_left(i % 31) ^ (i * 7919)) % 100) as i32;
                let height = 18 + ((wave * 2 + (duration as i32 % 37)) % 96).abs();
                let x = 22 + i * 10;
                for yy in (160-height/2)..(160+height/2) {
                    for xx in x..(x+4) {
                        if xx < 480 && yy >= 0 && yy < 320 {
                            canvas.put_pixel(xx, yy as u32, image::Rgba([205, 255, 85, 255]));
                        }
                    }
                }
            }
            let rgb = image::DynamicImage::ImageRgba8(canvas).to_rgb8();
            image::DynamicImage::ImageRgb8(rgb)
                .save_with_format(&target, image::ImageFormat::Jpeg).ok()?;
        }
        _ => return None,
    }
    Some(target.to_string_lossy().into_owned())
}


fn generate_filmstrip(source: &Path, folder: &Path, duration: f64) -> Vec<String> {
    if duration <= 0.0 { return Vec::new(); }
    let pattern = folder.join("film-%02d.jpg");
    let fps = 8.0 / duration.max(1.0);
    let vf = format!("fps={fps:.8},scale=180:104:force_original_aspect_ratio=decrease,pad=180:104:(ow-iw)/2:(oh-ih)/2:color=0x171714");
    let args = vec![
        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
        "-i".into(), source.to_string_lossy().into_owned(),
        "-vf".into(), vf, "-frames:v".into(), "8".into(),
        "-q:v".into(), "6".into(), pattern.to_string_lossy().into_owned()
    ];
    let output = run_capture("ffmpeg", &args);
    if output.as_ref().map(|o| !o.status.success()).unwrap_or(true) { return Vec::new(); }
    (1..=8).map(|i| folder.join(format!("film-{i:02}.jpg")))
        .filter(|p|p.is_file()).map(|p|p.to_string_lossy().into_owned()).collect()
}

fn ffmpeg_escape_path(value: &str) -> String {
    value.replace('\\', r"\\").replace(':', r"\:").replace('\'', r"\'")
}

fn video_adjustment_filters(request: &ProcessRequest, watermark_file: Option<&Path>) -> Vec<String> {
    let mut filters = Vec::new();
    if let Some(a) = request.adjustments.as_ref() {
        let brightness = (a.brightness.clamp(-100.0, 100.0) / 100.0).clamp(-1.0, 1.0);
        let contrast = a.contrast.clamp(0.0, 2.0);
        let saturation = a.saturation.clamp(0.0, 2.0);
        if brightness.abs() > 0.001 || (contrast-1.0).abs() > 0.001 || (saturation-1.0).abs() > 0.001 {
            filters.push(format!("eq=brightness={brightness:.3}:contrast={contrast:.3}:saturation={saturation:.3}"));
        }
        if a.sharpness > 0.01 {
            let amount = a.sharpness.clamp(0.0, 5.0);
            filters.push(format!("unsharp=5:5:{amount:.2}:5:5:0"));
        }
    }
    match request.lut.as_deref().unwrap_or("none") {
        "mono" => filters.push("hue=s=0".into()),
        "warm" => filters.push("colorbalance=rs=0.08:gs=0.02:bs=-0.06".into()),
        "cool" => filters.push("colorbalance=rs=-0.05:gs=0.01:bs=0.08".into()),
        "vintage" => { filters.push("eq=saturation=0.78:contrast=0.92:brightness=0.02".into()); filters.push("colorbalance=rs=0.06:gs=0.01:bs=-0.04".into()); },
        "cinematic" => { filters.push("eq=contrast=1.12:saturation=0.88".into()); filters.push("colorbalance=rs=0.04:bs=0.06:rh=0.05:bh=-0.04".into()); },
        _ => {}
    }
    if let Some(path) = request.lut_path.as_deref().filter(|p|Path::new(p).is_file()) {
        let escaped=ffmpeg_escape_path(path);
        filters.push(format!("lut3d=file='{escaped}'"));
    }
    if let Some(path) = watermark_file {
        let escaped=ffmpeg_escape_path(&path.to_string_lossy());
        filters.push(format!("drawtext=textfile='{escaped}':fontcolor=white:fontsize=h/28:box=1:boxcolor=black@0.45:boxborderw=12:x=w-tw-24:y=h-th-24"));
    }
    if let Some(export) = request.export.as_ref() {
        if let Some(fps) = export.frame_rate.as_deref().and_then(|v|v.parse::<f64>().ok()).filter(|v|*v>=1.0&&*v<=240.0) {
            filters.push(format!("fps={fps:.3}"));
        }
        if let Some(res) = export.resolution.as_deref() {
            let scale = match res {
                "720p" => Some("scale=w='if(gte(iw,ih),-2,720)':h='if(gte(iw,ih),720,-2)'"),
                "1080p" => Some("scale=w='if(gte(iw,ih),-2,1080)':h='if(gte(iw,ih),1080,-2)'"),
                "1440p" => Some("scale=w='if(gte(iw,ih),-2,1440)':h='if(gte(iw,ih),1440,-2)'"),
                "2160p" => Some("scale=w='if(gte(iw,ih),-2,2160)':h='if(gte(iw,ih),2160,-2)'"),
                _ => None,
            };
            if let Some(s) = scale { filters.push(s.into()); }
        }
    }
    filters
}

fn clamp_i(v: i64, lo: i64, hi: i64) -> i64 { v.max(lo).min(hi) }
fn even(v: i64) -> i64 { let n = v.max(2); if n % 2 == 0 { n } else { n - 1 } }
fn ratio_from(value: &str) -> Option<f64> { if value == "free" || value.is_empty() { return None; } if let Some((a,b)) = value.split_once(':') { let x = a.parse::<f64>().ok()?; let y = b.parse::<f64>().ok()?; if x > 0.0 && y > 0.0 { Some(x/y) } else { None } } else { value.parse::<f64>().ok().filter(|v| *v > 0.0) } }
fn normalize_crop(c: &CropRect, fw: i64, fh: i64, evenize: bool) -> CropRect {
    let mut w = c.width.max(2).min(fw.max(2)); let mut h = c.height.max(2).min(fh.max(2));
    if evenize { w = even(w); h = even(h); }
    CropRect { x: clamp_i(c.x, 0, (fw-w).max(0)), y: clamp_i(c.y, 0, (fh-h).max(0)), width: w, height: h }
}
fn fit_aspect(c: &CropRect, ratio: Option<f64>, fw: i64, fh: i64, evenize: bool) -> CropRect {
    let mut out = normalize_crop(c, fw, fh, evenize);
    if let Some(r) = ratio {
        if (out.width as f64)/(out.height as f64) > r { out.width = (out.height as f64*r).floor() as i64; }
        else { out.height = (out.width as f64/r).floor() as i64; }
        if evenize { out.width=even(out.width); out.height=even(out.height); }
        out.width=out.width.max(2).min(fw); out.height=out.height.max(2).min(fh);
        let cx=out.x as f64 + c.width as f64/2.0; let cy=out.y as f64 + c.height as f64/2.0;
        out.x=clamp_i((cx-out.width as f64/2.0).round() as i64,0,(fw-out.width).max(0));
        out.y=clamp_i((cy-out.height as f64/2.0).round() as i64,0,(fh-out.height).max(0));
    }
    out
}
fn run_ffmpeg(args: &[String], duration: f64, app: Option<&AppHandle>, job_id: &str) -> Result<(), String> {
    let mut command = Command::new("ffmpeg");
    if args.is_empty() { return Err("Empty FFmpeg command".into()); }
    command.args(&args[..args.len()-1]).arg("-progress").arg("pipe:1").arg("-nostats").arg(&args[args.len()-1]).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| format!("Could not start FFmpeg. Install FFmpeg and add it to PATH. Details: {e}"))?;
    let stdout = child.stdout.take().ok_or("Could not read FFmpeg progress")?;
    let stderr = child.stderr.take().ok_or("Could not read FFmpeg errors")?;
    let err_thread = std::thread::spawn(move || { let mut s=String::new(); let mut r=BufReader::new(stderr); let _=r.read_to_string(&mut s); s });
    if let Some(a) = app { let _ = a.emit("cropstudio:progress", json!({"jobId":job_id,"progress":1,"stage":"Starting encoder"})); }
    for line in BufReader::new(stdout).lines().flatten() {
        if let Some(v) = line.strip_prefix("out_time_ms=") {
            if let Ok(micros) = v.parse::<f64>() {
                let pct = if duration > 0.0 { (micros/1_000_000.0/duration*100.0).clamp(0.0,99.0) } else { 50.0 };
                if let Some(a)=app { let _=a.emit("cropstudio:progress",json!({"jobId":job_id,"progress":pct,"stage":"Encoding media"})); }
            }
        }
    }
    let status=child.wait().map_err(|e| e.to_string())?;
    let stderr_text=err_thread.join().unwrap_or_default();
    if !status.success() { return Err(if stderr_text.trim().is_empty() { "FFmpeg failed to process this media.".into() } else { stderr_text.trim().chars().rev().take(3500).collect::<String>().chars().rev().collect() }); }
    if let Some(a)=app { let _=a.emit("cropstudio:progress",json!({"jobId":job_id,"progress":99,"stage":"Finalizing output"})); }
    Ok(())
}
fn make_segments(duration: f64, start: f64, end: Option<f64>, segments: &[Segment]) -> Result<Vec<Segment>,String> {
    let mut out=if !segments.is_empty() { segments.to_vec() } else { vec![Segment{start,end:end.unwrap_or(duration)}] };
    out.sort_by(|a,b| a.start.total_cmp(&b.start));
    for s in &mut out { s.start=s.start.max(0.0); if duration>0.0 { s.end=s.end.min(duration); } if s.end<=s.start { return Err("Each trim segment must end after it starts.".into()); } }
    if out.is_empty() { return Err("Add a valid trim range before exporting.".into()); }
    Ok(out)
}
fn encode_args(media_type: &str, format: &str, has_audio: bool) -> Result<Vec<String>,String> {
    let mut a=Vec::<String>::new();
    match media_type {
        "image" => match format { "png"=>a.extend(["-frames:v","1","-c:v","png","-compression_level","6"].map(str::to_string)), "jpg"|"jpeg"=>a.extend(["-frames:v","1","-c:v","mjpeg","-q:v","2"].map(str::to_string)), "webp"=>a.extend(["-frames:v","1","-c:v","libwebp","-quality","88"].map(str::to_string)), _=>return Err("Choose PNG, JPEG, or WebP for images.".into()) },
        "video" => match format { "mp4"=>{a.extend(["-c:v","libx264","-preset","fast","-crf","19","-pix_fmt","yuv420p"].map(str::to_string)); if has_audio {a.extend(["-c:a","aac","-b:a","192k"].map(str::to_string));} else {a.push("-an".into());} a.extend(["-movflags","+faststart"].map(str::to_string));}, "webm"=>{a.extend(["-c:v","libvpx-vp9","-deadline","good","-cpu-used","4","-crf","32","-b:v","0"].map(str::to_string)); if has_audio {a.extend(["-c:a","libopus","-b:a","128k"].map(str::to_string));} else {a.push("-an".into());}}, _=>return Err("Choose MP4 or WebM for video.".into()) },
        "audio" => match format { "mp3"=>a.extend(["-c:a","libmp3lame","-b:a","192k"].map(str::to_string)), "wav"=>a.extend(["-c:a","pcm_s16le"].map(str::to_string)), "m4a"=>a.extend(["-c:a","aac","-b:a","192k","-movflags","+faststart"].map(str::to_string)), "flac"=>a.extend(["-c:a","flac"].map(str::to_string)), _=>return Err("Choose MP3, WAV, M4A, or FLAC for audio.".into()) },
        _=>return Err("Unsupported media type.".into())
    }
    Ok(a)
}
fn filter_crop(crop: &CropRect, fw: i64, fh: i64) -> String { let c=normalize_crop(crop,fw,fh,true); format!("crop={}:{}:{}:{}",c.width,c.height,c.x,c.y) }
fn filter_crop_image(crop: &CropRect, fw: i64, fh: i64) -> String { let c=normalize_crop(crop,fw,fh,false); format!("crop={}:{}:{}:{}",c.width,c.height,c.x,c.y) }
fn adaptive_filter(plan: &AdaptivePlan, fw: i64, fh: i64) -> Result<String,String> {
    if plan.windows.is_empty() { return Err("Adaptive crop plan has no time windows.".into()); }
    let w=even(plan.width.min(fw)); let h=even(plan.height.min(fh));
    let mut windows=plan.windows.clone(); windows.sort_by(|a,b| a.start.total_cmp(&b.start));
    let mut x="0".to_string(); let mut y="0".to_string();
    for win in windows.iter().rev() { let xx=clamp_i(win.x,0,(fw-w).max(0)); let yy=clamp_i(win.y,0,(fh-h).max(0)); x=format!("if(between(t\\,{:.3}\\,{:.3})\\,{}\\,{})",win.start,win.end,xx,x); y=format!("if(between(t\\,{:.3}\\,{:.3})\\,{}\\,{})",win.start,win.end,yy,y); }
    Ok(format!("crop={w}:{h}:x='{x}':y='{y}'"))
}
fn output_ext(format: &str) -> &str { match format { "jpeg"=>"jpg", other=>other } }

fn configure_video_encoder(args: &mut Vec<String>, request: &ProcessRequest) {
    let export=request.export.as_ref();
    let requested=export.and_then(|e|e.encoder.as_deref()).unwrap_or("auto");
    let mut encoder=match requested {
        "nvenc"=>"h264_nvenc","qsv"=>"h264_qsv","amf"=>"h264_amf","software"=>"libx264",_=>"libx264"
    };
    if requested!="auto" && requested!="software" {
        let probe=run_capture("ffmpeg",&["-hide_banner".into(),"-encoders".into()]);
        let compiled=probe.map(|o|String::from_utf8_lossy(&o.stdout).contains(encoder)).unwrap_or(false);
        let runtime=if compiled {
            run_capture("ffmpeg",&[
                "-hide_banner".into(),"-loglevel".into(),"error".into(),
                "-f".into(),"lavfi".into(),"-i".into(),"color=c=black:s=64x64:d=0.1".into(),
                "-frames:v".into(),"1".into(),"-c:v".into(),encoder.into(),"-f".into(),"null".into(),"-".into()
            ]).map(|o|o.status.success()).unwrap_or(false)
        } else { false };
        if !runtime { encoder="libx264"; }
    }
    if let Some(i)=args.iter().position(|v|v=="-c:v") {
        if i+1<args.len(){args[i+1]=encoder.into();}
    }
    args.retain(|v|!matches!(v.as_str(),"-crf"|"19"|"32"|"-b:v"|"0"|"-preset"|"fast"|"good"|"4"|"-deadline"));
    let bitrate=export.and_then(|e|e.bitrate.as_deref()).unwrap_or("auto");
    if bitrate!="auto" { args.extend(["-b:v".into(),bitrate.into()]); }
    match encoder {
        "h264_nvenc"=>{args.extend(["-preset".into(),"p4".into()]);if bitrate=="auto"{args.extend(["-cq".into(),"23".into()]);}},
        "h264_qsv"=>{args.extend(["-preset".into(),"medium".into()]);if bitrate=="auto"{args.extend(["-global_quality".into(),"23".into()]);}},
        "h264_amf"=>{args.extend(["-quality".into(),"balanced".into()]);if bitrate=="auto"{args.extend(["-qp_i".into(),"22".into(),"-qp_p".into(),"24".into()]);}},
        _=>{args.extend(["-preset".into(),"fast".into()]);if bitrate=="auto"{args.extend(["-crf".into(),"19".into()]);}},
    }
}

fn process_one(root: &Path, request: ProcessRequest, app: Option<AppHandle>, job_id: String) -> Result<OutputItem,String> {
    if request.media_id.len()!=32 || !request.media_id.chars().all(|c|c.is_ascii_hexdigit()) { return Err("Invalid media identifier.".into()); }
    let media_dir=root.join("uploads").join(&request.media_id);
    let meta:Value=serde_json::from_slice(&fs::read(media_dir.join("meta.json")).map_err(|_| "Media not found. Please add it again.".to_string())?).map_err(|e|e.to_string())?;
    let source=media_dir.join(meta.get("stored_name").and_then(Value::as_str).ok_or("Missing source media filename")?);
    let media_type=meta.get("media_type").and_then(Value::as_str).unwrap_or("video").to_string();
    let (actual_type, metadata)=probe(&source)?;
    if actual_type!=media_type { return Err("The source media type changed or is unsupported.".into()); }
    let format=request.output_format.clone().unwrap_or_else(|| match media_type.as_str(){"image"=>"png".into(),"audio"=>"mp3".into(),_=>"mp4".into()}).to_lowercase().trim_start_matches('.').to_string();
    let ext=output_ext(&format);
    let id=Uuid::new_v4().simple().to_string(); let out_dir=root.join("outputs").join(&id); fs::create_dir_all(&out_dir).map_err(|e|e.to_string())?;
    let original=meta.get("original_name").and_then(Value::as_str).unwrap_or("media");
    let filename=format!("{}-cropped.{}",safe_stem(original),ext); let target=out_dir.join(format!("output.{}",ext));
    let watermark_file=if let Some(text)=request.watermark.as_deref().filter(|s|!s.trim().is_empty()) {
        let path=out_dir.join("watermark.txt");
        fs::write(&path,text).map_err(|e|format!("Could not write watermark text: {e}"))?;
        Some(path)
    } else { None };
    let fw=metadata.get("width").and_then(Value::as_i64).unwrap_or(0); let fh=metadata.get("height").and_then(Value::as_i64).unwrap_or(0);
    let has_audio=metadata.get("has_audio").and_then(Value::as_bool).unwrap_or(false);
    if media_type == "image" {
        if let Some(a)=app.as_ref() { let _=a.emit("cropstudio:progress",json!({"jobId":job_id.clone(),"progress":8,"stage":"Processing image"})); }
        let mut filters=video_adjustment_filters(&request,watermark_file.as_deref());
        if let Some(c)=request.crop.as_ref(){filters.insert(0,filter_crop_image(c,fw,fh));}
        let mut args=vec!["-y".into(),"-hide_banner".into(),"-loglevel".into(),"error".into(),"-i".into(),source.to_string_lossy().into_owned()];
        if !filters.is_empty(){args.extend(["-vf".into(),filters.join(",")]);}
        args.extend(encode_args("image",&format,false)?);
        args.push(target.to_string_lossy().into_owned());
        let output=run_capture("ffmpeg",&args)?;
        if !output.status.success(){return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());}
        if let Some(path)=watermark_file.as_ref(){let _=fs::remove_file(path);}
        let (_,out_meta)=probe(&target)?;
        let result=OutputItem{output_id:id.clone(),filename:filename.clone(),path:target.to_string_lossy().into_owned(),media_type:media_type.clone(),metadata:out_meta};
        let manifest=json!({"output_id":id,"filename":filename,"path":target.file_name().and_then(|v|v.to_str()).unwrap_or("output"),"media_type":media_type,"metadata":result.metadata.clone()});
        fs::write(out_dir.join("meta.json"),serde_json::to_vec_pretty(&manifest).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if let Some(a)=app { let _=a.emit("cropstudio:progress",json!({"jobId":job_id.clone(),"progress":100,"stage":"Complete"})); }
        return Ok(result);
    }
    let mut args=vec!["-y".into(),"-hide_banner".into(),"-loglevel".into(),"error".into(),"-i".into(),source.to_string_lossy().into_owned()];
    let mut duration=metadata.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
    match media_type.as_str() {
        "image" => { if let Some(c)=request.crop { args.extend(["-vf".into(),filter_crop(&c,fw,fh)]); } args.extend(encode_args("image",&format,false)?); },
        "video" => {
            let crop_filter=if let Some(p)=request.adaptive_plan.as_ref(){Some(adaptive_filter(p,fw,fh)?)}else{request.crop.as_ref().map(|c|filter_crop(c,fw,fh))};
            let mut video_filters=video_adjustment_filters(&request,watermark_file.as_deref());
            if let Some(c)=crop_filter.as_ref(){video_filters.insert(0,c.clone());}
            let filter_chain=video_filters.join(",");
            let segments=make_segments(duration,request.trim_start,request.trim_end,&request.segments)?;
            duration=segments.iter().map(|s|s.end-s.start).sum();
            if segments.len()==1 {
                let s=&segments[0]; args.extend(["-ss".into(),format!("{:.3}",s.start),"-t".into(),format!("{:.3}",s.end-s.start)]);
                if !filter_chain.is_empty() { args.extend(["-vf".into(),filter_chain.clone()]); }
            } else {
                let n=segments.len(); let mut graph=String::new();
                graph.push_str(&format!("[0:v:0]split={n}")); for i in 0..n {graph.push_str(&format!("[vs{i}]"));} graph.push(';');
                if has_audio {graph.push_str(&format!("[0:a:0]asplit={n}")); for i in 0..n {graph.push_str(&format!("[as{i}]"));} graph.push(';');}
                for (i,s) in segments.iter().enumerate() { graph.push_str(&format!("[vs{i}]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS",s.start,s.end)); if !filter_chain.is_empty(){graph.push_str(&format!(",{filter_chain}"));} graph.push_str(&format!("[v{i}];")); if has_audio {graph.push_str(&format!("[as{i}]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{i}];",s.start,s.end));} }
                if has_audio { for i in 0..n {graph.push_str(&format!("[v{i}][a{i}]"));} graph.push_str(&format!("concat=n={n}:v=1:a=1[vout][aout]")); } else {for i in 0..n {graph.push_str(&format!("[v{i}]"));} graph.push_str(&format!("concat=n={n}:v=1:a=0[vout]"));}
                args.extend(["-filter_complex".into(),graph,"-map".into(),"[vout]".into()]); if has_audio {args.extend(["-map".into(),"[aout]".into()]);}
            }
            args.extend(encode_args("video",&format,has_audio)?);
            if format=="mp4" { configure_video_encoder(&mut args,&request); }
            else if format=="webm" {
                let bitrate=request.export.as_ref().and_then(|e|e.bitrate.as_deref()).unwrap_or("auto");
                if bitrate!="auto" {
                    args.retain(|v|v!="-crf"&&v!="32"&&v!="-b:v"&&v!="0");
                    args.extend(["-b:v".into(),bitrate.into()]);
                }
            }
        },
        "audio" => { let segments=make_segments(duration,request.trim_start,request.trim_end,&request.segments)?; duration=segments.iter().map(|s|s.end-s.start).sum(); if segments.len()==1 {let s=&segments[0];args.extend(["-ss".into(),format!("{:.3}",s.start),"-t".into(),format!("{:.3}",s.end-s.start)]);} else {let n=segments.len();let mut graph=String::new();graph.push_str(&format!("[0:a:0]asplit={n}"));for i in 0..n{graph.push_str(&format!("[a{i}]"));}graph.push(';');for(i,s)in segments.iter().enumerate(){graph.push_str(&format!("[a{i}]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[aout{i}];",s.start,s.end));}for i in 0..n{graph.push_str(&format!("[aout{i}]"));}graph.push_str(&format!("concat=n={n}:v=0:a=1[aout]"));args.extend(["-filter_complex".into(),graph,"-map".into(),"[aout]".into()]);}args.extend(encode_args("audio",&format,false)?); },
        _=>return Err("Unsupported media type.".into())
    }
    args.push(target.to_string_lossy().into_owned());
    run_ffmpeg(&args,duration,app.as_ref(),&job_id)?;
    if let Some(path)=watermark_file.as_ref(){let _=fs::remove_file(path);}
    let (_,out_meta)=probe(&target)?;
    let result=OutputItem{output_id:id.clone(),filename:filename.clone(),path:target.to_string_lossy().into_owned(),media_type:media_type.clone(),metadata:out_meta};
    let manifest=json!({"output_id":id,"filename":filename,"path":target.file_name().and_then(|v|v.to_str()).unwrap_or("output"),"media_type":media_type,"metadata":result.metadata.clone()});
    fs::write(out_dir.join("meta.json"),serde_json::to_vec_pretty(&manifest).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if let Some(a)=app {let _=a.emit("cropstudio:progress",json!({"jobId":job_id.clone(),"progress":100,"stage":"Complete"}));}
    Ok(result)
}
fn detect_crop(path: &Path, start: f64, duration: f64, detector: &str, still: bool) -> Option<CropRect> {
    let vf=if detector.starts_with("subject") {"fps=2,mestimate,cropdetect=mode=mvedges:round=2:reset=0"} else {"fps=2,cropdetect=mode=black:round=2:reset=0"};
    let mut args=vec!["-hide_banner".into(),"-loglevel".into(),"info".into()];
    if !still {args.extend(["-ss".into(),format!("{start:.3}"),"-t".into(),format!("{duration:.3}")]);}
    args.extend(["-i".into(),path.to_string_lossy().into_owned(),"-an".into(),"-sn".into(),"-dn".into(),"-vf".into(),if still {vf.replace("fps=2,","")} else {vf.into()},"-frames:v".into(),if still {"1".into()}else{"120".into()},"-f".into(),"null".into(),"-".into()]);
    let output=run_capture("ffmpeg",&args).ok()?; let log=String::from_utf8_lossy(&output.stderr);
    let re=Regex::new(r"crop=(\d+):(\d+):(\d+):(\d+)").ok()?;
    let mut crops=Vec::new(); for cap in re.captures_iter(&log){let w=cap[1].parse::<i64>().ok()?;let h=cap[2].parse::<i64>().ok()?;let x=cap[3].parse::<i64>().ok()?;let y=cap[4].parse::<i64>().ok()?;crops.push(CropRect{x,y,width:w,height:h});}
    if crops.is_empty() && detector.starts_with("subject") {return detect_crop(path,start,duration,"black",still);}
    if crops.is_empty(){return None;} crops.sort_by_key(|c|c.width*c.height);
    let m=crops[crops.len()/2].clone(); Some(m)
}
fn sample_windows(duration:f64,adaptive:bool,window:f64)->Vec<(f64,f64)> { if adaptive {let mut out=Vec::new();let mut t=0.0;while t<duration && out.len()<24 {let d=window.min(duration-t).max(0.1);out.push((t,d));t+=window;}if out.is_empty(){out.push((0.0,window));}out} else {let span=duration.min(4.0).max(0.1);if duration<=span{return vec![(0.0,duration.max(0.1))];}(0..7).map(|i|{let t=i as f64*(duration-span)/6.0;(t,span)}).collect()} }
fn median_i(values:&mut [i64])->i64 {values.sort();values[values.len()/2]}
fn autocrop(root:&Path, media_id:&str, request:AutoCropRequest)->Result<Value,String>{
    if media_id.len()!=32 || !media_id.chars().all(|c|c.is_ascii_hexdigit()) { return Err("Invalid media identifier.".into()); }
    let folder=root.join("uploads").join(media_id);let meta:Value=serde_json::from_slice(&fs::read(folder.join("meta.json")).map_err(|_|"Media not found".to_string())?).map_err(|e|e.to_string())?;let path=folder.join(meta.get("stored_name").and_then(Value::as_str).ok_or("Missing media")?);let media_type=meta.get("media_type").and_then(Value::as_str).unwrap_or("video");let (_,m)=probe(&path)?;let fw=m["width"].as_i64().unwrap_or(0);let fh=m["height"].as_i64().unwrap_or(0);let duration=m["duration"].as_f64().unwrap_or(0.0);let ratio=ratio_from(&request.aspect_ratio);let adaptive=request.mode.ends_with("adaptive");let detector=if request.mode.starts_with("subject"){"subject"}else{"black"};
    let detections:Vec<(f64,f64,CropRect)>=if media_type=="image" {detect_crop(&path,0.0,0.0,detector,true).map(|c|vec![(0.0,1.0,c)]).unwrap_or_default()} else {sample_windows(duration,adaptive,request.window_seconds.clamp(2.0,120.0)).into_iter().filter_map(|(s,d)|detect_crop(&path,s,d,detector,false).map(|c|(s,s+d,c))).collect()};
    let mut crop=if detections.is_empty(){CropRect{x:0,y:0,width:fw,height:fh}}else{let mut xs:Vec<i64>=detections.iter().map(|d|d.2.x).collect();let mut ys:Vec<i64>=detections.iter().map(|d|d.2.y).collect();let mut ws:Vec<i64>=detections.iter().map(|d|d.2.width).collect();let mut hs:Vec<i64>=detections.iter().map(|d|d.2.height).collect();CropRect{x:median_i(&mut xs),y:median_i(&mut ys),width:median_i(&mut ws),height:median_i(&mut hs)}};
    crop=fit_aspect(&crop,ratio,fw,fh,media_type != "image");
    if adaptive && media_type != "image" {
        let widths:Vec<i64>=detections.iter().map(|d|d.2.width).collect();
        let heights:Vec<i64>=detections.iter().map(|d|d.2.height).collect();
        let mut w=even(if widths.is_empty(){fw}else{*widths.iter().min().unwrap()});
        let mut h=even(if heights.is_empty(){fh}else{*heights.iter().min().unwrap()});
        if let Some(r)=ratio {if w as f64/h as f64>r{w=even((h as f64*r) as i64)}else{h=even((w as f64/r) as i64)}}
        w=w.min(even(fw));h=h.min(even(fh));
        // Smooth adjacent crop centers to avoid abrupt jumps between sampled windows.
        let mut previous_x:Option<f64>=None;let mut previous_y:Option<f64>=None;
        let windows=detections.iter().map(|(s,e,c)|{
            let raw_x=clamp_i(c.x,0,(fw-w).max(0)) as f64;
            let raw_y=clamp_i(c.y,0,(fh-h).max(0)) as f64;
            let x=previous_x.map(|p|(p*0.68+raw_x*0.32).round() as i64).unwrap_or(raw_x as i64);
            let y=previous_y.map(|p|(p*0.68+raw_y*0.32).round() as i64).unwrap_or(raw_y as i64);
            previous_x=Some(x as f64);previous_y=Some(y as f64);
            json!({"start":s,"end":e,"x":clamp_i(x,0,(fw-w).max(0)),"y":clamp_i(y,0,(fh-h).max(0))})
        }).collect::<Vec<_>>();
        let plan=json!({"width":w,"height":h,"windows":if windows.is_empty(){vec![json!({"start":0.0,"end":999999.0,"x":0,"y":0})]}else{windows}});
        return Ok(json!({"kind":"adaptive","crop":crop,"adaptivePlan":plan,"sampleCount":detections.len(),"tracking":"motion-smoothed"}));
    }
    Ok(json!({"kind":"static","crop":crop,"sampleCount":detections.len()}))
}

#[tauri::command]
async fn import_media(paths: Vec<String>, state: State<'_,AppState>) -> Result<Vec<MediaItem>,String> {
    let root=state.root.clone();
    tokio::task::spawn_blocking(move || {
        let mut items=Vec::new();
        for raw in paths {
            let src=PathBuf::from(&raw);
            if !src.is_file(){continue;}
            let name=src.file_name().and_then(|v|v.to_str()).unwrap_or("media").to_string();
            let id=Uuid::new_v4().simple().to_string();
            let folder=root.join("uploads").join(&id);
            fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
            let dest=folder.join(&name);
            fs::copy(&src,&dest).map_err(|e|format!("Could not copy {name}: {e}"))?;
            let (kind,metadata)=probe(&dest)?;
            let thumbnail_path=generate_thumbnail(&dest,&folder,&kind,&metadata);
            let duration=metadata.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
            let filmstrip=if kind=="video"{generate_filmstrip(&dest,&folder,duration)}else{Vec::new()};
            let record=json!({
                "media_id":id.clone(),"original_name":name.clone(),"stored_name":name.clone(),
                "media_type":kind.clone(),"metadata":metadata.clone(),"thumbnail_path":thumbnail_path,
                "filmstrip":filmstrip
            });
            fs::write(folder.join("meta.json"),serde_json::to_vec_pretty(&record).map_err(|e|e.to_string())?)
                .map_err(|e|e.to_string())?;
            items.push(MediaItem{
                media_id:id,filename:name,path:dest.to_string_lossy().into_owned(),
                media_type:kind,metadata,thumbnail_path,filmstrip
            });
        }
        Ok(items)
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn restore_media(paths:Vec<String>,state:State<'_,AppState>)->Result<Vec<MediaItem>,String>{
    let _root=state.root.clone();
    tokio::task::spawn_blocking(move||{
        let mut items=Vec::new();
        for raw in paths {
            let path=PathBuf::from(raw);
            if !path.is_file(){continue;}
            let folder=path.parent().ok_or("Invalid stored path")?;
            let meta_path=folder.join("meta.json");
            let mut meta:Value=serde_json::from_slice(&fs::read(&meta_path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            let id=meta["media_id"].as_str().unwrap_or_default().to_string();
            let (kind,metadata)=probe(&path)?;
            let thumbnail_path=meta.get("thumbnail_path").and_then(Value::as_str)
                .map(PathBuf::from).filter(|p|p.is_file())
                .map(|p|p.to_string_lossy().into_owned())
                .or_else(||generate_thumbnail(&path,folder,&kind,&metadata));
            meta["thumbnail_path"]=thumbnail_path.clone().map(Value::String).unwrap_or(Value::Null);
            let filmstrip=meta.get("filmstrip").and_then(Value::as_array).map(|a|a.iter().filter_map(Value::as_str).map(str::to_string).filter(|p|Path::new(p).is_file()).collect::<Vec<_>>()).filter(|a|!a.is_empty())
                .unwrap_or_else(||if kind=="video"{generate_filmstrip(&path,folder,metadata.get("duration").and_then(Value::as_f64).unwrap_or(0.0))}else{Vec::new()});
            meta["filmstrip"]=json!(filmstrip.clone());
            let _=fs::write(&meta_path,serde_json::to_vec_pretty(&meta).unwrap_or_default());
            items.push(MediaItem{
                media_id:id,
                filename:meta["original_name"].as_str().unwrap_or("media").to_string(),
                path:path.to_string_lossy().into_owned(),
                media_type:kind,metadata,thumbnail_path,filmstrip
            });
        }
        Ok(items)
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn process_media(app:AppHandle,state:State<'_,AppState>,request:ProcessRequest)->Result<OutputItem,String>{let root=state.root.clone();let app2=app.clone();let job=Uuid::new_v4().simple().to_string();tokio::task::spawn_blocking(move||process_one(&root,request,Some(app2),job)).await.map_err(|e|e.to_string())?}
#[tauri::command]
async fn auto_crop(state:State<'_,AppState>,media_id:String,request:AutoCropRequest)->Result<Value,String>{let root=state.root.clone();tokio::task::spawn_blocking(move||autocrop(&root,&media_id,request)).await.map_err(|e|e.to_string())?}
#[tauri::command]
async fn create_bundle(state:State<'_,AppState>,paths:Vec<String>)->Result<String,String>{
    if paths.is_empty(){return Err("No completed outputs to bundle.".into());}
    let root=state.root.clone();
    tokio::task::spawn_blocking(move||{
        let out=root.join("outputs").join(format!("batch-{}.zip",Uuid::new_v4().simple()));
        let file=File::create(&out).map_err(|e|e.to_string())?;
        let mut archive=zip::ZipWriter::new(file);
        let mut used=HashSet::new();
        for raw in paths {
            let path=PathBuf::from(raw); if !path.is_file(){continue;}
            let mut name=path.file_name().and_then(|v|v.to_str()).unwrap_or("output").to_string();
            let base=Path::new(&name).file_stem().and_then(|v|v.to_str()).unwrap_or("output").to_string();
            let ext=Path::new(&name).extension().and_then(|v|v.to_str()).unwrap_or("").to_string();
            let mut n=2; while !used.insert(name.to_lowercase()){name=if ext.is_empty(){format!("{base}-{n}")}else{format!("{base}-{n}.{ext}")};n+=1;}
            archive.start_file(name,FileOptions::default().compression_method(zip::CompressionMethod::Stored)).map_err(|e|e.to_string())?;
            let mut input=File::open(path).map_err(|e|e.to_string())?;
            std::io::copy(&mut input,&mut archive).map_err(|e|e.to_string())?;
        }
        archive.finish().map_err(|e|e.to_string())?;
        Ok(out.to_string_lossy().into_owned())
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn clear_media(state:State<'_,AppState>,media_ids:Vec<String>)->Result<(),String>{
    for id in media_ids { if !id.chars().all(|c|c.is_ascii_hexdigit()) { continue; } let dir=state.root.join("uploads").join(id); if dir.exists(){fs::remove_dir_all(dir).map_err(|e|e.to_string())?;} }
    Ok(())
}
#[tauri::command]
async fn copy_file(source:String,destination:String)->Result<(),String>{
    tokio::task::spawn_blocking(move||{let src=PathBuf::from(source);let dst=PathBuf::from(destination);if !src.is_file(){return Err("Export file no longer exists.".into());}if let Some(parent)=dst.parent(){fs::create_dir_all(parent).map_err(|e|e.to_string())?;}fs::copy(src,dst).map_err(|e|e.to_string())?;Ok(())}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn engine_status()->Value{let ffmpeg=Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s|s.success()).unwrap_or(false);let ffprobe=Command::new("ffprobe").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s|s.success()).unwrap_or(false);json!({"ffmpeg":ffmpeg,"ffprobe":ffprobe,"ready":ffmpeg&&ffprobe})}
#[tauri::command]
fn reveal_file(path:String)->Result<(),String>{let p=PathBuf::from(path);if !p.exists(){return Err("File does not exist.".into());}#[cfg(target_os="windows")] {Command::new("explorer").arg("/select,").arg(p).spawn().map_err(|e|e.to_string())?;}#[cfg(target_os="macos")] {Command::new("open").arg("-R").arg(p).spawn().map_err(|e|e.to_string())?;}#[cfg(target_os="linux")] {Command::new("xdg-open").arg(p.parent().unwrap_or(Path::new("."))).spawn().map_err(|e|e.to_string())?;}Ok(())}

fn main(){tauri::Builder::default().plugin(tauri_plugin_dialog::init()).setup(|app|{let root=app.path().app_data_dir()?;fs::create_dir_all(root.join("uploads"))?;fs::create_dir_all(root.join("outputs"))?;app.manage(AppState{root});Ok(())}).invoke_handler(tauri::generate_handler![import_media,restore_media,process_media,auto_crop,create_bundle,copy_file,clear_media,engine_status,reveal_file]).run(tauri::generate_context!()).expect("error while running CropStudio");}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_is_clamped_inside_source_frame() {
        let crop = normalize_crop(&CropRect { x: 900, y: -20, width: 400, height: 300 }, 1280, 720, true);
        assert_eq!(crop.x, 880);
        assert_eq!(crop.y, 0);
        assert_eq!(crop.width, 400);
        assert_eq!(crop.height, 300);
    }

    #[test]
    fn aspect_fit_preserves_requested_ratio() {
        let crop = fit_aspect(&CropRect { x: 0, y: 0, width: 1000, height: 500 }, Some(1.0), 1920, 1080, false);
        assert_eq!(crop.width, crop.height);
    }

    #[test]
    fn trim_segments_are_sorted_and_validated() {
        let ranges = vec![Segment { start: 2.0, end: 3.0 }, Segment { start: 0.0, end: 1.0 }];
        let sorted = make_segments(5.0, 0.0, Some(5.0), &ranges).unwrap();
        assert_eq!(sorted[0].start, 0.0);
        assert_eq!(sorted[1].start, 2.0);
        assert!(make_segments(5.0, 2.0, Some(1.0), &[]).is_err());
    }
}

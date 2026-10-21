//! Clementine conversion engine. Std-only so it can be tested without the Tauri
//! toolchain. It drives well-known open-source tools:
//!   ffmpeg (+ffprobe)  images, audio, video, subtitles
//!   pandoc             documents
//!   LibreOffice        office formats, images/SVG to PDF, PDF import
//!   Ghostscript        PDF rendering, text, merge, split, compress
//!   7-Zip              archives

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ------------------------------------------------------------------ kinds ---

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Kind {
    Image,
    Video,
    Audio,
    Doc,
    Slides,
    Sheet,
    Pdf,
    Subs,
    Archive,
    Folder,
    Other,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Doc => "document",
            Kind::Slides => "slides",
            Kind::Sheet => "spreadsheet",
            Kind::Pdf => "pdf",
            Kind::Subs => "subtitles",
            Kind::Archive => "archive",
            Kind::Folder => "folder",
            Kind::Other => "other",
        }
    }
}

pub fn ext_of(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

pub fn kind_of(ext: &str) -> Kind {
    match ext {
        "jpg" | "jpeg" | "jfif" | "png" | "webp" | "heic" | "heif" | "tif" | "tiff" | "avif"
        | "bmp" | "gif" | "ico" | "svg" => Kind::Image,
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "m4v" | "flv" | "mpg" | "mpeg" | "3gp"
        | "ts" | "mts" => Kind::Video,
        "mp3" | "m4a" | "wav" | "flac" | "ogg" | "oga" | "opus" | "aiff" | "aif" | "wma" | "aac"
        | "amr" => Kind::Audio,
        "docx" | "doc" | "md" | "markdown" | "txt" | "epub" | "rtf" | "html" | "htm" | "odt" => {
            Kind::Doc
        }
        "ppt" | "pptx" | "odp" => Kind::Slides,
        "xlsx" | "xls" | "ods" | "csv" | "tsv" => Kind::Sheet,
        "pdf" => Kind::Pdf,
        "srt" | "vtt" | "ass" | "ssa" => Kind::Subs,
        "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" | "bz2" | "xz" => Kind::Archive,
        _ => Kind::Other,
    }
}

pub fn kind_of_path(p: &Path) -> Kind {
    if p.is_dir() {
        Kind::Folder
    } else {
        kind_of(&ext_of(p))
    }
}

/// Canonical spelling so "jpeg" and "jpg" compare equal.
pub fn canon(ext: &str) -> String {
    match ext {
        "jpeg" | "jfif" => "jpg",
        "tif" => "tiff",
        "htm" => "html",
        "aif" => "aiff",
        "oga" => "ogg",
        "markdown" => "md",
        "tgz" => "tar.gz",
        e => e,
    }
    .to_string()
}

const IMG_OUT: [&str; 9] = ["jpg", "png", "webp", "tiff", "avif", "bmp", "gif", "ico", "pdf"];
const AUDIO_OUT: [&str; 8] = ["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff", "wma"];
const VIDEO_OUT: [&str; 13] = [
    "mp4", "webm", "mov", "mkv", "avi", "wmv", "gif", "mp3", "m4a", "wav", "flac", "ogg", "opus",
];
const DOC_OUT: [&str; 8] = ["docx", "pdf", "md", "txt", "html", "epub", "rtf", "odt"];
const ARCHIVE_OUT: [&str; 5] = ["zip", "tar", "tar.gz", "tar.xz", "7z"];

/// Every format a file of this kind and extension can be converted to.
/// This is the single source of truth; the UI asks the backend for it.
pub fn targets_for(kind: Kind, ext: &str) -> Vec<&'static str> {
    let e = canon(ext);
    let list: Vec<&'static str> = match kind {
        Kind::Image => match e.as_str() {
            "svg" => vec!["png", "jpg", "pdf"],
            "gif" => vec!["jpg", "png", "webp", "tiff", "avif", "bmp", "ico", "pdf", "mp4", "webm"],
            "ico" => vec!["png", "jpg", "webp", "bmp"],
            "heic" | "heif" => vec!["jpg", "png", "webp", "tiff", "avif", "bmp", "gif", "pdf"],
            _ => IMG_OUT.to_vec(),
        },
        Kind::Video => VIDEO_OUT.to_vec(),
        Kind::Audio => AUDIO_OUT.to_vec(),
        Kind::Doc => DOC_OUT.to_vec(),
        Kind::Slides => vec!["pdf", "pptx", "odp", "jpg", "png"],
        Kind::Sheet => vec!["xlsx", "ods", "csv", "tsv", "pdf", "html"],
        Kind::Pdf => vec!["docx", "pptx", "xlsx", "odt", "epub", "txt", "md", "jpg", "png"],
        Kind::Subs => vec!["srt", "vtt", "txt"],
        Kind::Archive => ARCHIVE_OUT.to_vec(),
        Kind::Folder => vec!["zip", "7z", "tar", "tar.gz"],
        Kind::Other => vec![],
    };
    list.into_iter().filter(|t| *t != e).collect()
}

/// Every file extension the engine recognises.
pub const KNOWN_EXTS: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "webp", "heic", "heif", "tif", "tiff", "avif", "bmp", "gif", "ico",
    "svg", "mp4", "mov", "mkv", "webm", "avi", "wmv", "m4v", "flv", "mpg", "mpeg", "3gp", "ts", "mts",
    "mp3", "m4a", "wav", "flac", "ogg", "oga", "opus", "aiff", "aif", "wma", "aac", "amr", "docx",
    "doc", "md", "markdown", "txt", "epub", "rtf", "html", "htm", "odt", "ppt", "pptx", "odp", "xlsx",
    "xls", "ods", "csv", "tsv", "pdf", "srt", "vtt", "ass", "ssa", "zip", "tar", "gz", "tgz", "7z",
    "rar", "bz2", "xz",
];

/// Total number of (input format -> output format) pairs Clementine offers.
pub fn conversion_option_count() -> usize {
    KNOWN_EXTS
        .iter()
        .map(|e| targets_for(kind_of(e), e).len())
        .sum()
}

// ------------------------------------------------------------ small utils ---

fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

fn owned(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(|x| x.as_str()).collect()
}

fn stem_of(p: &Path) -> String {
    let mut stem = p
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("output")
        .to_string();
    if stem.to_lowercase().ends_with(".tar") {
        stem.truncate(stem.len() - 4);
    }
    stem
}

/// Next free file name in `dir`: "name.ext", "name (1).ext", ...
pub fn unique_in(dir: &Path, stem: &str, suffix: &str, ext: &str) -> PathBuf {
    let mut n = 0;
    loop {
        let name = if n == 0 {
            format!("{stem}{suffix}.{ext}")
        } else {
            format!("{stem}{suffix} ({n}).{ext}")
        };
        let p = dir.join(name);
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

pub fn unique_output(input: &Path, suffix: &str, ext: &str) -> PathBuf {
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    unique_in(dir, &stem_of(input), suffix, ext)
}

/// A folder name that does not exist yet next to `input`.
pub fn unique_dir(input: &Path, suffix: &str) -> PathBuf {
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = stem_of(input);
    let mut n = 0;
    loop {
        let name = if n == 0 {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}{suffix} ({n})")
        };
        let p = dir.join(name);
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

/// Scratch folder that is deleted when dropped.
pub struct TmpDir(PathBuf);

impl TmpDir {
    pub fn new(tag: &str) -> Result<TmpDir, String> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "clementine-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(TmpDir(p))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn first_file_in(dir: &Path) -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    v.sort();
    v.into_iter().next()
}

fn ps_escape(p: &Path) -> String {
    s(p).replace('\\', "/").replace('(', "\\(").replace(')', "\\)")
}

// ----------------------------------------------------------- tool lookup ---

fn exe_name(n: &str) -> String {
    if cfg!(windows) {
        format!("{n}.exe")
    } else {
        n.to_string()
    }
}

fn which(n: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe_name(n)))
        .find(|p| p.is_file())
}

fn env_path(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).map(PathBuf::from)
}

/// Last (highest sorted) sub-folder of `base` whose name starts with `prefix`.
fn scan(base: &Path, prefix: &str) -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(base)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(prefix))
                    .unwrap_or(false)
        })
        .collect();
    v.sort();
    v.pop()
}

fn windows_candidates(id: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    let local = env_path("LOCALAPPDATA");
    let mut roots: Vec<PathBuf> = Vec::new();
    roots.extend(env_path("ProgramFiles"));
    roots.extend(env_path("ProgramFiles(x86)"));
    match id {
        "ffmpeg" | "ffprobe" => {
            if let Some(l) = &local {
                v.push(l.join("Microsoft/WinGet/Links").join(exe_name(id)));
                if let Some(pkg) = scan(&l.join("Microsoft/WinGet/Packages"), "Gyan.FFmpeg") {
                    if let Some(b) = scan(&pkg, "ffmpeg-") {
                        v.push(b.join("bin").join(exe_name(id)));
                    }
                }
            }
            for r in &roots {
                v.push(r.join("ffmpeg/bin").join(exe_name(id)));
            }
            v.push(PathBuf::from(r"C:\ffmpeg\bin").join(exe_name(id)));
        }
        "pandoc" => {
            if let Some(l) = &local {
                v.push(l.join("Pandoc/pandoc.exe"));
            }
            for r in &roots {
                v.push(r.join("Pandoc/pandoc.exe"));
            }
        }
        "soffice" => {
            for r in &roots {
                v.push(r.join("LibreOffice/program/soffice.exe"));
            }
        }
        "gs" => {
            for r in &roots {
                if let Some(d) = scan(&r.join("gs"), "gs") {
                    v.push(d.join("bin/gswin64c.exe"));
                    v.push(d.join("bin/gswin32c.exe"));
                }
            }
        }
        "7z" => {
            for r in &roots {
                v.push(r.join("7-Zip/7z.exe"));
            }
        }
        _ => {}
    }
    v
}

/// Find a helper program: bundled `tools` folder, then PATH, then the usual
/// Windows install folders (so a fresh winget install works without a restart).
pub fn find_tool(id: &str) -> Option<PathBuf> {
    let names: Vec<&str> = match id {
        "gs" => vec!["gs", "gswin64c", "gswin32c"],
        "7z" => vec!["7z", "7zz", "7za"],
        "soffice" => vec!["soffice", "libreoffice"],
        other => vec![other],
    };
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
        for sub in ["tools", "resources/tools", "."] {
            for n in &names {
                let p = dir.join(sub).join(exe_name(n));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    for n in &names {
        if let Some(p) = which(n) {
            return Some(p);
        }
    }
    windows_candidates(id).into_iter().find(|p| p.is_file())
}

fn tool_label(id: &str) -> &'static str {
    match id {
        "ffmpeg" => "FFmpeg",
        "ffprobe" => "FFmpeg",
        "pandoc" => "Pandoc",
        "soffice" => "LibreOffice",
        "gs" => "Ghostscript",
        "7z" => "7-Zip",
        _ => "tool",
    }
}

fn engine(id: &str) -> Result<String, String> {
    match find_tool(id) {
        Some(p) => Ok(s(&p)),
        None => Err(format!(
            "{} is not installed. Click its badge at the top of the window to install it.",
            tool_label(id)
        )),
    }
}

pub struct ToolInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub found: bool,
    pub winget: &'static str,
    pub used_for: &'static str,
}

pub fn tool_status() -> Vec<ToolInfo> {
    let rows = [
        ("ffmpeg", "FFmpeg", "Gyan.FFmpeg", "Images, audio, video, subtitles"),
        ("pandoc", "Pandoc", "JohnMacFarlane.Pandoc", "Markdown, DOCX, EPUB, HTML"),
        ("soffice", "LibreOffice", "TheDocumentFoundation.LibreOffice", "Office files, PDF import/export"),
        ("gs", "Ghostscript", "ArtifexSoftware.GhostScript", "PDF pages, text, merge, split"),
        ("7z", "7-Zip", "7zip.7zip", "Archives"),
    ];
    rows.iter()
        .map(|(id, label, winget, used_for)| ToolInfo {
            id,
            label,
            found: find_tool(id).is_some(),
            winget,
            used_for,
        })
        .collect()
}

/// Install a helper tool with winget (Windows 10/11).
pub fn install_tool(id: &str) -> Result<String, String> {
    let info = tool_status()
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Unknown tool: {id}"))?;
    if !cfg!(windows) {
        return Err("Automatic install is only available on Windows (it uses winget)".to_string());
    }
    run_capture(
        "winget",
        &[
            "install".into(),
            "-e".into(),
            "--id".into(),
            info.winget.into(),
            "--silent".into(),
            "--accept-package-agreements".into(),
            "--accept-source-agreements".into(),
        ],
    )
    .map(|_| format!("{} installed", info.label))
}

// ---------------------------------------------------------------- running ---

fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

fn run_capture(program: &str, args: &[String]) -> Result<String, String> {
    let out = command(program)
        .args(args)
        .output()
        .map_err(|e| format!("Could not start {program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let mut lines: Vec<&str> = err.lines().rev().take(6).collect();
        lines.reverse();
        let name = Path::new(program)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or(program);
        Err(format!("{name} failed:\n{}", lines.join("\n")))
    }
}

fn run(program: &str, args: &[String]) -> Result<(), String> {
    run_capture(program, args).map(|_| ())
}

fn ffmpeg(args: &[String]) -> Result<(), String> {
    run(&engine("ffmpeg")?, args)
}

fn ffprobe(args: &[&str]) -> Option<String> {
    let bin = find_tool("ffprobe").or_else(|| {
        find_tool("ffmpeg").and_then(|f| {
            let p = f.with_file_name(exe_name("ffprobe"));
            p.is_file().then_some(p)
        })
    })?;
    run_capture(&s(&bin), &owned(args)).ok()
}

fn has_audio(input: &Path) -> bool {
    match ffprobe(&["-v", "error", "-select_streams", "a", "-show_entries", "stream=codec_type", "-of", "csv=p=0", &s(input)]) {
        Some(o) => !o.trim().is_empty(),
        None => true,
    }
}

fn duration_of(input: &Path) -> Result<f64, String> {
    ffprobe(&["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", &s(input)])
        .and_then(|o| o.trim().parse::<f64>().ok())
        .ok_or_else(|| "Could not read the duration of this file".to_string())
}

// ------------------------------------------------------------ ffmpeg args ---

fn ff_args(input: &Path, pre: &[&str], post: &[&str], out: &Path) -> Vec<String> {
    let mut a = owned(&["-y", "-hide_banner", "-loglevel", "error"]);
    a.extend(owned(pre));
    a.push("-i".to_string());
    a.push(s(input));
    a.extend(owned(post));
    a.push(s(out));
    a
}

fn ffmpeg_with(input: &Path, suffix: &str, ext: &str, pre: &[&str], post: &[&str]) -> Result<PathBuf, String> {
    let out = unique_output(input, suffix, ext);
    ffmpeg(&ff_args(input, pre, post, &out))?;
    Ok(out)
}

fn audio_codec(target: &str) -> Option<Vec<&'static str>> {
    match target {
        "mp3" => Some(vec!["-c:a", "libmp3lame", "-q:a", "2"]),
        "m4a" => Some(vec!["-c:a", "aac", "-b:a", "192k"]),
        "ogg" => Some(vec!["-c:a", "libvorbis", "-q:a", "5"]),
        "opus" => Some(vec!["-c:a", "libopus", "-b:a", "128k"]),
        "wma" => Some(vec!["-c:a", "wmav2"]),
        "wav" | "flac" | "aiff" => Some(vec![]),
        _ => None,
    }
}

/// Encoder options for converting `kind` media to `target`.
pub fn codec_args(kind: Kind, target: &str) -> Vec<&'static str> {
    if matches!(kind, Kind::Audio | Kind::Video) {
        if let Some(a) = audio_codec(target) {
            let mut v = vec!["-vn"];
            v.extend(a);
            return v;
        }
    }
    match (kind, target) {
        (Kind::Video, "gif") => vec!["-vf", "fps=12,scale=480:-1:flags=lanczos"],
        (Kind::Video, "mp4") | (Kind::Video, "mov") | (Kind::Video, "m4v") => vec![
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-movflags", "+faststart",
        ],
        (Kind::Video, "webm") => vec!["-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0", "-c:a", "libopus"],
        (Kind::Video, "avi") => vec!["-c:v", "mpeg4", "-q:v", "4", "-c:a", "libmp3lame"],
        (Kind::Video, "wmv") => vec!["-c:v", "wmv2", "-c:a", "wmav2"],
        (Kind::Image, "jpg") => vec!["-frames:v", "1", "-q:v", "2"],
        (Kind::Image, "ico") => vec![
            "-frames:v", "1", "-vf",
            "scale=256:256:force_original_aspect_ratio=decrease,pad=256:256:(ow-iw)/2:(oh-ih)/2:color=0x00000000,format=rgba",
        ],
        (Kind::Image, "mp4") => vec![
            "-movflags", "+faststart", "-pix_fmt", "yuv420p", "-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2",
        ],
        (Kind::Image, "webm") => vec!["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "34", "-pix_fmt", "yuv420p"],
        (Kind::Image, "gif") => vec![],
        (Kind::Image, _) => vec!["-frames:v", "1"],
        _ => vec![],
    }
}

/// Full ffmpeg argument list for converting `input` into `target`.
pub fn media_args(kind: Kind, input: &Path, target: &str, output: &Path) -> Vec<String> {
    ff_args(input, &[], &codec_args(kind, target), output)
}

fn frame_args(ext: &str) -> Vec<&'static str> {
    if canon(ext) == "gif" {
        vec![]
    } else {
        vec!["-frames:v", "1"]
    }
}

// ------------------------------------------------------------- conversion ---

pub fn convert(input: &Path, target: &str) -> Result<PathBuf, String> {
    if !input.exists() {
        return Err(format!("{} no longer exists", input.display()));
    }
    let target = target.to_lowercase();
    let kind = kind_of_path(input);
    let src = ext_of(input);
    if canon(&src) == target {
        return Err(format!("Already a .{target} file"));
    }
    if !targets_for(kind, &src).contains(&target.as_str()) {
        return Err(format!("Cannot convert {} to .{target}", if src.is_empty() { kind.name().to_string() } else { format!(".{src}") }));
    }
    match kind {
        Kind::Image => convert_image(input, &src, &target),
        Kind::Video | Kind::Audio => {
            let out = unique_output(input, "", &target);
            ffmpeg(&media_args(kind, input, &target, &out))?;
            Ok(out)
        }
        Kind::Subs => convert_subs(input, &src, &target),
        Kind::Doc => convert_doc(input, &src, &target),
        Kind::Sheet => {
            let out = unique_output(input, "", &target);
            soffice_to(input, &lo_filter(&target), None, &out)?;
            Ok(out)
        }
        Kind::Slides => convert_slides(input, &target),
        Kind::Pdf => convert_pdf(input, &target),
        Kind::Archive => {
            let out = unique_output(input, "", &target);
            convert_archive(input, &target, &out)?;
            Ok(out)
        }
        Kind::Folder => convert_folder(input, &target),
        Kind::Other => Err(format!("No conversions for .{src} files yet")),
    }
}

fn lo_filter(target: &str) -> String {
    match target {
        "tsv" => "csv:Text - txt - csv (StarCalc):9,34,76,1,,0,false".to_string(),
        "txt" => "txt:Text".to_string(),
        "html" => "html".to_string(),
        t => t.to_string(),
    }
}

fn convert_image(input: &Path, src: &str, target: &str) -> Result<PathBuf, String> {
    let heic = matches!(canon(src).as_str(), "heic" | "heif");
    if canon(src) == "svg" || target == "pdf" {
        let out = unique_output(input, "", target);
        if heic {
            // LibreOffice cannot read HEIC: go through a PNG first.
            let tmp = TmpDir::new("heic")?;
            let png = tmp.path().join(format!("{}.png", stem_of(input)));
            decode_heic(input, &png)?;
            soffice_to(&png, "pdf", None, &out)?;
        } else {
            soffice_to(input, &lo_filter(target), None, &out)?;
        }
        return Ok(out);
    }
    let out = unique_output(input, "", target);
    if heic {
        decode_heic(input, &out)?;
        return Ok(out);
    }
    ffmpeg(&media_args(Kind::Image, input, target, &out))?;
    Ok(out)
}

/// HEIC/HEIF: ImageMagick when available (best support), otherwise FFmpeg.
fn decode_heic(input: &Path, out: &Path) -> Result<(), String> {
    if let Some(m) = find_tool("magick") {
        if run(&s(&m), &[s(input), s(out)]).is_ok() {
            return Ok(());
        }
    }
    let target = ext_of(out);
    ffmpeg(&media_args(Kind::Image, input, &target, out))
        .map_err(|e| format!("{e}\n(HEIC needs an FFmpeg build with HEIF support, or ImageMagick installed.)"))
}

/// Strip numbering, timestamps and headers, leaving only the spoken text.
pub fn strip_subs(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for raw in text.lines() {
        let l = raw.trim().trim_start_matches('\u{feff}');
        if l.is_empty() || l.starts_with("WEBVTT") || l.contains("-->") {
            continue;
        }
        if l.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        lines.push(l);
    }
    lines.join("\n") + "\n"
}

fn convert_subs(input: &Path, src: &str, target: &str) -> Result<PathBuf, String> {
    let out = unique_output(input, "", target);
    match target {
        "srt" | "vtt" => ffmpeg(&media_args(Kind::Subs, input, target, &out))?,
        "txt" => {
            let text = if matches!(src, "ass" | "ssa") {
                let tmp = TmpDir::new("subs")?;
                let srt = tmp.path().join("x.srt");
                ffmpeg(&media_args(Kind::Subs, input, "srt", &srt))?;
                fs::read_to_string(&srt).map_err(|e| e.to_string())?
            } else {
                fs::read_to_string(input).map_err(|e| e.to_string())?
            };
            fs::write(&out, strip_subs(&text)).map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Cannot convert .{src} to .{target}")),
    }
    Ok(out)
}

// -- LibreOffice ---------------------------------------------------------------

/// LibreOffice cannot run two conversions against one profile at once.
static LO_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn soffice_run(input: &Path, filter: &str, infilter: Option<&str>, outdir: &Path) -> Result<PathBuf, String> {
    let bin = engine("soffice")?;
    let _guard = LO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let profile = std::env::temp_dir().join("clementine-lo-profile");
    // file:// URL: forward slashes, and spaces (common in Windows user names) encoded.
    let p = s(&profile).replace('\\', "/").replace('%', "%25").replace(' ', "%20");
    let mut args = vec![
        format!("-env:UserInstallation=file:///{}", p.trim_start_matches('/')),
        "--headless".to_string(),
        "--norestore".to_string(),
    ];
    if let Some(f) = infilter {
        args.push(format!("--infilter={f}"));
    }
    args.extend(owned(&["--convert-to", filter, "--outdir", &s(outdir)]));
    args.push(s(input));
    run(&bin, &args)?;
    first_file_in(outdir).ok_or_else(|| "LibreOffice produced no output file".to_string())
}

fn soffice_to(input: &Path, filter: &str, infilter: Option<&str>, out: &Path) -> Result<(), String> {
    let tmp = TmpDir::new("lo")?;
    let produced = soffice_run(input, filter, infilter, tmp.path())?;
    fs::copy(&produced, out).map(|_| ()).map_err(|e| e.to_string())
}

// -- Documents -------------------------------------------------------------------

fn pandoc_to(input: &Path, target: &str, out: &Path) -> Result<(), String> {
    let mut args = vec![s(input), "-o".to_string(), s(out)];
    match target {
        "md" => args.extend(owned(&["-t", "gfm"])),
        "txt" => args.extend(owned(&["-t", "plain"])),
        "html" => args.push("-s".to_string()),
        _ => {}
    }
    run(&engine("pandoc")?, &args)
}

fn convert_doc(input: &Path, src: &str, target: &str) -> Result<PathBuf, String> {
    let out = unique_output(input, "", target);
    let src = canon(src);
    // Formats pandoc cannot read go through LibreOffice first.
    let lo_source = matches!(src.as_str(), "doc" | "rtf");
    let lo_direct = matches!(target, "docx" | "odt" | "rtf" | "txt" | "html" | "pdf");
    if target == "pdf" {
        if matches!(src.as_str(), "md" | "epub") {
            let tmp = TmpDir::new("doc")?;
            let docx = tmp.path().join(format!("{}.docx", stem_of(input)));
            pandoc_to(input, "docx", &docx)?;
            soffice_to(&docx, "pdf", None, &out)?;
        } else {
            soffice_to(input, "pdf", None, &out)?;
        }
    } else if lo_source && lo_direct {
        soffice_to(input, &lo_filter(target), None, &out)?;
    } else if lo_source {
        let tmp = TmpDir::new("doc")?;
        let docx = soffice_run(input, "docx", None, tmp.path())?;
        pandoc_to(&docx, target, &out)?;
    } else {
        pandoc_to(input, target, &out)?;
    }
    Ok(out)
}

// -- Slides ----------------------------------------------------------------------

fn convert_slides(input: &Path, target: &str) -> Result<PathBuf, String> {
    match target {
        "jpg" | "png" => {
            let tmp = TmpDir::new("slides")?;
            let pdf = soffice_run(input, "pdf", None, tmp.path())?;
            let dir = unique_dir(input, "-slides");
            pdf_to_images(&pdf, target, &dir, "slide")?;
            Ok(dir)
        }
        t => {
            let out = unique_output(input, "", t);
            soffice_to(input, t, None, &out)?;
            Ok(out)
        }
    }
}

// -- PDF -------------------------------------------------------------------------

fn gs(args: &[String]) -> Result<(), String> {
    let mut a = owned(&["-dBATCH", "-dNOPAUSE", "-dSAFER", "-q"]);
    a.extend_from_slice(args);
    run(&engine("gs")?, &a)
}

pub fn pdf_page_count(input: &Path) -> Result<u32, String> {
    let out = run_capture(
        &engine("gs")?,
        &[
            "-q".into(),
            "-dNODISPLAY".into(),
            "-dNOSAFER".into(),
            "-c".into(),
            format!("({}) (r) file runpdfbegin pdfpagecount = quit", ps_escape(input)),
        ],
    )?;
    out.trim()
        .parse::<u32>()
        .map_err(|_| "Could not read the page count (is this a valid, unencrypted PDF?)".to_string())
}

fn pdf_to_images(input: &Path, fmt: &str, dir: &Path, prefix: &str) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut a = if fmt == "jpg" {
        vec!["-sDEVICE=jpeg".to_string(), "-dJPEGQ=92".to_string()]
    } else {
        vec!["-sDEVICE=png16m".to_string()]
    };
    a.push("-r300".to_string());
    a.push(format!("-sOutputFile={}", s(&dir.join(format!("{prefix}-%03d.{fmt}")))));
    a.push(s(input));
    gs(&a)
}

/// Plain text of a PDF with its column layout preserved (Ghostscript txtwrite).
fn pdf_text_raw(input: &Path) -> Result<String, String> {
    let tmp = TmpDir::new("pdftext")?;
    let f = tmp.path().join("t.txt");
    gs(&["-sDEVICE=txtwrite".to_string(), format!("-sOutputFile={}", s(&f)), s(input)])?;
    let bytes = fs::read(&f).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Normalise line endings, drop the common left margin and runs of blank lines.
pub fn tidy_text(raw: &str, flush_left: bool) -> String {
    let lines: Vec<&str> = raw.lines().map(|l| l.trim_end()).collect();
    let margin = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = Vec::new();
    for l in lines {
        let l = if flush_left { l.trim_start().to_string() } else { l.chars().skip(margin).collect::<String>() };
        if l.is_empty() && out.last().map(|x| x.is_empty()).unwrap_or(true) {
            continue;
        }
        out.push(l);
    }
    while out.last().map(|x| x.is_empty()).unwrap_or(false) {
        out.pop();
    }
    out.join("\n") + "\n"
}

/// Split a text line into table cells wherever there is a gap of 2+ spaces (or a tab).
pub fn split_cells(line: &str) -> Vec<String> {
    let mut cells: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut spaces = 0;
    for c in line.trim().chars() {
        if c == ' ' {
            spaces += 1;
        } else if c == '\t' {
            spaces += 2;
        } else {
            if spaces >= 2 && !cur.is_empty() {
                cells.push(std::mem::take(&mut cur));
            } else if spaces == 1 {
                cur.push(' ');
            }
            spaces = 0;
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        cells.push(cur);
    }
    cells
}

fn csv_field(v: &str) -> String {
    if v.contains(',') || v.contains('"') || v.contains('\n') {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

/// Rows of cells from PDF text, ready to be written as CSV.
pub fn text_to_csv(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| split_cells(l).iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn convert_pdf(input: &Path, target: &str) -> Result<PathBuf, String> {
    match target {
        "txt" | "md" => {
            let out = unique_output(input, "", target);
            let raw = pdf_text_raw(input)?;
            fs::write(&out, tidy_text(&raw, target == "md")).map_err(|e| e.to_string())?;
            Ok(out)
        }
        "xlsx" => {
            // LibreOffice cannot import PDFs as spreadsheets, so rebuild the table
            // from the text layout: columns are separated by 2+ spaces.
            let out = unique_output(input, "", target);
            let raw = pdf_text_raw(input)?;
            let tmp = TmpDir::new("pdfx")?;
            let csv = tmp.path().join(format!("{}.csv", stem_of(input)));
            fs::write(&csv, text_to_csv(&tidy_text(&raw, false))).map_err(|e| e.to_string())?;
            soffice_to(&csv, "xlsx", None, &out)?;
            Ok(out)
        }
        "jpg" | "png" => {
            let dir = unique_dir(input, "-pages");
            pdf_to_images(input, target, &dir, "page")?;
            Ok(dir)
        }
        "docx" | "odt" => {
            let out = unique_output(input, "", target);
            soffice_to(input, target, Some("writer_pdf_import"), &out)?;
            Ok(out)
        }
        "pptx" => {
            let out = unique_output(input, "", target);
            soffice_to(input, "pptx", Some("impress_pdf_import"), &out)?;
            Ok(out)
        }
        "epub" => {
            let out = unique_output(input, "", target);
            let tmp = TmpDir::new("pdf")?;
            let docx = tmp.path().join(format!("{}.docx", stem_of(input)));
            soffice_to(input, "docx", Some("writer_pdf_import"), &docx)?;
            pandoc_to(&docx, "epub", &out)?;
            Ok(out)
        }
        t => Err(format!("Cannot convert PDF to .{t}")),
    }
}

// -- Archives and folders --------------------------------------------------------

fn sevenzip() -> Result<String, String> {
    engine("7z")
}

/// Extract an archive into `dest`, unwrapping a single inner .tar (for .tar.gz etc.).
fn extract_archive(input: &Path, dest: &Path) -> Result<(), String> {
    let sz = sevenzip()?;
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    run(&sz, &["x".into(), "-y".into(), format!("-o{}", s(dest)), s(input)])?;
    let entries: Vec<PathBuf> = fs::read_dir(dest)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    if entries.len() == 1 && entries[0].is_file() && ext_of(&entries[0]) == "tar" {
        let inner = entries[0].clone();
        run(&sz, &["x".into(), "-y".into(), format!("-o{}", s(dest)), s(&inner)])?;
        let _ = fs::remove_file(&inner);
    }
    Ok(())
}

fn pack(source_glob: &str, target: &str, out: &Path) -> Result<(), String> {
    let sz = sevenzip()?;
    match target {
        "zip" | "tar" | "7z" => run(&sz, &["a".into(), format!("-t{target}"), s(out), source_glob.to_string()]),
        "tar.gz" | "tar.xz" => {
            let tmp = TmpDir::new("pack")?;
            let tar = tmp.path().join("archive.tar");
            run(&sz, &["a".into(), "-ttar".into(), s(&tar), source_glob.to_string()])?;
            let comp = if target == "tar.gz" { "-tgzip" } else { "-txz" };
            run(&sz, &["a".into(), comp.into(), s(out), s(&tar)])
        }
        _ => Err(format!("Cannot create .{target} archives")),
    }
}

fn convert_archive(input: &Path, target: &str, out: &Path) -> Result<(), String> {
    let tmp = TmpDir::new("arc")?;
    let content = tmp.path().join("content");
    extract_archive(input, &content)?;
    let all = format!("{}{}*", s(&content), std::path::MAIN_SEPARATOR);
    pack(&all, target, out)
}

fn convert_folder(input: &Path, target: &str) -> Result<PathBuf, String> {
    let out = unique_output(input, "", target);
    let all = format!("{}{}*", s(input), std::path::MAIN_SEPARATOR);
    pack(&all, target, &out)?;
    Ok(out)
}

// ------------------------------------------------------------------ tools ---

pub struct FieldDef {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
}

pub struct ToolDef {
    pub id: &'static str,
    pub label: &'static str,
    pub title: &'static str,
    pub fields: Vec<FieldDef>,
    pub multi: bool,
}

fn t(id: &'static str, label: &'static str) -> ToolDef {
    ToolDef { id, label, title: label, fields: vec![], multi: false }
}

fn tf(id: &'static str, label: &'static str, title: &'static str, fields: Vec<FieldDef>) -> ToolDef {
    ToolDef { id, label, title, fields, multi: false }
}

fn fd(key: &'static str, label: &'static str, placeholder: &'static str) -> FieldDef {
    FieldDef { key, label, placeholder }
}

/// File tools offered for a kind. With several files of the same kind, the
/// "merge" style tool that consumes all of them at once is added.
pub fn tools_for(kind: Kind, ext: &str, count: usize) -> Vec<ToolDef> {
    let mut v: Vec<ToolDef> = match kind {
        Kind::Image if canon(ext) != "svg" => vec![
            t("compress", "Compress"),
            tf("resize", "Resize", "Resize image", vec![fd("a", "New width (px)", "1280")]),
            tf("crop", "Crop", "Crop image", vec![fd("a", "width:height:x:y", "800:600:0:0")]),
            tf("rotate", "Rotate", "Rotate image", vec![fd("a", "Degrees clockwise (90, 180, 270)", "90")]),
            tf("flip", "Flip", "Flip image", vec![fd("a", "h = horizontal, v = vertical", "h")]),
            t("grayscale", "Grayscale"),
            t("strip-metadata", "Strip metadata"),
        ],
        Kind::Video => vec![
            t("compress", "Compress"),
            tf("trim", "Trim", "Trim video", vec![fd("a", "Start (e.g. 00:00:05)", "00:00:05"), fd("b", "End (optional)", "00:00:30")]),
            tf("resize", "Resize", "Resize video", vec![fd("a", "New width (px)", "1280")]),
            tf("crop", "Crop", "Crop video", vec![fd("a", "width:height:x:y", "1280:720:0:0")]),
            tf("rotate", "Rotate", "Rotate video", vec![fd("a", "Degrees clockwise (90, 180, 270)", "90")]),
            tf("speed", "Speed", "Change speed", vec![fd("a", "Speed (0.25 to 4)", "2")]),
            tf("volume", "Volume", "Change volume", vec![fd("a", "Factor or dB (1.5, 0.5, 6dB)", "1.5")]),
            t("reverse", "Reverse"),
            t("mute", "Mute"),
            t("extract-audio", "Extract audio"),
            tf("thumbnail", "Thumbnail", "Save a frame", vec![fd("a", "Time (e.g. 00:00:03)", "00:00:01")]),
            tf("split", "Split", "Split into parts", vec![fd("a", "Seconds per part", "60")]),
            t("strip-metadata", "Strip metadata"),
        ],
        Kind::Audio => vec![
            t("compress", "Compress"),
            tf("trim", "Trim", "Trim audio", vec![fd("a", "Start (e.g. 00:00:05)", "00:00:05"), fd("b", "End (optional)", "00:00:30")]),
            t("normalize", "Normalize"),
            tf("volume", "Volume", "Change volume", vec![fd("a", "Factor or dB (1.5, 0.5, 6dB)", "1.5")]),
            tf("speed", "Speed", "Change speed", vec![fd("a", "Speed (0.25 to 4)", "1.25")]),
            tf("fade", "Fade", "Fade in / out", vec![fd("a", "Fade in (seconds)", "2"), fd("b", "Fade out (seconds)", "3")]),
            t("reverse", "Reverse"),
            tf("split", "Split", "Split into parts", vec![fd("a", "Seconds per part", "60")]),
            t("strip-metadata", "Strip metadata"),
        ],
        Kind::Pdf => vec![
            t("compress", "Compress"),
            t("split", "Split pages"),
            tf("extract-pages", "Extract pages", "Extract pages", vec![fd("a", "Pages (e.g. 2-5 or 3)", "1-3")]),
        ],
        Kind::Archive => vec![t("extract", "Extract")],
        _ => vec![],
    };
    if count > 1 {
        match kind {
            Kind::Pdf | Kind::Audio | Kind::Video => v.push(ToolDef { multi: true, ..t("merge", "Merge") }),
            Kind::Image => v.push(ToolDef { multi: true, ..t("merge", "Combine into PDF") }),
            _ => {}
        }
    }
    v
}

/// Number of distinct file tools Clementine offers.
pub fn tool_count() -> usize {
    let mut ids: Vec<&str> = [Kind::Image, Kind::Video, Kind::Audio, Kind::Pdf, Kind::Archive]
        .iter()
        .flat_map(|k| tools_for(*k, "x", 2))
        .map(|d| d.id)
        .collect();
    ids.sort();
    ids.dedup();
    ids.len()
}

fn num(v: Option<&str>, what: &str) -> Result<f64, String> {
    need(v, what)?
        .parse::<f64>()
        .map_err(|_| format!("{what} must be a number"))
}

fn need<'a>(v: Option<&'a str>, what: &str) -> Result<&'a str, String> {
    match v.map(|x| x.trim()) {
        Some(x) if !x.is_empty() => Ok(x),
        _ => Err(format!("Missing value: {what}")),
    }
}

fn blank(v: Option<&str>) -> bool {
    v.map(|x| x.trim().is_empty()).unwrap_or(true)
}

fn atempo_chain(mut f: f64) -> String {
    let mut parts: Vec<String> = Vec::new();
    while f > 2.0 {
        parts.push("atempo=2.0".to_string());
        f /= 2.0;
    }
    while f < 0.5 {
        parts.push("atempo=0.5".to_string());
        f *= 2.0;
    }
    parts.push(format!("atempo={f}"));
    parts.join(",")
}

fn transpose_for(deg: &str) -> Result<&'static str, String> {
    match deg.trim().trim_end_matches('°') {
        "90" => Ok("transpose=1"),
        "270" | "-90" => Ok("transpose=2"),
        "180" => Ok("hflip,vflip"),
        _ => Err("Rotation must be 90, 180 or 270".to_string()),
    }
}

pub fn tool(input: &Path, name: &str, a: Option<&str>, b: Option<&str>) -> Result<PathBuf, String> {
    if !input.exists() {
        return Err(format!("{} no longer exists", input.display()));
    }
    let ext = ext_of(input);
    let kind = kind_of_path(input);
    if !tools_for(kind, &ext, 1).iter().any(|d| d.id == name) {
        return Err(format!("\"{name}\" is not available for {} files", kind.name()));
    }
    match (kind, name) {
        // ---- compress
        (Kind::Pdf, "compress") => {
            let out = unique_output(input, "-compressed", "pdf");
            gs(&[
                "-sDEVICE=pdfwrite".into(),
                "-dCompatibilityLevel=1.4".into(),
                "-dPDFSETTINGS=/ebook".into(),
                format!("-sOutputFile={}", s(&out)),
                s(input),
            ])?;
            Ok(out)
        }
        (Kind::Image, "compress") => match canon(&ext).as_str() {
            "jpg" => ffmpeg_with(input, "-compressed", &ext, &[], &["-frames:v", "1", "-q:v", "6"]),
            "png" => ffmpeg_with(input, "-compressed", &ext, &[], &["-frames:v", "1", "-compression_level", "9"]),
            "webp" => ffmpeg_with(input, "-compressed", &ext, &[], &["-frames:v", "1", "-q:v", "60"]),
            e => Err(format!("Compress is not available for .{e} images")),
        },
        (Kind::Video, "compress") => {
            if canon(&ext) == "webm" {
                ffmpeg_with(input, "-compressed", &ext, &[], &["-c:v", "libvpx-vp9", "-crf", "38", "-b:v", "0", "-c:a", "libopus"])
            } else {
                ffmpeg_with(input, "-compressed", &ext, &[], &["-c:v", "libx264", "-crf", "28", "-preset", "medium", "-c:a", "aac", "-b:a", "128k"])
            }
        }
        (Kind::Audio, "compress") => match canon(&ext).as_str() {
            "wav" | "flac" | "aiff" => Err("This format is lossless; convert it to MP3 or Opus to shrink it".to_string()),
            _ => ffmpeg_with(input, "-compressed", &ext, &[], &["-b:a", "96k"]),
        },

        // ---- trim / split / thumbnail
        (Kind::Video, "trim") | (Kind::Audio, "trim") => {
            if blank(a) && blank(b) {
                return Err("Give a start time, an end time, or both (e.g. 00:00:05)".to_string());
            }
            let mut post: Vec<&str> = Vec::new();
            if let Some(start) = a.map(|x| x.trim()).filter(|x| !x.is_empty()) {
                post.extend(["-ss", start]);
            }
            if let Some(end) = b.map(|x| x.trim()).filter(|x| !x.is_empty()) {
                post.extend(["-to", end]);
            }
            ffmpeg_with(input, "-trimmed", &ext, &[], &post)
        }
        (Kind::Video, "thumbnail") => {
            let at = if blank(a) { "00:00:01" } else { a.unwrap().trim() };
            ffmpeg_with(input, "-thumb", "jpg", &["-ss", at], &["-frames:v", "1", "-q:v", "2"])
        }
        (Kind::Video, "split") | (Kind::Audio, "split") => {
            let secs = num(a, "Seconds per part")?;
            if secs < 1.0 {
                return Err("Parts must be at least 1 second long".to_string());
            }
            let dir = unique_dir(input, "-parts");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let pattern = dir.join(format!("part-%03d.{ext}"));
            let secs_s = format!("{secs}");
            ffmpeg(&ff_args(
                input,
                &[],
                &["-f", "segment", "-segment_time", &secs_s, "-c", "copy", "-reset_timestamps", "1"],
                &pattern,
            ))?;
            Ok(dir)
        }

        // ---- geometry
        (Kind::Image, "resize") | (Kind::Video, "resize") => {
            let w = need(a, "width in pixels")?;
            if !w.chars().all(|c| c.is_ascii_digit()) {
                return Err("Width must be a whole number of pixels".to_string());
            }
            let vf = format!("scale={w}:-2");
            let mut post = vec!["-vf", vf.as_str()];
            if kind == Kind::Image {
                post.extend(frame_args(&ext));
            }
            ffmpeg_with(input, "-resized", &ext, &[], &post)
        }
        (Kind::Image, "crop") | (Kind::Video, "crop") => {
            let spec = need(a, "crop as width:height:x:y")?;
            if spec.split(':').count() != 4 {
                return Err("Crop needs four numbers: width:height:x:y".to_string());
            }
            let vf = format!("crop={spec}");
            let mut post = vec!["-vf", vf.as_str()];
            if kind == Kind::Image {
                post.extend(frame_args(&ext));
            }
            ffmpeg_with(input, "-cropped", &ext, &[], &post)
        }
        (Kind::Image, "rotate") | (Kind::Video, "rotate") => {
            let vf = transpose_for(need(a, "degrees")?)?;
            let mut post = vec!["-vf", vf];
            if kind == Kind::Image {
                post.extend(frame_args(&ext));
            }
            ffmpeg_with(input, "-rotated", &ext, &[], &post)
        }
        (Kind::Image, "flip") => {
            let vf = match need(a, "h or v")?.to_lowercase().as_str() {
                "h" | "horizontal" => "hflip",
                "v" | "vertical" => "vflip",
                _ => return Err("Type h (horizontal) or v (vertical)".to_string()),
            };
            let mut post = vec!["-vf", vf];
            post.extend(frame_args(&ext));
            ffmpeg_with(input, "-flipped", &ext, &[], &post)
        }
        (Kind::Image, "grayscale") => {
            let mut post = vec!["-vf", "hue=s=0"];
            post.extend(frame_args(&ext));
            ffmpeg_with(input, "-gray", &ext, &[], &post)
        }

        // ---- audio / video processing
        (Kind::Video, "speed") | (Kind::Audio, "speed") => {
            let f = num(a, "Speed")?;
            if !(0.25..=4.0).contains(&f) {
                return Err("Speed must be between 0.25 and 4".to_string());
            }
            let mut post: Vec<String> = Vec::new();
            if kind == Kind::Video {
                post.extend(owned(&["-vf", &format!("setpts=PTS/{f}")]));
            }
            if kind == Kind::Audio || has_audio(input) {
                post.extend(owned(&["-af", &atempo_chain(f)]));
            }
            ffmpeg_with(input, "-speed", &ext, &[], &refs(&post))
        }
        (Kind::Video, "volume") | (Kind::Audio, "volume") => {
            let v = need(a, "volume")?;
            let ok = v.parse::<f64>().is_ok() || (v.to_lowercase().ends_with("db") && v[..v.len() - 2].trim().parse::<f64>().is_ok());
            if !ok {
                return Err("Volume must be a factor (1.5) or decibels (6dB)".to_string());
            }
            let af = format!("volume={v}");
            let mut post = vec!["-af", af.as_str()];
            if kind == Kind::Video {
                post.extend(["-c:v", "copy"]);
            }
            ffmpeg_with(input, "-volume", &ext, &[], &post)
        }
        (Kind::Audio, "normalize") => ffmpeg_with(input, "-normalized", &ext, &[], &["-af", "loudnorm=I=-16:TP=-1.5:LRA=11"]),
        (Kind::Audio, "fade") => {
            let fin = if blank(a) { 0.0 } else { num(a, "Fade in")? };
            let fout = if blank(b) { 0.0 } else { num(b, "Fade out")? };
            if fin <= 0.0 && fout <= 0.0 {
                return Err("Give a fade-in time, a fade-out time, or both".to_string());
            }
            let mut filters: Vec<String> = Vec::new();
            if fin > 0.0 {
                filters.push(format!("afade=t=in:st=0:d={fin}"));
            }
            if fout > 0.0 {
                let dur = duration_of(input)?;
                filters.push(format!("afade=t=out:st={}:d={fout}", (dur - fout).max(0.0)));
            }
            let af = filters.join(",");
            ffmpeg_with(input, "-faded", &ext, &[], &["-af", af.as_str()])
        }
        (Kind::Video, "reverse") => {
            let mut post = owned(&["-vf", "reverse"]);
            if has_audio(input) {
                post.extend(owned(&["-af", "areverse"]));
            }
            ffmpeg_with(input, "-reversed", &ext, &[], &refs(&post))
        }
        (Kind::Audio, "reverse") => ffmpeg_with(input, "-reversed", &ext, &[], &["-af", "areverse"]),
        (Kind::Video, "mute") => ffmpeg_with(input, "-muted", &ext, &[], &["-an", "-c:v", "copy"]),
        (Kind::Video, "extract-audio") => convert(input, "mp3"),
        (Kind::Image, "strip-metadata") | (Kind::Video, "strip-metadata") | (Kind::Audio, "strip-metadata") => {
            ffmpeg_with(input, "-clean", &ext, &[], &["-map_metadata", "-1", "-c", "copy"])
        }

        // ---- pdf
        (Kind::Pdf, "split") => {
            let n = pdf_page_count(input)?;
            let dir = unique_dir(input, "-pages");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            for i in 1..=n {
                let out = dir.join(format!("page-{i:03}.pdf"));
                gs(&[
                    "-sDEVICE=pdfwrite".into(),
                    format!("-dFirstPage={i}"),
                    format!("-dLastPage={i}"),
                    format!("-sOutputFile={}", s(&out)),
                    s(input),
                ])?;
            }
            Ok(dir)
        }
        (Kind::Pdf, "extract-pages") => {
            let spec = need(a, "page range")?;
            let (first, last) = match spec.split_once('-') {
                Some((x, y)) => (x.trim(), y.trim()),
                None => (spec, spec),
            };
            let (f, l): (u32, u32) = match (first.parse(), last.parse()) {
                (Ok(f), Ok(l)) if f >= 1 && l >= f => (f, l),
                _ => return Err("Pages must look like 3 or 2-5".to_string()),
            };
            let total = pdf_page_count(input)?;
            if f > total {
                return Err(format!("This PDF only has {total} pages"));
            }
            let out = unique_output(input, &format!("-p{f}-{}", l.min(total)), "pdf");
            gs(&[
                "-sDEVICE=pdfwrite".into(),
                format!("-dFirstPage={f}"),
                format!("-dLastPage={l}"),
                format!("-sOutputFile={}", s(&out)),
                s(input),
            ])?;
            Ok(out)
        }

        // ---- archive
        (Kind::Archive, "extract") => {
            let dir = unique_dir(input, "");
            extract_archive(input, &dir)?;
            Ok(dir)
        }
        _ => Err(format!("\"{name}\" is not available for {} files", kind.name())),
    }
}

/// Tools that take several files at once (merge / combine).
pub fn tool_multi(inputs: &[PathBuf], name: &str) -> Result<PathBuf, String> {
    if name != "merge" {
        return Err(format!("Unknown multi-file tool: {name}"));
    }
    if inputs.len() < 2 {
        return Err("Select at least two files to merge".to_string());
    }
    let kinds: Vec<Kind> = inputs.iter().map(|p| kind_of_path(p)).collect();
    if kinds.iter().any(|k| *k != kinds[0]) {
        return Err("Merge needs files of the same type".to_string());
    }
    let first = &inputs[0];
    match kinds[0] {
        Kind::Pdf => {
            let out = unique_output(first, "-merged", "pdf");
            merge_pdfs(inputs, &out)?;
            Ok(out)
        }
        Kind::Image => {
            let out = unique_output(first, "-combined", "pdf");
            let tmp = TmpDir::new("combine")?;
            let mut pdfs: Vec<PathBuf> = Vec::new();
            for (i, img) in inputs.iter().enumerate() {
                let sub = tmp.path().join(format!("{i:03}"));
                fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
                let pdf = sub.join("page.pdf");
                let src = ext_of(img);
                if matches!(canon(&src).as_str(), "heic" | "heif") {
                    let png = sub.join("in.png");
                    decode_heic(img, &png)?;
                    soffice_to(&png, "pdf", None, &pdf)?;
                } else {
                    soffice_to(img, "pdf", None, &pdf)?;
                }
                pdfs.push(pdf);
            }
            merge_pdfs(&pdfs, &out)?;
            Ok(out)
        }
        Kind::Audio => {
            let ext = ext_of(first);
            let out = unique_output(first, "-merged", &ext);
            let n = inputs.len();
            let mut args = owned(&["-y", "-hide_banner", "-loglevel", "error"]);
            for p in inputs {
                args.push("-i".into());
                args.push(s(p));
            }
            let labels: String = (0..n).map(|i| format!("[{i}:a]")).collect();
            args.push("-filter_complex".into());
            args.push(format!("{labels}concat=n={n}:v=0:a=1[a]"));
            args.extend(owned(&["-map", "[a]"]));
            args.extend(owned(&codec_args(Kind::Audio, &canon(&ext))));
            args.push(s(&out));
            ffmpeg(&args)?;
            Ok(out)
        }
        Kind::Video => {
            let ext = ext_of(first);
            let out = unique_output(first, "-merged", &ext);
            let n = inputs.len();
            let size = ffprobe(&["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=s=x:p=0", &s(first)])
                .map(|o| o.trim().to_string())
                .unwrap_or_else(|| "1280x720".to_string());
            let (w, h) = size.split_once('x').unwrap_or(("1280", "720"));
            let with_audio = inputs.iter().all(|p| has_audio(p));
            let mut args = owned(&["-y", "-hide_banner", "-loglevel", "error"]);
            let mut graph = String::new();
            for (i, p) in inputs.iter().enumerate() {
                args.push("-i".into());
                args.push(s(p));
                graph.push_str(&format!(
                    "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30[v{i}];"
                ));
            }
            for i in 0..n {
                graph.push_str(&format!("[v{i}]"));
                if with_audio {
                    graph.push_str(&format!("[{i}:a]"));
                }
            }
            graph.push_str(&format!("concat=n={n}:v=1:a={}[v]", if with_audio { 1 } else { 0 }));
            if with_audio {
                graph.push_str("[a]");
            }
            args.extend(owned(&["-filter_complex", &graph, "-map", "[v]"]));
            if with_audio {
                args.extend(owned(&["-map", "[a]"]));
            }
            args.extend(owned(&codec_args(Kind::Video, &canon(&ext))));
            args.push(s(&out));
            ffmpeg(&args)?;
            Ok(out)
        }
        k => Err(format!("Merge is not available for {} files", k.name())),
    }
}

fn merge_pdfs(inputs: &[PathBuf], out: &Path) -> Result<(), String> {
    let mut a = vec!["-sDEVICE=pdfwrite".to_string(), format!("-sOutputFile={}", s(out))];
    a.extend(inputs.iter().map(|p| s(p)));
    gs(&a)
}

// ---------------------------------------------------------------- actions ---

fn downloads_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join("Downloads")
}

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for e in fs::read_dir(src)? {
            let e = e?;
            copy_recursive(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

/// Free destination name for `name` inside `dir` ("a.txt" -> "a (1).txt").
fn free_name(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("item");
    let ext = p.extension().and_then(|x| x.to_str());
    let mut n = 1;
    loop {
        let candidate = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let c = dir.join(candidate);
        if !c.exists() {
            return c;
        }
        n += 1;
    }
}

pub fn file_action(path: &Path, name: &str, dest: Option<&str>) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("{} no longer exists", path.display()));
    }
    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    };
    match name {
        "open" => if cfg!(windows) {
            command("cmd").args(["/C", "start", ""]).arg(path).spawn()
        } else {
            command("xdg-open").arg(path).spawn()
        }
        .map(|_| "Opened".to_string())
        .map_err(|e| e.to_string()),
        "reveal" => if cfg!(windows) {
            command("explorer").arg(format!("/select,{}", path.display())).spawn()
        } else {
            command("xdg-open").arg(&dir).spawn()
        }
        .map(|_| "Shown in file manager".to_string())
        .map_err(|e| e.to_string()),
        "terminal" => {
            if !cfg!(windows) {
                return Err("Open Terminal is only wired up for Windows".to_string());
            }
            if command("wt").arg("-d").arg(&dir).spawn().is_ok() {
                return Ok("Terminal opened".to_string());
            }
            command("cmd")
                .args(["/C", "start", "cmd"])
                .current_dir(&dir)
                .spawn()
                .map(|_| "Terminal opened".to_string())
                .map_err(|e| e.to_string())
        }
        "copy-to" | "move-to" | "copy-downloads" => {
            let dest_dir = match (name, dest) {
                ("copy-downloads", _) => downloads_dir(),
                (_, Some(d)) if !d.trim().is_empty() => PathBuf::from(d),
                _ => return Err("Choose a destination folder".to_string()),
            };
            fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
            let file_name = path.file_name().ok_or("This item has no name")?;
            let target = free_name(&dest_dir, file_name);
            if path.is_dir() && target.starts_with(path) {
                return Err("A folder cannot be copied into itself".to_string());
            }
            if name == "move-to" {
                if fs::rename(path, &target).is_err() {
                    copy_recursive(path, &target).map_err(|e| e.to_string())?;
                    if path.is_dir() {
                        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
                    } else {
                        fs::remove_file(path).map_err(|e| e.to_string())?;
                    }
                }
                Ok(format!("Moved to {}", target.display()))
            } else {
                copy_recursive(path, &target).map_err(|e| e.to_string())?;
                Ok(format!("Copied to {}", target.display()))
            }
        }
        other => Err(format!("Unknown action: {other}")),
    }
}

// ------------------------------------------------------------------ tests ---

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> TmpDir {
        TmpDir::new(name).unwrap()
    }
    fn have(id: &str) -> bool {
        find_tool(id).is_some()
    }
    fn has_encoder(name: &str) -> bool {
        match find_tool("ffmpeg") {
            Some(f) => command(&s(&f))
                .args(["-hide_banner", "-encoders"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(name))
                .unwrap_or(false),
            None => false,
        }
    }
    fn nonempty(p: &Path) -> bool {
        if p.is_dir() {
            fs::read_dir(p).map(|d| d.count() > 0).unwrap_or(false)
        } else {
            fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
        }
    }
    fn count_files(p: &Path) -> usize {
        fs::read_dir(p).map(|d| d.count()).unwrap_or(0)
    }
    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
    }
    fn gen(dir: &Path, name: &str, args: &[&str]) -> PathBuf {
        let p = dir.join(name);
        let mut a = owned(&["-y", "-hide_banner", "-loglevel", "error"]);
        a.extend(owned(args));
        a.push(s(&p));
        ffmpeg(&a).unwrap_or_else(|e| panic!("fixture {name}: {e}"));
        p
    }
    fn conv(p: &Path, target: &str) -> PathBuf {
        let o = convert(p, target).unwrap_or_else(|e| panic!("{} -> {target}: {e}", p.display()));
        assert!(nonempty(&o), "{} -> {target} produced nothing", p.display());
        o
    }
    fn tl(p: &Path, name: &str, a: Option<&str>, b: Option<&str>) -> PathBuf {
        let o = tool(p, name, a, b).unwrap_or_else(|e| panic!("{name} on {}: {e}", p.display()));
        assert!(nonempty(&o), "{name} produced nothing");
        o
    }
    fn pages(p: &Path) -> u32 {
        pdf_page_count(p).unwrap()
    }
    const MD: &str = "# Report\n\nHello world paragraph.\n\n| Item | Qty |\n|------|-----|\n| Apples | 3 |\n| Pears | 5 |\n";

    // ---- pure logic ---------------------------------------------------------

    #[test]
    fn kinds_and_targets() {
        assert_eq!(kind_of("png"), Kind::Image);
        assert_eq!(kind_of("mkv"), Kind::Video);
        assert_eq!(kind_of("flac"), Kind::Audio);
        assert_eq!(kind_of("xlsx"), Kind::Sheet);
        assert_eq!(kind_of("pdf"), Kind::Pdf);
        assert_eq!(kind_of("xyz"), Kind::Other);
        assert!(!targets_for(Kind::Image, "png").contains(&"png"));
        assert!(!targets_for(Kind::Image, "jpeg").contains(&"jpg"));
        assert!(targets_for(Kind::Pdf, "pdf").contains(&"xlsx"));
        assert!(targets_for(Kind::Other, "xyz").is_empty());
        assert!(targets_for(Kind::Folder, "").contains(&"zip"));
        let n = conversion_option_count();
        eprintln!("conversion options (input format -> output format pairs): {n}");
        assert!(n > 250, "expected a broad catalog, got {n}");
    }

    #[test]
    fn tool_catalog() {
        let ids = |k: Kind, e: &str, c: usize| tools_for(k, e, c).iter().map(|t| t.id).collect::<Vec<_>>();
        assert!(ids(Kind::Image, "png", 1).contains(&"rotate"));
        assert!(ids(Kind::Image, "svg", 1).is_empty());
        assert!(!ids(Kind::Pdf, "pdf", 1).contains(&"merge"));
        assert!(ids(Kind::Pdf, "pdf", 2).contains(&"merge"));
        assert!(ids(Kind::Video, "mp4", 1).contains(&"thumbnail"));
        assert!(ids(Kind::Doc, "docx", 3).is_empty());
        let mut all: Vec<&str> = [Kind::Image, Kind::Video, Kind::Audio, Kind::Pdf, Kind::Archive]
            .iter()
            .flat_map(|k| ids(*k, "x", 2))
            .collect();
        all.sort();
        all.dedup();
        eprintln!("distinct file tools: {} -> {:?}", all.len(), all);
    }

    #[test]
    fn text_helpers() {
        assert_eq!(split_cells("Item            Qty"), vec!["Item", "Qty"]);
        assert_eq!(split_cells("New York  10001  USA"), vec!["New York", "10001", "USA"]);
        assert_eq!(text_to_csv("a  b\n\nc  \"d\"\n"), "a,b\nc,\"\"\"d\"\"\"\n");
        assert_eq!(tidy_text("       Report\r\n\r\n\r\n\r\n       Body\r\n", false), "Report\n\nBody\n");
        assert_eq!(tidy_text("    a\n      b\n", true), "a\nb\n");
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nHello there\n\n2\n00:00:03,000 --> 00:00:04,000\nGeneral Kenobi\n";
        assert_eq!(strip_subs(srt), "Hello there\nGeneral Kenobi\n");
    }

    #[test]
    fn argument_validation() {
        let d = tmpdir("val");
        let f = d.path().join("x.png");
        fs::write(&f, b"x").unwrap();
        assert!(convert(Path::new("/nope/a.png"), "jpg").unwrap_err().contains("no longer exists"));
        assert!(convert(&f, "png").unwrap_err().contains("Already"));
        assert!(convert(&f, "mp3").unwrap_err().contains("Cannot convert"));
        let o = d.path().join("x.unknownext");
        fs::write(&o, b"x").unwrap();
        assert!(convert(&o, "png").unwrap_err().contains("Cannot convert"));
        for (name, a, b) in [
            ("resize", Some("abc"), None),
            ("crop", Some("1:2"), None),
            ("rotate", Some("45"), None),
            ("flip", Some("z"), None),
            ("trim", Some("0"), None), // not available for images
            ("nonsense", None, None),
        ] {
            assert!(tool(&f, name, a, b).is_err(), "{name} should be rejected");
        }
        assert!(engine("nope").unwrap_err().contains("not installed"));
        assert!(find_tool("definitely-not-a-real-tool").is_none());
        assert!(tool_multi(&[f.clone()], "merge").is_err());
        assert!(tool_multi(&[f.clone(), o.clone()], "merge").unwrap_err().contains("same type"));
    }

    // ---- ffmpeg: images, audio, video, subtitles ---------------------------------

    #[test]
    fn audio_conversions_and_tools() {
        if !have("ffmpeg") {
            return eprintln!("ffmpeg missing, skipping");
        }
        let d = tmpdir("audio");
        let dir = d.path();
        let wav = gen(dir, "tone.wav", &["-f", "lavfi", "-i", "sine=frequency=440:duration=4"]);
        for (t, enc) in [("flac", "flac"), ("mp3", "libmp3lame"), ("ogg", "libvorbis"), ("opus", "libopus"), ("m4a", "aac"), ("aiff", "pcm_s16be"), ("wma", "wmav2")] {
            if has_encoder(enc) {
                conv(&wav, t);
                eprintln!("ok wav -> {t}");
            }
        }
        assert!(tool(&wav, "compress", None, None).is_err(), "lossless compress refused");
        // tools (work on wav so no encoder dependency)
        tl(&wav, "trim", Some("0"), Some("2"));
        tl(&wav, "normalize", None, None);
        tl(&wav, "volume", Some("0.5"), None);
        tl(&wav, "volume", Some("6dB"), None);
        let fast = tl(&wav, "speed", Some("2"), None);
        assert!((duration_of(&fast).unwrap() - 2.0).abs() < 0.3, "2x speed halves duration");
        let slow = tl(&wav, "speed", Some("0.25"), None);
        assert!(duration_of(&slow).unwrap() > 14.0, "0.25x quadruples duration");
        tl(&wav, "fade", Some("1"), Some("1"));
        tl(&wav, "reverse", None, None);
        tl(&wav, "strip-metadata", None, None);
        let parts = tl(&wav, "split", Some("1"), None);
        assert!(count_files(&parts) >= 3, "4s split by 1s gives several parts");
        assert!(tool(&wav, "speed", Some("9"), None).is_err());
        assert!(tool(&wav, "volume", Some("loud"), None).is_err());
        assert!(tool(&wav, "fade", None, None).is_err());
        assert!(tool(&wav, "split", Some("0"), None).is_err());
        // merge two clips: durations add up
        let b = gen(dir, "tone2.wav", &["-f", "lavfi", "-i", "sine=frequency=880:duration=2"]);
        let m = tool_multi(&[wav.clone(), b], "merge").unwrap();
        assert!((duration_of(&m).unwrap() - 6.0).abs() < 0.4, "merged audio is 4s + 2s");
        if has_encoder("libmp3lame") {
            let mp3 = conv(&wav, "mp3");
            tl(&mp3, "compress", None, None);
        }
    }

    #[test]
    fn image_conversions_and_tools() {
        if !have("ffmpeg") {
            return eprintln!("ffmpeg missing, skipping");
        }
        let d = tmpdir("img");
        let dir = d.path();
        let png = gen(dir, "pic.png", &["-f", "lavfi", "-i", "testsrc=size=320x240:rate=1", "-frames:v", "1"]);
        for (t, enc) in [("jpg", "mjpeg"), ("bmp", "bmp"), ("webp", "libwebp"), ("tiff", "tiff"), ("gif", "gif"), ("avif", "libaom")] {
            if has_encoder(enc) {
                conv(&png, t);
                eprintln!("ok png -> {t}");
            }
        }
        conv(&png, "ico");
        let ico = dir.join("pic.ico");
        assert!(fs::read(&ico).unwrap().starts_with(&[0, 0, 1, 0]), "valid .ico header");
        conv(&ico, "png");
        // image tools
        tl(&png, "resize", Some("160"), None);
        tl(&png, "crop", Some("100:100:10:10"), None);
        tl(&png, "rotate", Some("90"), None);
        tl(&png, "rotate", Some("180"), None);
        tl(&png, "rotate", Some("270"), None);
        tl(&png, "flip", Some("h"), None);
        tl(&png, "flip", Some("v"), None);
        tl(&png, "grayscale", None, None);
        tl(&png, "strip-metadata", None, None);
        tl(&png, "compress", None, None);
        // gif -> video
        if has_encoder("mjpeg") && has_encoder("gif") {
            let gif = gen(dir, "anim.gif", &["-f", "lavfi", "-i", "testsrc=size=160x120:rate=5:duration=1"]);
            if has_encoder("libx264") {
                conv(&gif, "mp4");
            }
            if has_encoder("libvpx-vp9") {
                conv(&gif, "webm");
            }
            conv(&gif, "png");
        }
        // SVG and image -> PDF go through LibreOffice
        if have("soffice") {
            let svg = dir.join("dot.svg");
            fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><circle cx="50" cy="50" r="40" fill="orange"/></svg>"#).unwrap();
            conv(&svg, "png");
            conv(&svg, "jpg");
            conv(&svg, "pdf");
            let pdf = conv(&png, "pdf");
            if have("gs") {
                assert_eq!(pages(&pdf), 1);
                let png2 = gen(dir, "pic2.png", &["-f", "lavfi", "-i", "testsrc2=size=200x200:rate=1", "-frames:v", "1"]);
                let combined = tool_multi(&[png.clone(), png2], "merge").unwrap();
                assert_eq!(pages(&combined), 2, "two images become a two-page PDF");
            }
        }
    }

    #[test]
    fn video_conversions_and_tools() {
        if !have("ffmpeg") || !has_encoder("mpeg4") {
            return eprintln!("ffmpeg/mpeg4 missing, skipping");
        }
        let d = tmpdir("video");
        let dir = d.path();
        let avi = gen(
            dir,
            "clip.avi",
            &["-f", "lavfi", "-i", "testsrc=size=160x120:rate=10:duration=3", "-f", "lavfi", "-i", "sine=duration=3", "-c:v", "mpeg4", "-c:a", "pcm_s16le", "-shortest"],
        );
        let silent = gen(dir, "silent.avi", &["-f", "lavfi", "-i", "testsrc=size=160x120:rate=10:duration=2", "-c:v", "mpeg4"]);
        conv(&avi, "gif");
        conv(&avi, "wav");
        conv(&avi, "flac");
        for (t, enc) in [("mp3", "libmp3lame"), ("m4a", "aac"), ("ogg", "libvorbis"), ("opus", "libopus")] {
            if has_encoder(enc) {
                conv(&avi, t);
            }
        }
        if has_encoder("libx264") {
            let mp4 = conv(&avi, "mp4");
            tl(&mp4, "compress", None, None);
            conv(&mp4, "mkv");
            conv(&mp4, "mov");
        }
        if has_encoder("libvpx-vp9") {
            conv(&avi, "webm");
        }
        tl(&avi, "trim", Some("0"), Some("1"));
        tl(&avi, "resize", Some("80"), None);
        tl(&avi, "crop", Some("100:80:10:10"), None);
        tl(&avi, "rotate", Some("90"), None);
        tl(&avi, "mute", None, None);
        tl(&avi, "reverse", None, None);
        tl(&avi, "strip-metadata", None, None);
        tl(&avi, "volume", Some("1.5"), None);
        let thumb = tl(&avi, "thumbnail", Some("00:00:01"), None);
        assert_eq!(ext_of(&thumb), "jpg");
        let fast = tl(&avi, "speed", Some("2"), None);
        assert!((duration_of(&fast).unwrap() - 1.5).abs() < 0.4);
        // silent video must also work with speed / reverse (no audio stream)
        tl(&silent, "speed", Some("2"), None);
        tl(&silent, "reverse", None, None);
        assert!(!has_audio(&silent) && has_audio(&avi));
        let parts = tl(&avi, "split", Some("1"), None);
        assert!(count_files(&parts) >= 2);
        if has_encoder("pcm_s16le") {
            let m = tool_multi(&[avi.clone(), avi.clone()], "merge").unwrap();
            assert!((duration_of(&m).unwrap() - 6.0).abs() < 0.6, "merged video is 3s + 3s");
        }
    }

    #[test]
    fn subtitle_conversions() {
        let d = tmpdir("subs");
        let srt = d.path().join("movie.srt");
        fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        assert_eq!(read(&conv(&srt, "txt")), "Hello\n");
        if have("ffmpeg") {
            assert!(read(&conv(&srt, "vtt")).starts_with("WEBVTT"));
            let ass = d.path().join("movie.ass");
            ffmpeg(&ff_args(&srt, &[], &[], &ass)).unwrap();
            assert!(read(&conv(&ass, "srt")).contains("Hello"));
            assert!(read(&conv(&ass, "txt")).contains("Hello"));
        }
    }

    // ---- LibreOffice / pandoc / ghostscript ---------------------------------------

    #[test]
    fn documents() {
        if !(have("pandoc") && have("soffice")) {
            return eprintln!("pandoc/LibreOffice missing, skipping");
        }
        let d = tmpdir("docs");
        let dir = d.path();
        let md = dir.join("report.md");
        fs::write(&md, MD).unwrap();
        for t in ["docx", "html", "txt", "epub", "rtf", "odt", "pdf"] {
            conv(&md, t);
            eprintln!("ok md -> {t}");
        }
        assert!(read(&dir.join("report.txt")).contains("Hello world"));
        let docx = dir.join("report.docx");
        let back = conv(&docx, "md");
        assert!(read(&back).contains("Apples"));
        for t in ["pdf", "html", "txt", "epub", "rtf", "odt"] {
            conv(&docx, t);
        }
        // legacy .doc and .rtf are read through LibreOffice
        let doc = dir.join("legacy.doc");
        soffice_to(&docx, "doc", None, &doc).unwrap();
        for t in ["docx", "md", "epub", "pdf", "txt", "html", "odt"] {
            conv(&doc, t);
        }
        assert!(read(&conv(&doc, "md")).contains("Hello"));
        let rtf = dir.join("report.rtf");
        assert!(read(&conv(&rtf, "md")).contains("Hello"));
        conv(&dir.join("report.html"), "docx");
        conv(&dir.join("report.epub"), "docx");
        conv(&dir.join("report.odt"), "md");
    }

    #[test]
    fn spreadsheets_and_slides() {
        if !(have("soffice") && have("pandoc")) {
            return eprintln!("LibreOffice/pandoc missing, skipping");
        }
        let d = tmpdir("office");
        let dir = d.path();
        let csv = dir.join("data.csv");
        fs::write(&csv, "Item,Qty\nApples,3\nPears,5\n").unwrap();
        for t in ["xlsx", "ods", "tsv", "pdf", "html"] {
            conv(&csv, t);
        }
        let xlsx = dir.join("data.xlsx");
        assert!(read(&conv(&xlsx, "csv")).contains("Apples,3"));
        assert!(read(&dir.join("data.tsv")).contains("Apples\t3"));
        let xls = dir.join("old.xls");
        soffice_to(&xlsx, "xls", None, &xls).unwrap();
        conv(&xls, "xlsx");
        conv(&dir.join("data.ods"), "xlsx");

        let md = dir.join("deck.md");
        fs::write(&md, "% Deck\n\n# Slide one\n\n- point a\n- point b\n\n# Slide two\n\nHello world\n").unwrap();
        let pptx = dir.join("deck.pptx");
        run(&engine("pandoc").unwrap(), &[s(&md), "-o".into(), s(&pptx)]).unwrap();
        conv(&pptx, "pdf");
        conv(&pptx, "odp");
        assert!(convert(&pptx, "pptx").is_err());
        let ppt = dir.join("legacy.ppt");
        soffice_to(&pptx, "ppt", None, &ppt).unwrap();
        conv(&ppt, "pptx");
        conv(&ppt, "pdf");
        if have("gs") {
            let png = conv(&pptx, "png");
            assert!(png.is_dir() && count_files(&png) >= 2, "one PNG per slide");
            let jpg = conv(&pptx, "jpg");
            assert!(count_files(&jpg) >= 2);
            eprintln!("slides rendered: {} images", count_files(&jpg));
        }
    }

    #[test]
    fn pdf_in_and_out() {
        if !(have("pandoc") && have("soffice") && have("gs")) {
            return eprintln!("pandoc/LibreOffice/Ghostscript missing, skipping");
        }
        let d = tmpdir("pdf");
        let dir = d.path();
        let md = dir.join("report.md");
        fs::write(&md, MD).unwrap();
        let pdf = conv(&md, "pdf");
        assert_eq!(pages(&pdf), 1);

        let txt = read(&conv(&pdf, "txt"));
        assert!(txt.contains("Hello world paragraph"), "{txt}");
        assert!(!txt.contains('\r') && !txt.lines().next().unwrap().starts_with(' '));
        let mdout = read(&conv(&pdf, "md"));
        assert!(mdout.lines().all(|l| !l.starts_with(' ')), "no indented (code-block) lines in markdown");
        let png = conv(&pdf, "png");
        assert!(png.is_dir() && png.join("page-001.png").is_file());
        assert!(conv(&pdf, "jpg").join("page-001.jpg").is_file());
        conv(&pdf, "docx");
        conv(&pdf, "odt");
        conv(&pdf, "pptx");
        conv(&pdf, "epub");
        let xlsx = conv(&pdf, "xlsx");
        let csv = read(&conv(&xlsx, "csv"));
        assert!(csv.contains("Apples,3") && csv.contains("Pears,5"), "table rebuilt from PDF: {csv}");

        // pdf tools
        let three = tool_multi(&[pdf.clone(), pdf.clone(), pdf.clone()], "merge").unwrap();
        assert_eq!(pages(&three), 3);
        let split = tl(&three, "split", None, None);
        assert_eq!(count_files(&split), 3);
        assert_eq!(pages(&split.join("page-002.pdf")), 1);
        let part = tl(&three, "extract-pages", Some("2-3"), None);
        assert_eq!(pages(&part), 2);
        let one = tl(&three, "extract-pages", Some("1"), None);
        assert_eq!(pages(&one), 1);
        tl(&three, "compress", None, None);
        assert!(tool(&three, "extract-pages", Some("9"), None).unwrap_err().contains("only has 3"));
        assert!(tool(&three, "extract-pages", Some("3-1"), None).is_err());
        assert!(tool(&three, "extract-pages", Some("x"), None).is_err());
        // a file name with spaces and brackets still works
        let odd = dir.join("my report (final).pdf");
        fs::copy(&pdf, &odd).unwrap();
        assert_eq!(pages(&odd), 1);
        conv(&odd, "txt");
    }

    #[test]
    fn archives_and_folders() {
        if !have("7z") {
            return eprintln!("7-Zip missing, skipping");
        }
        let d = tmpdir("arc");
        let dir = d.path();
        let stuff = dir.join("stuff");
        fs::create_dir_all(stuff.join("sub")).unwrap();
        fs::write(stuff.join("a.txt"), "alpha").unwrap();
        fs::write(stuff.join("sub/b.txt"), "beta").unwrap();
        let zip = conv(&stuff, "zip");
        assert_eq!(ext_of(&zip), "zip");
        for t in ["7z", "tar", "tar.gz", "tar.xz"] {
            conv(&zip, t);
        }
        let tgz = dir.join("stuff.tar.gz");
        assert!(tgz.is_file(), "tar.gz named correctly");
        let back = conv(&tgz, "zip");
        assert_eq!(back.file_name().unwrap().to_str().unwrap(), "stuff (1).zip", "name not stuffed with .tar, no overwrite");
        let out = tl(&back, "extract", None, None);
        assert_eq!(read(&out.join("a.txt")), "alpha", "inner tar was unwrapped");
        assert_eq!(read(&out.join("sub/b.txt")), "beta");
        assert!(convert(&zip, "zip").is_err());
    }

    // ---- file actions ----------------------------------------------------------------

    #[test]
    fn copy_and_move_actions() {
        let d = tmpdir("actions");
        let dir = d.path();
        let dest = dir.join("dest");
        fs::create_dir_all(&dest).unwrap();
        let f = dir.join("note.txt");
        fs::write(&f, "hi").unwrap();
        let msg = file_action(&f, "copy-to", Some(&s(&dest))).unwrap();
        assert!(msg.starts_with("Copied") && dest.join("note.txt").is_file() && f.is_file());
        file_action(&f, "copy-to", Some(&s(&dest))).unwrap();
        assert!(dest.join("note (1).txt").is_file(), "never overwrites");
        let moved = file_action(&f, "move-to", Some(&s(&dest))).unwrap();
        assert!(moved.starts_with("Moved") && !f.exists() && dest.join("note (2).txt").is_file());
        // folders, including nested content
        let folder = dir.join("pics");
        fs::create_dir_all(folder.join("raw")).unwrap();
        fs::write(folder.join("raw/x.bin"), "1").unwrap();
        file_action(&folder, "copy-to", Some(&s(&dest))).unwrap();
        assert!(dest.join("pics/raw/x.bin").is_file());
        file_action(&folder, "move-to", Some(&s(&dest))).unwrap();
        assert!(!folder.exists() && dest.join("pics (1)/raw/x.bin").is_file());
        // errors
        assert!(file_action(&dest, "copy-to", Some(&s(&dest.join("inside")))).unwrap_err().contains("itself"));
        assert!(file_action(&dest, "copy-to", None).is_err());
        assert!(file_action(&dest, "explode", None).is_err());
        assert!(file_action(&dir.join("gone.txt"), "open", None).unwrap_err().contains("no longer exists"));
    }
}

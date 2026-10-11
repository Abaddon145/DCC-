use super::models::DownloadRequest;
use std::path::Path;
pub fn base_args() -> Vec<String> {
    [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-colors",
        "--encoding",
        "utf-8",
        "--socket-timeout",
        "20",
        "--retries",
        "3",
        "--fragment-retries",
        "3",
    ]
    .map(str::to_string)
    .into()
}
pub fn download_args(request: &DownloadRequest, folder: &Path) -> Result<Vec<String>, String> {
    super::domain::validate_request(request)?;
    let mut args = base_args();
    args.extend(
        [
            "--no-playlist",
            "--no-overwrites",
            "--windows-filenames",
            "--trim-filenames",
            "180",
            "--continue",
            "--newline",
            "--progress",
            "--progress-delta",
            "0.5",
            "--no-simulate",
            "-P",
        ]
        .map(str::to_string),
    );
    args.push(folder.to_string_lossy().into());
    args.extend(
        [
            "-o",
            "%(title).120B [%(id)s].%(ext)s",
            "--progress-template",
            "download:DCC_PROGRESS%(progress)j",
            "--progress-template",
            "postprocess:DCC_PROCESS%(progress)j",
            "--print",
            "after_move:DCC_FILE%(filepath)j",
        ]
        .map(str::to_string),
    );
    let height = match request.quality.as_str() {
        "1080p" => Some(1080),
        "720p" => Some(720),
        _ => None,
    };
    let selection = if request.format != "best" {
        if request.audio_only {
            request.format.clone()
        } else {
            format!("{}+ba/{}", request.format, request.format)
        }
    } else if request.audio_only {
        "ba/b".into()
    } else if let Some(height) = height {
        format!("bv*[height<=?{height}]+ba/b[height<=?{height}]")
    } else {
        "bv*+ba/b".into()
    };
    args.extend(["-f".into(), selection]);
    if request.audio_only {
        args.extend(["-x", "--audio-format", "mp3"].map(str::to_string));
    } else {
        args.extend(["--merge-output-format", "mp4/webm/mkv"].map(str::to_string));
    }
    if request.subtitles {
        args.extend(
            [
                "--write-subs",
                "--write-auto-subs",
                "--sub-langs",
                "all,-live_chat",
            ]
            .map(str::to_string),
        );
    }
    args.extend(["--".into(), super::domain::validate_url(&request.url)?]);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_arguments_are_safe_resumable_and_do_not_transcode_video() {
        let request = DownloadRequest {
            url: "https://example.com/a".into(),
            format: "best".into(),
            quality: "720p".into(),
            audio_only: false,
            subtitles: true,
            output_mode: "library".into(),
        };
        let args = download_args(&request, Path::new("C:/下载/task")).unwrap();
        for flag in [
            "--ignore-config",
            "--no-overwrites",
            "--continue",
            "--write-subs",
            "--no-playlist",
            "--no-plugin-dirs",
        ] {
            assert!(args.iter().any(|s| s == flag), "{flag}");
        }
        assert!(!args.iter().any(|s| s == "--recode-video" || s == "--exec"));
        assert_eq!(args[args.len() - 2], "--");
        assert_eq!(args.last(), Some(&request.url));
        assert!(args.iter().any(|s| s.contains("height<=?720")));
    }
}

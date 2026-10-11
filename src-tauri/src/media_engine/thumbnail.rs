use std::path::Path;
pub struct ManagedThumbnail {
    pub relative_path: String,
    pub width: u32,
    pub height: u32,
}
pub fn generate(
    base: &Path,
    resources: Option<&Path>,
    file_id: &str,
    source: &Path,
    kind: &str,
    stop: &dyn Fn() -> bool,
) -> Result<ManagedThumbnail, String> {
    let file_id = uuid::Uuid::parse_str(file_id)
        .map_err(|_| "文件 ID 无效")?
        .to_string();
    if stop() {
        return Err("缩略图生成已停止".into());
    }
    if !matches!(kind, "image" | "audio" | "video") {
        return Err("模型缩略图沿用现有 3D 渲染器".into());
    }
    let library = base.canonicalize().map_err(|e| e.to_string())?;
    let source = source.canonicalize().map_err(|_| "原件缺失")?;
    if !source.starts_with(&library) || !source.is_file() {
        return Err("原件不在当前素材库".into());
    }
    let directory = library.join(".dcc/cache/thumbnails").join(file_id);
    if directory.ancestors().any(crate::asset::scanner::is_link) {
        return Err("缩略图缓存目录不安全".into());
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix(".thumbnail-")
        .tempdir_in(&directory)
        .map_err(|e| e.to_string())?;
    let extension = if kind == "image" { "webp" } else { "png" };
    let output = stage.path().join(format!("preview.{extension}"));
    if kind == "image" {
        let picture = image::open(&source).map_err(|e| format!("图片无法解析：{e}"))?;
        if stop() {
            return Err("缩略图生成已停止".into());
        }
        picture
            .resize(480, 480, image::imageops::FilterType::Lanczos3)
            .save_with_format(&output, image::ImageFormat::WebP)
            .map_err(|e| e.to_string())?;
    } else {
        let ffmpeg = crate::runtime::tools::require(resources, "ffmpeg")?;
        crate::media::generate_thumbnail(&ffmpeg, kind, &source, &output, stop)?;
    }
    if stop() {
        return Err("缩略图生成已停止".into());
    }
    let (width, height) = image::image_dimensions(&output).map_err(|_| "生成的缩略图无法解析")?;
    if width == 0 || height == 0 {
        return Err("缩略图尺寸无效".into());
    }
    let name = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let target = directory.join(name);
    std::fs::rename(output, &target).map_err(|e| e.to_string())?;
    let relative_path = target
        .strip_prefix(&library)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(ManagedThumbnail {
        relative_path,
        width,
        height,
    })
}

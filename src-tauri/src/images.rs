use base64::{engine::general_purpose::STANDARD, Engine};
use image::imageops::FilterType;
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub struct StoredImage {
    pub id: String,
    pub original_name: String,
    pub original_rel_path: String,
    pub thumbnail_rel_path: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

pub fn import_image(base_dir: &Path, source: &Path) -> Result<StoredImage, String> {
    import_image_to(base_dir, source, "images")
}

pub fn import_image_to(
    base_dir: &Path,
    source: &Path,
    relative_root: &str,
) -> Result<StoredImage, String> {
    if !source.is_file() {
        return Err(format!("预览图片不存在：{}", source.display()));
    }
    let original_name = source
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("preview")
        .to_string();
    let ext = source
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("jpg")
        .to_ascii_lowercase();
    let allowed = ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];
    if !allowed.contains(&ext.as_str()) {
        return Err(format!("不支持的图片格式：{ext}"));
    }
    let id = Uuid::new_v4().to_string();
    let original_rel_path = format!("{relative_root}/originals/{id}.{ext}");
    let thumbnail_rel_path = format!("{relative_root}/thumbnails/{id}.webp");
    let original_target = base_dir.join(&original_rel_path);
    let thumbnail_target = base_dir.join(&thumbnail_rel_path);
    let image = image::open(source).map_err(|e| format!("读取图片失败：{e}"))?;
    let (pixel_width, pixel_height) = (image.width(), image.height());
    if pixel_width == 0 || pixel_height == 0 {
        return Err("图片尺寸无效".into());
    }
    if let Some(parent) = original_target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = thumbnail_target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(source, &original_target).map_err(|e| format!("复制图片失败：{e}"))?;
    let thumb = image.resize(480, 480, FilterType::Lanczos3);
    if let Err(error) = thumb.save_with_format(&thumbnail_target, image::ImageFormat::WebP) {
        let _ = fs::remove_file(&original_target);
        return Err(format!("生成缩略图失败：{error}"));
    }
    Ok(StoredImage {
        id,
        original_name,
        original_rel_path,
        thumbnail_rel_path,
        pixel_width,
        pixel_height,
    })
}

pub fn import_rgba_to(
    base_dir: &Path,
    relative_root: &str,
    width: u32,
    height: u32,
    bytes: &[u8],
) -> Result<StoredImage, String> {
    let image =
        image::RgbaImage::from_raw(width, height, bytes.to_vec()).ok_or("剪贴板图片数据无效")?;
    let id = Uuid::new_v4().to_string();
    let original_rel_path = format!("{relative_root}/originals/{id}.png");
    let thumbnail_rel_path = format!("{relative_root}/thumbnails/{id}.webp");
    let original_target = base_dir.join(&original_rel_path);
    let thumbnail_target = base_dir.join(&thumbnail_rel_path);
    if let Some(parent) = original_target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = thumbnail_target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let dynamic = image::DynamicImage::ImageRgba8(image);
    dynamic
        .save_with_format(&original_target, image::ImageFormat::Png)
        .map_err(|e| format!("保存剪贴板图片失败：{e}"))?;
    if let Err(error) = dynamic
        .resize(480, 480, FilterType::Lanczos3)
        .save_with_format(&thumbnail_target, image::ImageFormat::WebP)
    {
        let _ = fs::remove_file(&original_target);
        return Err(format!("生成缩略图失败：{error}"));
    }
    Ok(StoredImage {
        id,
        original_name: "剪贴板图片.png".into(),
        original_rel_path,
        thumbnail_rel_path,
        pixel_width: width,
        pixel_height: height,
    })
}

pub fn image_data_url(base_dir: &Path, relative: &str) -> Result<String, String> {
    let path = safe_join(base_dir, relative)?;
    let bytes = fs::read(&path).map_err(|e| format!("读取图片失败：{e}"))?;
    let mime = match path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        _ => "image/jpeg",
    };
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

pub fn safe_join(base_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("非法文件路径".into());
    }
    Ok(base_dir.join(relative_path))
}

pub fn remove_managed_file(base_dir: &Path, relative: &str) {
    if let Ok(path) = safe_join(base_dir, relative) {
        let _ = fs::remove_file(path);
    }
}

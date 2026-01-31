//! File upload handling with strict image validation
//!
//! Security measures:
//! - Validate magic bytes, not just Content-Type headers
//! - Only allow JPEG, PNG, GIF, WebP
//! - Strip EXIF metadata
//! - Generate thumbnails server-side
//! - Store with random filenames
//! - Enforce size limits

use anyhow::{anyhow, Result};
use image::{DynamicImage, GenericImageView, ImageFormat};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

/// Allowed image formats with their magic bytes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllowedFormat {
    Jpeg,
    Png,
    Gif,
    WebP,
}

impl AllowedFormat {
    /// Detect format from magic bytes (first 12 bytes)
    pub fn from_magic_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 12 {
            return None;
        }

        // JPEG: FF D8 FF
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(AllowedFormat::Jpeg);
        }

        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Some(AllowedFormat::Png);
        }

        // GIF: 47 49 46 38 (GIF87a or GIF89a)
        if bytes.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
            return Some(AllowedFormat::Gif);
        }

        // WebP: 52 49 46 46 ... 57 45 42 50 (RIFF....WEBP)
        if bytes.starts_with(&[0x52, 0x49, 0x46, 0x46]) && bytes[8..12] == [0x57, 0x45, 0x42, 0x50]
        {
            return Some(AllowedFormat::WebP);
        }

        None
    }

    pub fn extension(&self) -> &'static str {
        match self {
            AllowedFormat::Jpeg => "jpg",
            AllowedFormat::Png => "png",
            AllowedFormat::Gif => "gif",
            AllowedFormat::WebP => "webp",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            AllowedFormat::Jpeg => "image/jpeg",
            AllowedFormat::Png => "image/png",
            AllowedFormat::Gif => "image/gif",
            AllowedFormat::WebP => "image/webp",
        }
    }

    fn to_image_format(&self) -> ImageFormat {
        match self {
            AllowedFormat::Jpeg => ImageFormat::Jpeg,
            AllowedFormat::Png => ImageFormat::Png,
            AllowedFormat::Gif => ImageFormat::Gif,
            AllowedFormat::WebP => ImageFormat::WebP,
        }
    }
}

/// Result of processing an uploaded image
#[derive(Debug)]
pub struct ProcessedImage {
    /// Path to the stored full-size image (relative to uploads dir)
    pub file_path: String,
    /// Path to the thumbnail (relative to uploads dir)
    pub thumb_path: String,
    /// Original filename provided by user
    pub original_name: String,
    /// Detected MIME type
    pub mime_type: String,
    /// File size in bytes
    pub file_size: i64,
    /// Image width
    pub width: i32,
    /// Image height
    pub height: i32,
    /// Thumbnail width
    pub thumb_width: i32,
    /// Thumbnail height
    pub thumb_height: i32,
    /// SHA-256 hash of file content
    pub file_hash: String,
}

/// Configuration for file uploads
#[derive(Debug, Clone)]
pub struct UploadConfig {
    /// Directory to store uploads
    pub upload_dir: PathBuf,
    /// Maximum file size in bytes
    pub max_file_size: usize,
    /// Maximum image dimension (width or height)
    pub max_dimension: u32,
    /// Thumbnail max dimension
    pub thumb_size: u32,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            upload_dir: PathBuf::from("uploads"),
            max_file_size: 4 * 1024 * 1024, // 4MB
            max_dimension: 4096,
            thumb_size: 250,
        }
    }
}

/// Process and store an uploaded image
pub async fn process_upload(
    data: &[u8],
    original_name: &str,
    config: &UploadConfig,
) -> Result<ProcessedImage> {
    // Check size limit
    if data.len() > config.max_file_size {
        return Err(anyhow!(
            "File too large: {} bytes (max: {} bytes)",
            data.len(),
            config.max_file_size
        ));
    }

    // Validate magic bytes
    let format = AllowedFormat::from_magic_bytes(data)
        .ok_or_else(|| anyhow!("Invalid or unsupported image format. Allowed: JPEG, PNG, GIF, WebP"))?;

    // Calculate hash before any processing
    let file_hash = {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    };

    // CPU-intensive image processing in spawn_blocking to avoid blocking async runtime
    let data_owned = data.to_vec();
    let max_dimension = config.max_dimension;
    let thumb_size = config.thumb_size;
    let image_format = format.to_image_format();

    let (img, width, height, clean_data, thumb_width, thumb_height, thumb_data) =
        tokio::task::spawn_blocking(move || -> Result<_> {
            // Decode and validate the image
            let img = image::load_from_memory_with_format(&data_owned, image_format)
                .map_err(|e| anyhow!("Failed to decode image: {}", e))?;

            let (width, height) = img.dimensions();

            // Check dimensions
            if width > max_dimension || height > max_dimension {
                return Err(anyhow!(
                    "Image too large: {}x{} (max: {}x{})",
                    width,
                    height,
                    max_dimension,
                    max_dimension
                ));
            }

            // Re-encode image (strips EXIF and validates content)
            let clean_data = reencode_image_sync(&img, image_format)?;

            // Generate thumbnail
            let thumb = generate_thumbnail(&img, thumb_size);
            let (thumb_width, thumb_height) = thumb.dimensions();
            let thumb_data = reencode_image_sync(&thumb, image_format)?;

            Ok((img, width, height, clean_data, thumb_width, thumb_height, thumb_data))
        })
        .await
        .map_err(|e| anyhow!("Image processing task failed: {}", e))??;

    // Generate unique filename
    let file_id = Uuid::new_v4();
    let ext = format.extension();
    let file_name = format!("{}.{}", file_id, ext);
    let thumb_name = format!("{}_thumb.{}", file_id, ext);

    // Create directories
    let src_dir = config.upload_dir.join("src");
    let thumb_dir = config.upload_dir.join("thumb");
    fs::create_dir_all(&src_dir).await?;
    fs::create_dir_all(&thumb_dir).await?;

    // Save full-size image
    let file_path = src_dir.join(&file_name);
    fs::write(&file_path, &clean_data).await?;

    // Save thumbnail
    let thumb_path = thumb_dir.join(&thumb_name);
    fs::write(&thumb_path, &thumb_data).await?;

    // Suppress unused variable warning - img was used for dimensions
    let _ = img;

    Ok(ProcessedImage {
        file_path: format!("src/{}", file_name),
        thumb_path: format!("thumb/{}", thumb_name),
        original_name: sanitize_filename(original_name),
        mime_type: format.mime_type().to_string(),
        file_size: clean_data.len() as i64,
        width: width as i32,
        height: height as i32,
        thumb_width: thumb_width as i32,
        thumb_height: thumb_height as i32,
        file_hash,
    })
}

/// Re-encode image to strip metadata and validate content (sync version for spawn_blocking)
fn reencode_image_sync(img: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    img.write_to(&mut buffer, format)?;
    Ok(buffer.into_inner())
}

/// Generate a thumbnail that fits within max_size
fn generate_thumbnail(img: &DynamicImage, max_size: u32) -> DynamicImage {
    let (width, height) = img.dimensions();

    // If already small enough, return as-is
    if width <= max_size && height <= max_size {
        return img.clone();
    }

    // Calculate new dimensions maintaining aspect ratio
    let ratio = width as f64 / height as f64;
    let (new_width, new_height) = if width > height {
        (max_size, (max_size as f64 / ratio) as u32)
    } else {
        ((max_size as f64 * ratio) as u32, max_size)
    };

    img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
}

/// Sanitize filename to prevent path traversal
fn sanitize_filename(name: &str) -> String {
    // Get just the filename, no path components
    let name = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed");

    // Remove any remaining problematic characters
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .take(100) // Limit length
        .collect()
}

/// Check if a file with this hash already exists
pub async fn check_duplicate(db: &crate::db::Database, file_hash: &str) -> Result<Option<i64>> {
    let result: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM posts WHERE file_hash = $1 LIMIT 1"
    )
    .bind(file_hash)
    .fetch_optional(db.pool())
    .await?;

    Ok(result.map(|(id,)| id))
}

/// Delete a file and its thumbnail
pub async fn delete_file(upload_dir: &Path, file_path: &str, thumb_path: &str) -> Result<()> {
    let file_full_path = upload_dir.join(file_path);
    let thumb_full_path = upload_dir.join(thumb_path);

    if file_full_path.exists() {
        fs::remove_file(file_full_path).await?;
    }
    if thumb_full_path.exists() {
        fs::remove_file(thumb_full_path).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_bytes_jpeg() {
        let jpeg_magic = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01];
        assert_eq!(AllowedFormat::from_magic_bytes(&jpeg_magic), Some(AllowedFormat::Jpeg));
    }

    #[test]
    fn test_magic_bytes_png() {
        let png_magic = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D];
        assert_eq!(AllowedFormat::from_magic_bytes(&png_magic), Some(AllowedFormat::Png));
    }

    #[test]
    fn test_magic_bytes_invalid() {
        let invalid = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B];
        assert_eq!(AllowedFormat::from_magic_bytes(&invalid), None);
    }

    #[test]
    fn test_sanitize_filename() {
        // Path traversal is stripped, only filename component kept
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("normal_file.jpg"), "normal_file.jpg");
        assert_eq!(sanitize_filename("file with spaces.png"), "filewithspaces.png");
    }
}

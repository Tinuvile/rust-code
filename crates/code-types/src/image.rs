//! Image validation and utilities for API submissions.
//!
//! Validates image blocks before sending to the LLM API, checking:
//! - Supported media types
//! - Base64 data size limits
//! - URL format validity
//!
//! Ref: src/utils/messages.ts (validateImagesForAPI)

use crate::message::{ContentBlock, ImageBlock, ImageSource};

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum raw base64 data size (before decoding), in bytes.
/// Anthropic API limit is ~20 MB decoded; base64 is ~33% larger.
const MAX_BASE64_SIZE: usize = 20 * 1024 * 1024 * 4 / 3; // ~26.6 MB of base64 text

/// Supported media types for image content blocks.
const SUPPORTED_MEDIA_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
];

/// Supported media types for PDF documents.
const PDF_MEDIA_TYPE: &str = "application/pdf";

// ── Validation result ────────────────────────────────────────────────────────

/// An issue found during image validation.
#[derive(Debug, Clone)]
pub struct ImageIssue {
    /// Zero-based index of the content block in the message.
    pub block_index: usize,
    /// Kind of issue.
    pub kind: ImageIssueKind,
    /// Human-readable description.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageIssueKind {
    /// Media type is not supported by the API.
    UnsupportedMediaType,
    /// Base64 data exceeds the size limit.
    TooLarge,
    /// Base64 data is malformed (not valid base64 characters).
    MalformedBase64,
    /// URL is empty or invalid.
    InvalidUrl,
}

// ── Validation ───────────────────────────────────────────────────────────────

/// Validate all image blocks in a list of content blocks.
///
/// Returns a list of issues found.  An empty list means all images are valid.
pub fn validate_images(blocks: &[ContentBlock]) -> Vec<ImageIssue> {
    let mut issues = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if let ContentBlock::Image(img) = block {
            issues.extend(validate_image_block(img, i));
        }
    }
    issues
}

/// Validate a single image block.
fn validate_image_block(img: &ImageBlock, index: usize) -> Vec<ImageIssue> {
    let mut issues = Vec::new();

    match &img.source {
        ImageSource::Base64 { media_type, data } => {
            // Check media type.
            if !SUPPORTED_MEDIA_TYPES.contains(&media_type.as_str())
                && media_type != PDF_MEDIA_TYPE
            {
                issues.push(ImageIssue {
                    block_index: index,
                    kind: ImageIssueKind::UnsupportedMediaType,
                    message: format!(
                        "Unsupported media type '{media_type}'. Supported: {}",
                        SUPPORTED_MEDIA_TYPES.join(", ")
                    ),
                });
            }

            // Check base64 data size.
            if data.len() > MAX_BASE64_SIZE {
                let mb = data.len() as f64 / (1024.0 * 1024.0);
                issues.push(ImageIssue {
                    block_index: index,
                    kind: ImageIssueKind::TooLarge,
                    message: format!(
                        "Image data is too large ({mb:.1} MB). \
                         Maximum allowed is ~20 MB (decoded)."
                    ),
                });
            }

            // Check base64 validity (light check — just characters).
            if !data.is_empty() && !is_valid_base64_chars(data) {
                issues.push(ImageIssue {
                    block_index: index,
                    kind: ImageIssueKind::MalformedBase64,
                    message: "Image data contains invalid base64 characters.".to_owned(),
                });
            }
        }
        ImageSource::Url { url } => {
            if url.is_empty() {
                issues.push(ImageIssue {
                    block_index: index,
                    kind: ImageIssueKind::InvalidUrl,
                    message: "Image URL is empty.".to_owned(),
                });
            } else if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("data:") {
                issues.push(ImageIssue {
                    block_index: index,
                    kind: ImageIssueKind::InvalidUrl,
                    message: format!("Image URL must start with http://, https://, or data: — got '{url}'."),
                });
            }
        }
    }

    issues
}

/// Check that a string contains only valid base64 characters.
fn is_valid_base64_chars(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=' || b == b'\n' || b == b'\r')
}

/// Filter out invalid image blocks from a content block list.
///
/// Returns `(valid_blocks, issues)`.  Invalid images are replaced with a
/// text block describing the error so the model knows an image was intended.
pub fn filter_invalid_images(blocks: Vec<ContentBlock>) -> (Vec<ContentBlock>, Vec<ImageIssue>) {
    let issues = validate_images(&blocks);
    if issues.is_empty() {
        return (blocks, vec![]);
    }

    let bad_indices: std::collections::HashSet<usize> = issues.iter().map(|i| i.block_index).collect();

    let filtered = blocks
        .into_iter()
        .enumerate()
        .map(|(i, block)| {
            if bad_indices.contains(&i) {
                // Replace the invalid image with an error text block.
                let msg = issues
                    .iter()
                    .filter(|issue| issue.block_index == i)
                    .map(|issue| issue.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                ContentBlock::text(format!("[Image validation error: {msg}]"))
            } else {
                block
            }
        })
        .collect();

    (filtered, issues)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ImageBlock, ImageSource};

    fn base64_image(media_type: &str, data: &str) -> ContentBlock {
        ContentBlock::Image(ImageBlock {
            source: ImageSource::Base64 {
                media_type: media_type.to_owned(),
                data: data.to_owned(),
            },
            cache_control: None,
        })
    }

    fn url_image(url: &str) -> ContentBlock {
        ContentBlock::Image(ImageBlock {
            source: ImageSource::Url {
                url: url.to_owned(),
            },
            cache_control: None,
        })
    }

    #[test]
    fn valid_png_passes() {
        let blocks = vec![base64_image("image/png", "iVBORw0KGgo=")];
        assert!(validate_images(&blocks).is_empty());
    }

    #[test]
    fn valid_url_passes() {
        let blocks = vec![url_image("https://example.com/logo.png")];
        assert!(validate_images(&blocks).is_empty());
    }

    #[test]
    fn unsupported_media_type() {
        let blocks = vec![base64_image("image/bmp", "data")];
        let issues = validate_images(&blocks);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, ImageIssueKind::UnsupportedMediaType);
    }

    #[test]
    fn too_large_image() {
        let huge_data = "A".repeat(MAX_BASE64_SIZE + 1);
        let blocks = vec![base64_image("image/png", &huge_data)];
        let issues = validate_images(&blocks);
        assert!(issues.iter().any(|i| i.kind == ImageIssueKind::TooLarge));
    }

    #[test]
    fn malformed_base64() {
        let blocks = vec![base64_image("image/png", "not valid base64!!!")];
        let issues = validate_images(&blocks);
        assert!(
            issues.iter().any(|i| i.kind == ImageIssueKind::MalformedBase64),
            "expected MalformedBase64, got {issues:?}"
        );
    }

    #[test]
    fn empty_url() {
        let blocks = vec![url_image("")];
        let issues = validate_images(&blocks);
        assert_eq!(issues[0].kind, ImageIssueKind::InvalidUrl);
    }

    #[test]
    fn invalid_url_scheme() {
        let blocks = vec![url_image("ftp://example.com/image.png")];
        let issues = validate_images(&blocks);
        assert_eq!(issues[0].kind, ImageIssueKind::InvalidUrl);
    }

    #[test]
    fn data_url_is_valid() {
        let blocks = vec![url_image("data:image/png;base64,iVBOR")];
        assert!(validate_images(&blocks).is_empty());
    }

    #[test]
    fn filter_replaces_invalid() {
        let blocks = vec![
            ContentBlock::text("Hello"),
            base64_image("image/bmp", "data"),
            ContentBlock::text("World"),
        ];
        let (filtered, issues) = filter_invalid_images(blocks);
        assert_eq!(filtered.len(), 3);
        assert_eq!(issues.len(), 1);
        // The middle block should be replaced with error text.
        if let ContentBlock::Text(t) = &filtered[1] {
            assert!(t.text.contains("Image validation error"));
        } else {
            panic!("expected text replacement for invalid image");
        }
    }

    #[test]
    fn text_only_passes() {
        let blocks = vec![ContentBlock::text("Just text")];
        assert!(validate_images(&blocks).is_empty());
    }

    #[test]
    fn pdf_media_type_passes() {
        let blocks = vec![base64_image("application/pdf", "JVBERi0xLjQ=")];
        assert!(validate_images(&blocks).is_empty());
    }
}

//! Worker side of the image flows (W2-L1-beeld): decoding bitmaps, rendering the processed
//! pixels of an image operation for the live preview, and writing them as a PNG. Pure functions
//! over their inputs: no controller, no document, no UI.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_cam::processed_bitmap;
use burnburn_document::DocumentSnapshot;
use burnburn_document::process::{ImageSettings, OperationMode};
use burnburn_geometry::{Path, ProductionTolerance};
use burnburn_image::{DecodeLimits, Engraved, RasterError, SourceImage, decode, encode_png_gray8};
use burnburn_io::AssetStore;
use burnburn_io::fs::FileSystem;
use burnburn_types::{CancellationToken, ContentHash};

use crate::image_panel::ImagePreview;
use crate::task::{TaskError, TaskOutput};

/// Where a bitmap is read from.
#[derive(Clone)]
pub enum BitmapInput {
    /// A file chosen by the user (import, replace, refresh).
    File { path: PathBuf, fs: Arc<dyn FileSystem> },
    /// The stored asset whose decoded pixels hash to `wanted` (a project opened from disk).
    Stored { assets: AssetStore, wanted: ContentHash },
}

impl fmt::Debug for BitmapInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BitmapInput::File { path, .. } => write!(f, "File({})", path.display()),
            BitmapInput::Stored { wanted, .. } => write!(f, "Stored({wanted:?})"),
        }
    }
}

/// The original pixels of an image operation: already decoded, or still stored in the project
/// (a project opened from disk) and decoded on the worker.
#[derive(Clone)]
pub enum BitmapSource {
    Decoded(Arc<SourceImage>),
    Stored { assets: AssetStore, wanted: ContentHash },
}

/// Everything needed to render the processed pixels of one image frame.
#[derive(Clone)]
pub struct BitmapJob {
    pub settings: ImageSettings,
    pub source: BitmapSource,
    /// The target geometry the source is stretched over.
    pub outline: Path,
    pub tolerance: ProductionTolerance,
}

/// Largest image file read for the image flows (the decoder also limits pixels).
const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) fn load_bitmap(input: BitmapInput, cancel: &CancellationToken) -> Result<TaskOutput, TaskError> {
    cancel.check().map_err(|_| TaskError::Cancelled)?;
    match input {
        BitmapInput::File { path, fs } => {
            let bytes = fs.read(&path, MAX_IMAGE_BYTES).map_err(TaskError::Read)?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let source = decode(&bytes, DecodeLimits::default()).map_err(TaskError::Decode)?;
            Ok(TaskOutput::ImageLoaded { source: Arc::new(source), bytes: Arc::from(bytes), path })
        }
        BitmapInput::Stored { assets, wanted } => {
            let source = resolve(&BitmapSource::Stored { assets: assets.clone(), wanted }, cancel)?;
            let bytes = assets
                .iter()
                .find(|(_, a)| decode(&a.bytes, DecodeLimits::default()).is_ok_and(|i| i.content_hash() == wanted))
                .map(|(_, a)| Arc::clone(&a.bytes))
                .ok_or(TaskError::SourceMissing)?;
            Ok(TaskOutput::ImageLoaded { source, bytes, path: PathBuf::new() })
        }
    }
}

/// The decoded original pixels of `source`; a stored one is found by its pixel hash among the
/// project's PNG/JPEG assets.
fn resolve(source: &BitmapSource, cancel: &CancellationToken) -> Result<Arc<SourceImage>, TaskError> {
    match source {
        BitmapSource::Decoded(image) => Ok(Arc::clone(image)),
        BitmapSource::Stored { assets, wanted } => {
            for (_, asset) in assets.iter().filter(|(_, a)| is_decodable(&a.media)) {
                cancel.check().map_err(|_| TaskError::Cancelled)?;
                if let Ok(image) = decode(&asset.bytes, DecodeLimits::default())
                    && image.content_hash() == *wanted
                {
                    return Ok(Arc::new(image));
                }
            }
            Err(TaskError::SourceMissing)
        }
    }
}

fn is_decodable(media: &str) -> bool {
    matches!(media, "image/png" | "image/jpeg")
}

fn processed(job: &BitmapJob, cancel: &CancellationToken) -> Result<(Engraved, Arc<SourceImage>), TaskError> {
    let source = resolve(&job.source, cancel)?;
    let engraved =
        processed_bitmap(&job.settings, &source, &job.outline, &job.tolerance, cancel).map_err(|e| match e {
            RasterError::Cancelled => TaskError::Cancelled,
            e => TaskError::Raster(e),
        })?;
    Ok((engraved, source))
}

pub(crate) fn render_preview(job: &BitmapJob, cancel: &CancellationToken) -> Result<TaskOutput, TaskError> {
    let (engraved, source) = processed(job, cancel)?;
    let preview =
        ImagePreview { width: engraved.width(), height: engraved.height(), gray8: Arc::from(engraved.to_gray8()) };
    Ok(TaskOutput::ImagePreview { preview, source })
}

/// PNG bytes of the processed pixels with their size.
pub(crate) fn processed_png(job: &BitmapJob, cancel: &CancellationToken) -> Result<(Vec<u8>, u32, u32), TaskError> {
    let (engraved, _) = processed(job, cancel)?;
    let (width, height) = (engraved.width(), engraved.height());
    let bytes = encode_png_gray8(width, height, &engraved.to_gray8()).map_err(TaskError::Raster)?;
    Ok((bytes, width, height))
}

/// The decoded source pixels of every image operation in `snapshot`, found among `assets`.
pub(crate) fn compile_rasters(snapshot: &DocumentSnapshot, assets: &AssetStore) -> BTreeMap<ContentHash, SourceImage> {
    let mut wanted: Vec<ContentHash> = snapshot
        .process()
        .operations()
        .filter_map(|op| match op.mode() {
            OperationMode::Image(s) => Some(s.source()),
            _ => None,
        })
        .collect();
    let mut found = BTreeMap::new();
    for (_, asset) in assets.iter().filter(|(_, a)| is_decodable(&a.media)) {
        if wanted.is_empty() {
            break;
        }
        if let Ok(image) = decode(&asset.bytes, DecodeLimits::default())
            && wanted.contains(&image.content_hash())
        {
            wanted.retain(|h| *h != image.content_hash());
            found.insert(image.content_hash(), image);
        }
    }
    found
}

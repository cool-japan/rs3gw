//! Video transformation implementation

use super::image::Transformer;
use super::types::*;
use async_trait::async_trait;

#[cfg(feature = "video-transcoding")]
use bytes::Bytes;
#[cfg(feature = "video-transcoding")]
use std::io::Write;
#[cfg(feature = "video-transcoding")]
use std::path::PathBuf;

/// Video transformer
pub struct VideoTransformer;

#[async_trait]
impl Transformer for VideoTransformer {
    fn supports(&self, transformation: &TransformationType) -> bool {
        matches!(transformation, TransformationType::Video(_))
    }

    async fn transform(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Result<TransformationResult, TransformationError> {
        let TransformationType::Video(params) = transformation else {
            return Err(TransformationError::UnsupportedFormat(
                "Not a video transformation".to_string(),
            ));
        };

        #[cfg(feature = "video-transcoding")]
        {
            transcode_video(data, params).await
        }

        #[cfg(not(feature = "video-transcoding"))]
        {
            let _ = (params, data);
            Err(TransformationError::VideoError(
                "Video transcoding not enabled (compile with --features video-transcoding)"
                    .to_string(),
            ))
        }
    }
}

#[cfg(feature = "video-transcoding")]
async fn transcode_video(
    data: &[u8],
    params: &VideoTransformParams,
) -> Result<TransformationResult, TransformationError> {
    // Initialize FFmpeg
    ffmpeg_next::init()
        .map_err(|e| TransformationError::VideoError(format!("FFmpeg init failed: {}", e)))?;

    // Create temporary input file
    let temp_dir = std::env::temp_dir();
    let input_path = temp_dir.join(format!("input_{}.tmp", uuid::Uuid::new_v4()));
    let output_path = temp_dir.join(format!("output_{}.tmp", uuid::Uuid::new_v4()));

    // Write input data to temp file
    let mut input_file =
        std::fs::File::create(&input_path).map_err(TransformationError::IoError)?;
    input_file
        .write_all(data)
        .map_err(TransformationError::IoError)?;
    drop(input_file);

    // Perform transcoding
    let result = perform_transcode(&input_path, &output_path, params).await;

    // Clean up input file
    let _ = std::fs::remove_file(&input_path);

    // Read output or clean up on error
    match result {
        Ok(metadata) => {
            let output_data = tokio::fs::read(&output_path)
                .await
                .map_err(TransformationError::IoError)?;
            let _ = std::fs::remove_file(&output_path);

            let output_len = output_data.len();
            let mut result = TransformationResult::new(Bytes::from(output_data), "video/mp4");
            result = result
                .with_metadata("codec", params.codec.as_str())
                .with_metadata(
                    "original_size",
                    metadata
                        .get("original_size")
                        .unwrap_or(&"unknown".to_string())
                        .clone(),
                )
                .with_metadata("output_size", output_len.to_string());

            if let Some(bitrate) = params.bitrate {
                result = result.with_metadata("bitrate", bitrate.to_string());
            }
            if let Some(fps) = params.fps {
                result = result.with_metadata("fps", fps.to_string());
            }

            Ok(result)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&output_path);
            Err(e)
        }
    }
}

#[cfg(feature = "video-transcoding")]
async fn perform_transcode(
    input_path: &PathBuf,
    output_path: &PathBuf,
    params: &VideoTransformParams,
) -> Result<std::collections::HashMap<String, String>, TransformationError> {
    use ffmpeg_next as ffmpeg;

    let mut metadata = std::collections::HashMap::new();

    // Open input
    let input_ctx = ffmpeg::format::input(&input_path)
        .map_err(|e| TransformationError::VideoError(format!("Failed to open input: {}", e)))?;

    // Find video stream
    let input_stream = input_ctx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| TransformationError::VideoError("No video stream found".to_string()))?;

    let video_stream_index = input_stream.index();

    metadata.insert(
        "original_size".to_string(),
        input_ctx.duration().to_string(),
    );

    // Create output
    let output_ctx = ffmpeg::format::output(&output_path)
        .map_err(|e| TransformationError::VideoError(format!("Failed to create output: {}", e)))?;

    // Set up transcoding parameters
    let codec_name = params.codec.as_str();
    let encoder = ffmpeg::encoder::find_by_name(codec_name).ok_or_else(|| {
        TransformationError::VideoError(format!("Codec {} not found", codec_name))
    })?;

    // This is a simplified implementation
    // A full implementation would:
    // 1. Set up proper decoder
    // 2. Configure encoder with bitrate, resolution, fps
    // 3. Process frames
    // 4. Handle audio streams
    // 5. Write output

    // For now, return error indicating this needs full implementation
    let _ = (output_ctx, encoder, video_stream_index);

    Err(TransformationError::VideoError(
        "Full video transcoding implementation requires additional FFmpeg setup".to_string(),
    ))
}

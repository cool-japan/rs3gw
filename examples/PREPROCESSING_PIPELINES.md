# Dataset Preprocessing Pipelines

This directory contains example preprocessing pipelines for common AI/ML workflows with rs3gw.

## Overview

rs3gw provides a comprehensive preprocessing pipeline system that allows you to transform data on-the-fly for AI/ML workloads. Pipelines are defined as JSON configuration files and can be applied via HTTP API or CLI.

## Available Pipelines

### 1. ImageNet Classification (`imagenet_pipeline.json`)

**Use Case**: Standard image classification with pre-trained ImageNet models

**Pipeline Steps**:
- Resize to 224x224 (Lanczos3 filter)
- Normalize with ImageNet mean/std ([0.485, 0.456, 0.406] / [0.229, 0.224, 0.225])

**Target Models**: ResNet, VGG, Inception, EfficientNet

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @imagenet_pipeline.json

# Apply to an image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "imagenet-preprocessing",
    "bucket": "images",
    "key": "cat.jpg",
    "output_bucket": "processed-images",
    "output_key": "cat_preprocessed.jpg"
  }'
```

### 2. Training Data Augmentation (`augmentation_pipeline.json`)

**Use Case**: Data augmentation for robust training

**Pipeline Steps**:
- Resize to 256x256
- Random augmentation (flip, rotation, brightness, contrast)
- Normalize to [-1, 1] range

**Target Models**: Any image classification model during training

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @augmentation_pipeline.json

# Apply to training data
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "training-augmentation",
    "bucket": "training-data",
    "key": "samples/img_001.jpg",
    "output_bucket": "augmented-data",
    "output_key": "samples/img_001_aug.jpg"
  }'
```

### 3. Medical Imaging (`medical_imaging_pipeline.json`)

**Use Case**: Diagnostic AI for medical imaging (CT, MRI, X-Ray)

**Pipeline Steps**:
- Resize to 512x512 (preserving aspect ratio)
- Normalize grayscale to [-1, 1] range

**Target Models**: Medical diagnostic CNNs

**Compliance**: HIPAA-compliant pipeline

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @medical_imaging_pipeline.json

# Apply to medical image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "medical-imaging-preprocessing",
    "bucket": "medical-images",
    "key": "patient-123/ct-scan.dcm",
    "output_bucket": "processed-medical",
    "output_key": "patient-123/ct-scan_preprocessed.jpg"
  }'
```

### 4. NLP Text Preprocessing (`nlp_text_pipeline.json`)

**Use Case**: Text classification, sentiment analysis, NLU

**Pipeline Steps**:
- WordPiece tokenization (BERT-compatible)
- Truncation and padding to max_length=512

**Target Models**: BERT, RoBERTa, DistilBERT

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @nlp_text_pipeline.json

# Apply to text file
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "nlp-text-preprocessing",
    "bucket": "text-data",
    "key": "reviews/review_001.txt",
    "output_bucket": "tokenized-text",
    "output_key": "reviews/review_001_tokens.json"
  }'
```

### 5. Object Detection (`object_detection_pipeline.json`)

**Use Case**: Real-time object detection

**Pipeline Steps**:
- Resize to 640x640 with padding
- Normalize to [0, 1] range

**Target Models**: YOLO (v5-v9), Faster R-CNN, RetinaNet

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @object_detection_pipeline.json

# Apply to detection image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "object-detection-preprocessing",
    "bucket": "detection-images",
    "key": "street_scene.jpg",
    "output_bucket": "yolo-ready",
    "output_key": "street_scene_yolo.jpg"
  }'
```

### 6. Video Frame Extraction (`video_preprocessing_pipeline.json`)

**Use Case**: Action recognition and video classification

**Pipeline Steps**:
- Extract 16 frames uniformly from video
- Resize frames to 224x224
- Normalize frames

**Target Models**: I3D, SlowFast, TimeSformer, Video Transformers

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @video_preprocessing_pipeline.json

# Apply to video
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "video-frame-extraction",
    "bucket": "video-data",
    "key": "actions/jump.mp4",
    "output_bucket": "video-frames",
    "output_key": "actions/jump_frames.tar"
  }'
```

### 7. Audio Feature Extraction (`audio_preprocessing_pipeline.json`)

**Use Case**: Speech recognition and audio classification

**Pipeline Steps**:
- Extract Mel spectrogram features
- Sample rate: 16kHz
- 128 Mel bands, normalized

**Target Models**: Wav2Vec2, Whisper, Audio Classifiers

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @audio_preprocessing_pipeline.json

# Apply to audio file
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "audio-feature-extraction",
    "bucket": "audio-data",
    "key": "speech/sample_001.wav",
    "output_bucket": "audio-features",
    "output_key": "speech/sample_001_mels.npy"
  }'
```

### 6. CLIP (OpenAI) Multimodal AI (`clip_pipeline.json`)

**Use Case**: Multimodal AI, image-text retrieval, zero-shot classification

**Pipeline Steps**:
- Resize to 224x224 (bicubic filter)
- Normalize with CLIP-specific mean/std ([0.481, 0.458, 0.408] / [0.269, 0.261, 0.276])

**Target Models**: OpenAI CLIP (ViT-B/32, ViT-L/14), CLIP-based models

**Framework**: PyTorch, HuggingFace Transformers

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @clip_pipeline.json

# Apply to an image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "clip-preprocessing",
    "bucket": "images",
    "key": "photo.jpg",
    "output_bucket": "clip-processed",
    "output_key": "photo_clip.jpg"
  }'
```

### 7. DINOv2 (Meta) Self-Supervised Learning (`dinov2_pipeline.json`)

**Use Case**: Self-supervised learning, feature extraction, dense prediction tasks

**Pipeline Steps**:
- Resize to 518x518 (bicubic filter, larger than standard models)
- Normalize with ImageNet mean/std ([0.485, 0.456, 0.406] / [0.229, 0.224, 0.225])

**Target Models**: Meta DINOv2 (ViT-S/14, ViT-B/14, ViT-L/14, ViT-g/14)

**Framework**: PyTorch, HuggingFace Transformers

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @dinov2_pipeline.json

# Apply to an image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "dinov2-preprocessing",
    "bucket": "images",
    "key": "scene.jpg",
    "output_bucket": "dinov2-processed",
    "output_key": "scene_dinov2.jpg"
  }'
```

### 8. Vision Transformer (ViT) (`vit_pipeline.json`)

**Use Case**: Image classification, transfer learning, feature extraction with transformers

**Pipeline Steps**:
- Resize to 224x224 (bicubic filter, ViT-Base) or 384x384 (ViT-Large)
- Normalize to [-1, 1] range ([0.5, 0.5, 0.5] / [0.5, 0.5, 0.5])

**Target Models**: Vision Transformer (ViT-Base/16, ViT-Base/32, ViT-Large)

**Framework**: PyTorch, HuggingFace Transformers, JAX

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @vit_pipeline.json

# Apply to an image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "vit-base-preprocessing",
    "bucket": "images",
    "key": "landscape.jpg",
    "output_bucket": "vit-processed",
    "output_key": "landscape_vit.jpg"
  }'
```

### 9. EfficientNet Efficient Classification (`efficientnet_pipeline.json`)

**Use Case**: Efficient image classification, mobile deployment, edge computing

**Pipeline Steps**:
- Resize to 224x224 (bicubic filter, EfficientNet-B0) - scales up for B1-B7
- Normalize with ImageNet mean/std ([0.485, 0.456, 0.406] / [0.229, 0.224, 0.225])

**Target Models**: EfficientNet-B0 through B7 (compound scaled architectures)

**Framework**: PyTorch, TensorFlow, HuggingFace

**Note**: EfficientNet uses different input resolutions:
- B0: 224x224, B1: 240x240, B2: 260x260, B3: 300x300
- B4: 380x380, B5: 456x456, B6: 528x528, B7: 600x600

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @efficientnet_pipeline.json

# Apply to an image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "efficientnet-b0-preprocessing",
    "bucket": "images",
    "key": "mobile_capture.jpg",
    "output_bucket": "efficient-processed",
    "output_key": "mobile_capture_b0.jpg"
  }'
```

### 10. MobileNet Mobile & Edge Deployment (`mobilenet_pipeline.json`)

**Use Case**: Mobile applications, edge devices, real-time inference, IoT

**Pipeline Steps**:
- Resize to 224x224 (bilinear filter, optimized for mobile)
- Normalize with ImageNet mean/std ([0.485, 0.456, 0.406] / [0.229, 0.224, 0.225])

**Target Models**: MobileNetV1, MobileNetV2, MobileNetV3

**Framework**: PyTorch, TensorFlow, TensorFlow Lite, CoreML

**Example Usage**:
```bash
# Register pipeline
curl -X POST http://localhost:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @mobilenet_pipeline.json

# Apply to an image
curl -X POST http://localhost:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "mobilenet-v2-preprocessing",
    "bucket": "images",
    "key": "mobile_photo.jpg",
    "output_bucket": "mobile-processed",
    "output_key": "mobile_photo_mobilenet.jpg"
  }'
```

## CLI Usage with rs3ctl

You can also use the `rs3ctl` CLI tool for preprocessing operations:

```bash
# Create a pipeline
rs3ctl preprocessing create imagenet_pipeline.json

# List all pipelines
rs3ctl preprocessing list

# Get pipeline details
rs3ctl preprocessing get imagenet-preprocessing

# Apply a pipeline
rs3ctl preprocessing apply imagenet-preprocessing \
  --bucket images \
  --key photo.jpg \
  --output-bucket processed-images \
  --output-key photo_preprocessed.jpg

# Delete a pipeline
rs3ctl preprocessing delete imagenet-preprocessing

# Get cache statistics
rs3ctl preprocessing cache-stats

# Clear cache
rs3ctl preprocessing cache-clear
```

## Pipeline Configuration Structure

All pipelines follow this JSON schema:

```json
{
  "id": "unique-pipeline-id",
  "name": "Human Readable Name",
  "version": "1.0.0",
  "description": "What this pipeline does",
  "steps": [
    {
      "id": "step-id",
      "step_type": "operation_type",
      "config": {
        // Step-specific configuration
      },
      "cache_results": true,
      "description": "What this step does"
    }
  ],
  "metadata": {
    "created_at": "2025-12-31T00:00:00Z",
    "author": "team-name",
    "target_model": "Model architecture",
    "dataset": "Dataset name",
    "use_case": "Primary use case",
    "recommended_batch_size": 16,
    "notes": "Additional notes"
  }
}
```

## Supported Step Types

### Image Operations

#### `image_resize`
```json
{
  "width": 224,
  "height": 224,
  "mode": "fit|fill|exact",
  "filter": "nearest|bilinear|lanczos3"
}
```

#### `image_normalization`
```json
{
  "mean": [0.485, 0.456, 0.406],
  "std": [0.229, 0.224, 0.225],
  "normalize_range": true
}
```

#### `data_augmentation`
```json
{
  "horizontal_flip_prob": 0.5,
  "vertical_flip_prob": 0.0,
  "rotation_range": 15.0,
  "brightness_range": [0.8, 1.2],
  "contrast_range": [0.8, 1.2]
}
```

### Text Operations

#### `text_tokenization`
```json
{
  "tokenizer_type": "wordpiece|bpe|sentencepiece",
  "max_length": 512,
  "truncation": true,
  "padding": "max_length",
  "lowercase": true
}
```

### Audio Operations

#### `audio_features`
```json
{
  "feature_type": "mel_spectrogram|mfcc|stft",
  "sample_rate": 16000,
  "n_fft": 2048,
  "hop_length": 512,
  "n_mels": 128
}
```

### Video Operations

#### `video_frames`
```json
{
  "fps": 1,
  "max_frames": 16,
  "sampling_strategy": "uniform|keyframes",
  "output_format": "rgb|bgr|grayscale"
}
```

## Performance Optimization

### Caching

Enable caching for expensive operations:
```json
{
  "id": "expensive-step",
  "cache_results": true,
  ...
}
```

Cached results are stored with SHA256-based keys and automatically expire based on TTL.

### Batch Processing

Process multiple objects in parallel:
```python
import requests

# Apply pipeline to multiple objects
objects = ["img1.jpg", "img2.jpg", "img3.jpg"]
for obj in objects:
    requests.post("http://localhost:9000/api/preprocessing/apply", json={
        "pipeline_id": "imagenet-preprocessing",
        "bucket": "images",
        "key": obj,
        "output_bucket": "processed",
        "output_key": f"processed_{obj}"
    })
```

## Integration with Arrow Flight

For high-performance data transfer, combine preprocessing with Arrow Flight:

```python
import pyarrow.flight as flight
from arrow_flight_pandas import RS3FlightClient

# Connect to rs3gw
client = RS3FlightClient(host="localhost", port=9000)

# List preprocessed objects as DataFrame
df = client.list_objects_as_dataframe("processed-images")

# Access preprocessed data via Flight (zero-copy)
data = client.get_object("processed-images", "cat_preprocessed.jpg")
```

## Best Practices

1. **Use Caching Wisely**: Enable caching for deterministic, expensive operations
2. **Pipeline Versioning**: Include version numbers in pipeline IDs
3. **Metadata Documentation**: Document target models, datasets, and use cases
4. **Batch Size Tuning**: Adjust batch sizes based on your infrastructure
5. **Error Handling**: Monitor preprocessing failures via observability endpoints
6. **Testing**: Validate pipelines with sample data before production deployment

## Monitoring

Monitor preprocessing performance:

```bash
# Get cache statistics
curl http://localhost:9000/api/preprocessing/cache/stats

# Get resource usage
curl http://localhost:9000/api/observability/profiling

# Get business metrics
curl http://localhost:9000/api/observability/business-metrics
```

## Troubleshooting

### Pipeline Not Found
```bash
# List all pipelines to verify ID
curl http://localhost:9000/api/preprocessing/pipelines
```

### Out of Memory
```bash
# Check cache size and clear if needed
curl -X POST http://localhost:9000/api/preprocessing/cache/clear

# Reduce batch size in pipeline metadata
```

### Slow Performance
```bash
# Enable caching for expensive steps
# Increase RS3GW_CACHE_MAX_SIZE_MB environment variable
# Use Arrow Flight for large data transfers
```

## See Also

- [Arrow Flight Integration Guide](ARROW_FLIGHT_README.md)
- [PyTorch Integration Examples](pytorch_integration.py)
- [TensorFlow Integration Examples](tensorflow_integration.py)
- [Production Deployment Guide](../docs/production_deployment.md)

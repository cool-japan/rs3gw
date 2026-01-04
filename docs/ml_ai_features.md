# rs3gw ML/AI Features Guide

This guide provides comprehensive documentation for using rs3gw's ML/AI capabilities including model registry, dataset version control, and preprocessing pipelines.

## Table of Contents

- [Overview](#overview)
- [Model Registry](#model-registry)
- [Dataset Version Control](#dataset-version-control)
- [Preprocessing Pipelines](#preprocessing-pipelines)
- [Tensor Storage Optimizations](#tensor-storage-optimizations)
- [Production Workflows](#production-workflows)
- [Integration Examples](#integration-examples)
- [Best Practices](#best-practices)

---

## Overview

rs3gw v5.0.0+ includes enterprise-grade ML/AI features designed for modern machine learning workflows:

- **Model Registry**: MLflow-compatible model versioning with lineage tracking
- **Dataset Version Control**: Git-like versioning for datasets with train/val/test split management
- **Preprocessing Pipelines**: Declarative data transformation pipelines with caching
- **Tensor Optimizations**: Format-aware storage for PyTorch, TensorFlow, ONNX, and Safetensors
- **Provenance Tracking**: Complete lineage from datasets to trained models

### Key Benefits

✅ **Reproducibility**: Track exact dataset versions used for each model
✅ **Collaboration**: Share models and datasets with complete metadata
✅ **Performance**: Optimized storage and caching for ML workloads
✅ **Compatibility**: Works with existing PyTorch, TensorFlow, and HuggingFace workflows
✅ **Enterprise-Ready**: Full audit trails and compliance tracking

---

## Model Registry

### Overview

The model registry provides MLflow-compatible model management with versioning, metadata extraction, and deployment tracking.

### Features

- **Automatic Format Detection**: PyTorch (.pt, .pth), TensorFlow (SavedModel), ONNX (.onnx), Safetensors
- **Versioning**: Monotonic version numbering with immutable history
- **Metadata Extraction**: Framework, architecture, parameter counts, tensor shapes
- **Stage Management**: Development, Staging, Production, Archived
- **Provenance Tracking**: Link models to training datasets and hyperparameters
- **A/B Testing**: Multiple model versions in different stages

### Uploading Models

Models are automatically registered when uploaded to the `models/` prefix:

```python
import boto3

s3 = boto3.client('s3', endpoint_url='http://rs3gw:9000')

# Upload PyTorch model
s3.upload_file(
    'resnet50.pt',
    'ml-models',
    'models/image-classifier/v1.0.0/model.pt',
    ExtraArgs={
        'Metadata': {
            'framework': 'pytorch',
            'architecture': 'resnet50',
            'params': '25000000',
            'task': 'image-classification',
            'dataset': 'imagenet',
            'accuracy': '0.92'
        }
    }
)
```

### Model Metadata Schema

rs3gw automatically extracts and enhances model metadata:

```json
{
  "model_id": "image-classifier",
  "version": "v1.0.0",
  "format": "PyTorch",
  "framework": "pytorch",
  "architecture": "resnet50",
  "parameters": 25000000,
  "tensor_info": {
    "input_shape": [1, 3, 224, 224],
    "output_shape": [1, 1000],
    "dtypes": ["float32"]
  },
  "provenance": {
    "training_dataset": "imagenet-v1",
    "training_script": "train_resnet.py",
    "hyperparameters": {
      "learning_rate": 0.001,
      "batch_size": 32,
      "epochs": 100
    }
  },
  "stage": "Production",
  "created_at": "2025-01-02T10:30:00Z"
}
```

### Querying Models via S3 API

```python
# List all models
response = s3.list_objects_v2(Bucket='ml-models', Prefix='models/')

for obj in response['Contents']:
    # Get model metadata
    metadata = s3.head_object(Bucket='ml-models', Key=obj['Key'])
    print(f"Model: {obj['Key']}")
    print(f"Framework: {metadata['Metadata'].get('framework')}")
    print(f"Architecture: {metadata['Metadata'].get('architecture')}")
```

### Model Lifecycle Management

```python
# Upload new model version
s3.upload_file('resnet50_v2.pt', 'ml-models', 'models/image-classifier/v2.0.0/model.pt')

# Tag for different stages
s3.put_object_tagging(
    Bucket='ml-models',
    Key='models/image-classifier/v1.0.0/model.pt',
    Tagging={'TagSet': [{'Key': 'stage', 'Value': 'production'}]}
)

s3.put_object_tagging(
    Bucket='ml-models',
    Key='models/image-classifier/v2.0.0/model.pt',
    Tagging={'TagSet': [{'Key': 'stage', 'Value': 'staging'}]}
)
```

### Supported Model Formats

| Format | Extensions | Auto-Detection | Metadata Extraction |
|--------|-----------|----------------|---------------------|
| PyTorch | .pt, .pth | ✅ | Architecture, params, shapes |
| TensorFlow | SavedModel | ✅ | Signatures, variables, shapes |
| ONNX | .onnx | ✅ | Graph, operators, shapes |
| Safetensors | .safetensors | ✅ | Tensors, dtypes, shapes |

---

## Dataset Version Control

### Overview

Git-like dataset versioning with split management, provenance tracking, and model-dataset linking.

### Features

- **Version Control**: Monotonic versioning (v1.0.0, v1.1.0, etc.)
- **Split Management**: Train/validation/test splits with metadata
- **Provenance**: Citation, author, creation date, source
- **Model Linking**: Track which datasets trained which models
- **Lineage Tracking**: Full audit trail for compliance

### Creating a Dataset

```python
# Upload dataset files
for split in ['train', 'val', 'test']:
    s3.upload_file(
        f'imagenet_{split}.tar',
        'ml-datasets',
        f'datasets/imagenet/v1.0.0/{split}.tar',
        ExtraArgs={
            'Metadata': {
                'split': split,
                'samples': str({'train': 1000000, 'val': 50000, 'test': 100000}[split]),
                'format': 'tar',
                'description': f'ImageNet {split} split'
            }
        }
    )
```

### Dataset Metadata

```json
{
  "dataset_id": "imagenet",
  "version": "v1.0.0",
  "description": "ImageNet ILSVRC 2012",
  "author": "ml-team",
  "created_at": "2025-01-02T09:00:00Z",
  "provenance": {
    "source": "http://image-net.org",
    "citation": "Russakovsky et al., 2015",
    "license": "Custom"
  },
  "splits": [
    {
      "name": "train",
      "uri": "s3://ml-datasets/datasets/imagenet/v1.0.0/train.tar",
      "samples": 1000000,
      "size_bytes": 134217728000
    },
    {
      "name": "val",
      "uri": "s3://ml-datasets/datasets/imagenet/v1.0.0/val.tar",
      "samples": 50000,
      "size_bytes": 6710886400
    },
    {
      "name": "test",
      "uri": "s3://ml-datasets/datasets/imagenet/v1.0.0/test.tar",
      "samples": 100000,
      "size_bytes": 13421772800
    }
  ],
  "statistics": {
    "total_samples": 1150000,
    "total_size_bytes": 154350387200,
    "image_size": [224, 224],
    "num_classes": 1000
  }
}
```

### Linking Datasets to Models

When training a model, record the dataset version used:

```python
# Upload trained model with dataset link
s3.upload_file(
    'trained_model.pt',
    'ml-models',
    'models/resnet50/v1.0.0/model.pt',
    ExtraArgs={
        'Metadata': {
            'training_dataset': 'imagenet',
            'dataset_version': 'v1.0.0',
            'dataset_split': 'train',
            'framework': 'pytorch'
        }
    }
)
```

This creates a bidirectional link for reproducibility:
- **Model → Dataset**: Which data trained this model?
- **Dataset → Models**: Which models were trained on this data?

---

## Preprocessing Pipelines

### Overview

Declarative data transformation pipelines with automatic caching and format detection.

### Features

- **Image Operations**: Resize, normalize, augment (flip, rotate, brightness, contrast)
- **Pipeline Definition**: JSON/YAML configuration files
- **Caching**: LRU cache for preprocessed results
- **Versioning**: Track pipeline versions with datasets
- **Reproducibility**: Deterministic transformations

### Creating a Pipeline

**File**: `imagenet_pipeline.json`

```json
{
  "id": "imagenet-preprocessing",
  "name": "ImageNet Standard Preprocessing",
  "version": "1.0.0",
  "description": "Standard ImageNet preprocessing with normalization",
  "steps": [
    {
      "id": "resize",
      "step_type": "image_resize",
      "config": {
        "width": 224,
        "height": 224,
        "mode": "fit",
        "filter": "lanczos3"
      },
      "cache": true
    },
    {
      "id": "normalize",
      "step_type": "image_normalization",
      "config": {
        "mean": [0.485, 0.456, 0.406],
        "std": [0.229, 0.224, 0.225],
        "normalize_range": true
      },
      "cache": true
    }
  ],
  "metadata": {
    "author": "ml-team",
    "purpose": "training",
    "target_models": ["resnet50", "efficientnet"]
  }
}
```

### Uploading and Using Pipelines

```bash
# Upload pipeline definition via REST API
curl -X POST http://rs3gw:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d @imagenet_pipeline.json

# Apply pipeline to an object
curl -X POST http://rs3gw:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "imagenet-preprocessing",
    "bucket": "raw-images",
    "key": "photo.jpg",
    "output_bucket": "processed-images",
    "output_key": "photo_preprocessed.jpg"
  }'
```

### Batch Processing with Python

```python
import requests
import concurrent.futures

def preprocess_image(key):
    response = requests.post(
        'http://rs3gw:9000/api/preprocessing/apply',
        json={
            'pipeline_id': 'imagenet-preprocessing',
            'bucket': 'raw-images',
            'key': key,
            'output_bucket': 'processed-images'
        }
    )
    return response.json()

# Get all images
images = [obj['Key'] for obj in s3.list_objects_v2(Bucket='raw-images')['Contents']]

# Process in parallel
with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
    results = list(executor.map(preprocess_image, images))

print(f"Processed {len(results)} images")
```

### Pre-built Pipeline Templates

rs3gw includes 12 production-ready pipeline templates in `examples/`:

1. **ImageNet** - ResNet, VGG, Inception preprocessing
2. **Medical Imaging** - CT, MRI, X-Ray DICOM preprocessing
3. **Object Detection** - YOLO format preprocessing
4. **Video** - Frame extraction and temporal sampling
5. **Audio** - MFCC and mel spectrogram extraction
6. **NLP/Text** - Tokenization preprocessing
7. **CLIP** - OpenAI multimodal preprocessing
8. **DINOv2** - Meta self-supervised learning
9. **ViT** - Vision Transformer preprocessing
10. **EfficientNet** - Compound scaling preprocessing
11. **MobileNet** - Mobile/edge deployment
12. **Training Augmentation** - Data augmentation pipeline

See `examples/PREPROCESSING_PIPELINES.md` for detailed documentation.

---

## Tensor Storage Optimizations

### Overview

rs3gw automatically detects and optimizes storage for common ML tensor formats.

### Supported Formats

| Format | Detection | Optimization |
|--------|-----------|--------------|
| PyTorch (.pt, .pth) | Magic bytes + pickle header | Metadata extraction, chunked upload |
| TensorFlow (SavedModel) | Protobuf structure | Variable extraction, sharded storage |
| ONNX (.onnx) | Protobuf schema | Graph optimization metadata |
| Safetensors | JSON header | Zero-copy tensor loading metadata |

### PyTorch Example

```python
import torch

# Save model
model = torch.hub.load('pytorch/vision', 'resnet50', pretrained=True)
torch.save(model.state_dict(), 'resnet50.pt')

# Upload to rs3gw
s3.upload_file('resnet50.pt', 'ml-models', 'models/resnet50/v1.0.0/model.pt')

# rs3gw automatically extracts:
# - Tensor shapes
# - Parameter count
# - Layer names
# - Data types
```

### TensorFlow Example

```python
import tensorflow as tf

# Save model
model = tf.keras.applications.ResNet50(weights='imagenet')
model.save('resnet50_savedmodel')

# Upload to rs3gw (directory becomes tar archive)
import shutil
shutil.make_archive('resnet50', 'tar', 'resnet50_savedmodel')
s3.upload_file('resnet50.tar', 'ml-models', 'models/resnet50-tf/v1.0.0/model.tar')
```

### Safetensors Example

```python
from safetensors.torch import save_file
import torch

# Save with safetensors
tensors = {
    'weight': torch.randn(100, 100),
    'bias': torch.randn(100)
}
save_file(tensors, 'model.safetensors')

# Upload to rs3gw
s3.upload_file('model.safetensors', 'ml-models', 'models/custom/v1.0.0/model.safetensors')

# rs3gw parses JSON header for instant metadata access
```

---

## Production Workflows

### Complete ML Pipeline Example

```python
#!/usr/bin/env python3
"""Complete ML training workflow with rs3gw"""

import boto3
import torch
import torchvision
from datetime import datetime

# Initialize rs3gw client
s3 = boto3.client('s3', endpoint_url='http://rs3gw:9000')

# 1. Register dataset version
dataset_version = 'v1.0.0'
s3.put_object(
    Bucket='ml-datasets',
    Key=f'datasets/imagenet/{dataset_version}/metadata.json',
    Body=json.dumps({
        'version': dataset_version,
        'created_at': datetime.utcnow().isoformat(),
        'author': 'ml-engineer',
        'samples': 1000000
    })
)

# 2. Download and preprocess data
# (using preprocessing pipeline API)
requests.post('http://rs3gw:9000/api/preprocessing/apply', json={
    'pipeline_id': 'imagenet-preprocessing',
    'bucket': 'ml-datasets',
    'key': f'datasets/imagenet/{dataset_version}/train.tar'
})

# 3. Train model
model = torchvision.models.resnet50()
# ... training code ...

# 4. Save model with metadata
model_version = 'v1.0.0'
torch.save(model.state_dict(), 'resnet50.pt')

# 5. Upload to model registry
s3.upload_file(
    'resnet50.pt',
    'ml-models',
    f'models/resnet50/{model_version}/model.pt',
    ExtraArgs={
        'Metadata': {
            'framework': 'pytorch',
            'architecture': 'resnet50',
            'dataset': 'imagenet',
            'dataset_version': dataset_version,
            'training_date': datetime.utcnow().isoformat(),
            'accuracy': '0.92',
            'hyperparameters': json.dumps({
                'lr': 0.001,
                'batch_size': 32,
                'epochs': 100
            })
        }
    }
)

# 6. Tag for deployment stage
s3.put_object_tagging(
    Bucket='ml-models',
    Key=f'models/resnet50/{model_version}/model.pt',
    Tagging={'TagSet': [{'Key': 'stage', 'Value': 'staging'}]}
)

print(f"✅ Model {model_version} uploaded and tagged for staging")
```

### A/B Testing Workflow

```python
# Deploy multiple model versions
models = {
    'v1.0.0': 'production',
    'v1.1.0': 'staging',
    'v2.0.0': 'development'
}

for version, stage in models.items():
    s3.put_object_tagging(
        Bucket='ml-models',
        Key=f'models/resnet50/{version}/model.pt',
        Tagging={'TagSet': [{'Key': 'stage', 'Value': stage}]}
    )

# Inference service can route based on stage tag
def get_model_for_stage(stage):
    objects = s3.list_objects_v2(Bucket='ml-models', Prefix='models/resnet50/')
    for obj in objects['Contents']:
        tags = s3.get_object_tagging(Bucket='ml-models', Key=obj['Key'])
        for tag in tags['TagSet']:
            if tag['Key'] == 'stage' and tag['Value'] == stage:
                return obj['Key']
    return None

# Load production model
production_model_key = get_model_for_stage('production')
model_obj = s3.get_object(Bucket='ml-models', Key=production_model_key)
model = torch.load(io.BytesIO(model_obj['Body'].read()))
```

---

## Integration Examples

### HuggingFace Integration

```python
from transformers import AutoModel, AutoTokenizer

# Upload HuggingFace model
model = AutoModel.from_pretrained('bert-base-uncased')
tokenizer = AutoTokenizer.from_pretrained('bert-base-uncased')

# Save to temporary directory
model.save_pretrained('./bert-model')
tokenizer.save_pretrained('./bert-model')

# Create tar archive
shutil.make_archive('bert-model', 'tar', './bert-model')

# Upload to rs3gw
s3.upload_file(
    'bert-model.tar',
    'ml-models',
    'models/bert/v1.0.0/model.tar',
    ExtraArgs={
        'Metadata': {
            'framework': 'transformers',
            'architecture': 'bert-base-uncased',
            'task': 'masked-language-modeling'
        }
    }
)
```

### MLflow Integration

```python
import mlflow
import mlflow.pytorch

# Configure MLflow to use rs3gw as artifact store
mlflow.set_tracking_uri('http://mlflow-server:5000')

# Log model with MLflow (artifacts go to rs3gw)
with mlflow.start_run():
    mlflow.log_param('learning_rate', 0.001)
    mlflow.log_param('batch_size', 32)
    mlflow.log_metric('accuracy', 0.92)

    # This saves to rs3gw via S3 API
    mlflow.pytorch.log_model(model, 'model')
```

### DVC Integration

```yaml
# .dvc/config
['remote "rs3gw"']
    url = s3://ml-datasets
    endpointurl = http://rs3gw:9000
```

```bash
# Track dataset with DVC
dvc add datasets/imagenet.tar
dvc push -r rs3gw

# Dataset now versioned in Git + stored in rs3gw
```

---

## Best Practices

### 1. Versioning Strategy

✅ **DO**: Use semantic versioning (v1.0.0, v1.1.0, v2.0.0)
✅ **DO**: Include dataset version in model metadata
✅ **DO**: Tag models by deployment stage
❌ **DON'T**: Overwrite existing versions
❌ **DON'T**: Use timestamps as version numbers

### 2. Metadata Completeness

Always include:
- Framework and architecture
- Dataset version used for training
- Key hyperparameters
- Training date and author
- Performance metrics (accuracy, loss, etc.)
- Intended use case or task

### 3. Storage Organization

```
ml-models/
├── models/
│   ├── resnet50/
│   │   ├── v1.0.0/
│   │   │   ├── model.pt
│   │   │   └── config.json
│   │   └── v2.0.0/
│   │       └── model.pt
│   └── bert/
│       └── v1.0.0/
│           └── model.tar
└── checkpoints/
    └── resnet50/
        └── epoch_50.pt

ml-datasets/
├── datasets/
│   ├── imagenet/
│   │   ├── v1.0.0/
│   │   │   ├── train.tar
│   │   │   ├── val.tar
│   │   │   ├── test.tar
│   │   │   └── metadata.json
│   │   └── v1.1.0/
│   │       └── ...
│   └── coco/
│       └── v1.0.0/
└── preprocessed/
    └── imagenet-224x224/
```

### 4. Caching Strategy

- Enable preprocessing cache for frequently used transformations
- Set cache size based on available memory (default: 4GB)
- Clear cache between major pipeline changes
- Monitor cache hit rate via observability API

### 5. Security

- Use separate buckets for models and datasets
- Enable bucket policies for access control
- Enable audit logging for compliance
- Encrypt sensitive models with SSE-S3 or SSE-C
- Use presigned URLs for temporary access

### 6. Performance

- Use multipart upload for models >100MB
- Enable compression for text-based formats (JSON, YAML)
- Use Arrow Flight for large dataset transfers
- Leverage preprocessing cache to avoid redundant computations
- Use gRPC API for high-throughput model serving

---

## Monitoring and Observability

### Metrics to Track

```bash
# Model registry metrics
curl http://rs3gw:9000/api/observability/business-metrics | jq '.model_registry'

# Dataset version counts
curl http://rs3gw:9000/api/observability/business-metrics | jq '.dataset_versions'

# Preprocessing cache performance
curl http://rs3gw:9000/api/preprocessing/cache/stats
```

### Example Response

```json
{
  "model_registry": {
    "total_models": 42,
    "total_versions": 156,
    "storage_size_bytes": 85899345920,
    "formats": {
      "pytorch": 89,
      "tensorflow": 45,
      "onnx": 22
    }
  },
  "dataset_versions": {
    "total_datasets": 15,
    "total_versions": 48,
    "total_size_bytes": 5497558138880
  },
  "preprocessing_cache": {
    "size_mb": 2048,
    "objects": 15234,
    "hit_rate": 0.847,
    "evictions": 1543
  }
}
```

---

## Troubleshooting

### Issue: Model metadata not extracted

**Solution**: Ensure model file extension matches format (.pt for PyTorch, .onnx for ONNX, etc.)

### Issue: Preprocessing pipeline fails

**Solution**: Validate pipeline JSON with `/api/preprocessing/validate` endpoint

### Issue: Slow model uploads

**Solution**: Use multipart upload for files >100MB

```python
from boto3.s3.transfer import TransferConfig

config = TransferConfig(
    multipart_threshold=100 * 1024 * 1024,  # 100MB
    multipart_chunksize=10 * 1024 * 1024    # 10MB chunks
)

s3.upload_file('large_model.pt', 'ml-models', 'models/large/v1.0.0/model.pt', Config=config)
```

### Issue: Cache not being used

**Solution**: Check cache configuration and ensure `cache: true` in pipeline steps

---

## Conclusion

rs3gw provides enterprise-grade ML/AI infrastructure for:

✅ **Model Management**: Versioned model registry with MLflow compatibility
✅ **Dataset Version Control**: Git-like dataset versioning with provenance
✅ **Preprocessing**: Declarative pipelines with automatic caching
✅ **Optimization**: Format-aware storage for ML tensors
✅ **Integration**: Works with PyTorch, TensorFlow, HuggingFace, MLflow, DVC

For more information, see:

- [Production Deployment Guide](production_deployment.md)
- [Preprocessing Pipeline Examples](../examples/PREPROCESSING_PIPELINES.md)
- [API Documentation](../src/api/README.md)
- [Storage Documentation](../src/storage/README.md)

---

**rs3gw ML/AI Features** - Production-ready ML infrastructure built on S3-compatible object storage.

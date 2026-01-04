# rs3gw Integration Examples

This directory contains integration examples demonstrating how to use rs3gw as a storage backend for various frameworks and use cases.

## Machine Learning Framework Integration

### PyTorch Integration

**File**: `pytorch_integration.py`

Demonstrates how to use rs3gw for PyTorch training workflows:
- Loading training data from S3 using custom Dataset
- Saving and loading model checkpoints
- Storing training metrics and metadata
- Managing model versioning

**Usage**:
```bash
# Install dependencies
pip install torch boto3

# Set rs3gw endpoint (optional, defaults to localhost:9000)
export RS3GW_ENDPOINT=http://localhost:9000
export RS3GW_ACCESS_KEY=your_access_key
export RS3GW_SECRET_KEY=your_secret_key

# Run the example
python pytorch_integration.py
```

**Features**:
- Custom `S3Dataset` class for loading data from rs3gw
- Checkpoint saving with metadata (epoch, loss)
- Efficient serialization using PyTorch's native format
- Automatic checkpoint management

### TensorFlow/Keras Integration

**File**: `tensorflow_integration.py`

Demonstrates how to use rs3gw for TensorFlow/Keras training workflows:
- Custom Keras callbacks for S3 checkpoint saving
- SavedModel format storage
- TensorBoard logs on S3
- Training history persistence

**Usage**:
```bash
# Install dependencies
pip install tensorflow boto3

# Set rs3gw endpoint (optional, defaults to localhost:9000)
export RS3GW_ENDPOINT=http://localhost:9000
export RS3GW_ACCESS_KEY=your_access_key
export RS3GW_SECRET_KEY=your_secret_key

# Run the example
python tensorflow_integration.py
```

**Features**:
- `S3CheckpointCallback` for automatic checkpoint saving
- `S3TensorBoardCallback` for metrics logging
- SavedModel format preservation
- Training history in JSON format

## Advanced Use Cases

### Distributed Training

For distributed training scenarios, rs3gw provides:
- **High throughput**: Parallel writes from multiple workers
- **Consistency**: Strong consistency for checkpoint coordination
- **Deduplication**: Automatic deduplication of model weights
- **Versioning**: Track all checkpoint versions

Example distributed PyTorch setup:
```python
import torch.distributed as dist

# Initialize process group
dist.init_process_group(backend='nccl')
rank = dist.get_rank()

# Only rank 0 saves checkpoints to avoid conflicts
if rank == 0:
    save_checkpoint_to_s3(model, optimizer, epoch, loss, s3_client, bucket, key)

# All ranks can read checkpoints
load_checkpoint_from_s3(model, optimizer, s3_client, bucket, key)
```

### Model Registry

Use rs3gw as a centralized model registry:

```python
# Organize models by version
s3://ml-models/
├── mnist-classifier/
│   ├── v1.0/model.pt
│   ├── v1.1/model.pt
│   └── latest/model.pt
└── imagenet-resnet50/
    ├── v2.0/model.pt
    └── latest/model.pt
```

### Dataset Versioning

Version your datasets for reproducibility:

```python
# Tag datasets with version metadata
s3_client.put_object(
    Bucket='datasets',
    Key='imagenet/train/batch_001.tar',
    Body=data,
    Metadata={
        'dataset-version': 'v2.1',
        'created-at': '2025-12-31',
        'split': 'train'
    }
)
```

## Performance Tips

### 1. Use Multipart Upload for Large Files

```python
# For files > 100MB, use multipart upload
import boto3.s3.transfer

config = boto3.s3.transfer.TransferConfig(
    multipart_threshold=100 * 1024 * 1024,  # 100MB
    max_concurrency=10,
    multipart_chunksize=10 * 1024 * 1024,   # 10MB chunks
)

s3_client.upload_file(
    'large_model.pt',
    bucket,
    'models/large_model.pt',
    Config=config
)
```

### 2. Enable Compression

rs3gw automatically compresses data. For best results:
- Use PyTorch's `.pt` format (already efficient)
- For TensorFlow, SavedModel format is already compressed
- JSON metrics benefit from automatic compression

### 3. Leverage Caching

rs3gw's ML-based cache learns access patterns:
- Frequently accessed checkpoints are automatically cached
- Recent models get priority
- Training data benefits from prefetching

### 4. Use S3 Select for Large Datasets

Query training metrics without downloading entire files:

```python
# Query training metrics using S3 Select
response = s3_client.select_object_content(
    Bucket='ml-training',
    Key='training_history.json',
    ExpressionType='SQL',
    Expression='SELECT * FROM s3object[*] WHERE s.loss < 0.1',
    InputSerialization={'JSON': {'Type': 'DOCUMENT'}},
    OutputSerialization={'JSON': {}}
)
```

## Monitoring and Observability

### Track Storage Usage

```python
# List all checkpoints and their sizes
response = s3_client.list_objects_v2(Bucket='ml-training', Prefix='checkpoints/')
total_size = sum(obj['Size'] for obj in response.get('Contents', []))
print(f"Total checkpoint storage: {total_size / (1024**3):.2f} GB")
```

### Monitor Training Progress

```python
# Get latest checkpoint metadata
response = s3_client.head_object(Bucket='ml-training', Key='checkpoints/latest.pt')
metadata = response['Metadata']
print(f"Latest checkpoint: epoch {metadata['epoch']}, loss {metadata['loss']}")
```

### Use rs3gw Metrics

Access rs3gw Prometheus metrics at `http://localhost:9000/metrics`:
- `rs3gw_objects_total{bucket="ml-training"}` - Number of stored objects
- `rs3gw_storage_bytes{bucket="ml-training"}` - Total storage used
- `rs3gw_operation_duration_seconds` - Operation latencies

## Best Practices

### 1. Organize Your Buckets

```
ml-datasets/        # Raw training data
ml-checkpoints/     # Training checkpoints
ml-models/          # Production models
ml-experiments/     # Experimental runs
```

### 2. Use Meaningful Metadata

```python
s3_client.put_object(
    Bucket=bucket,
    Key=key,
    Body=data,
    Metadata={
        'model-architecture': 'resnet50',
        'dataset': 'imagenet',
        'training-steps': '100000',
        'validation-accuracy': '0.945',
        'git-commit': 'abc123',
        'framework': 'pytorch',
        'framework-version': '2.0.0'
    }
)
```

### 3. Implement Checkpoint Cleanup

```python
def cleanup_old_checkpoints(s3_client, bucket, prefix, keep_last_n=5):
    """Keep only the N most recent checkpoints"""
    response = s3_client.list_objects_v2(Bucket=bucket, Prefix=prefix)
    objects = sorted(
        response.get('Contents', []),
        key=lambda x: x['LastModified'],
        reverse=True
    )

    # Delete old checkpoints
    for obj in objects[keep_last_n:]:
        s3_client.delete_object(Bucket=bucket, Key=obj['Key'])
        print(f"Deleted old checkpoint: {obj['Key']}")
```

### 4. Use Object Tagging for Cost Management

```python
# Tag expensive models
s3_client.put_object_tagging(
    Bucket=bucket,
    Key='models/large_model.pt',
    Tagging={
        'TagSet': [
            {'Key': 'cost-center', 'Value': 'ml-research'},
            {'Key': 'lifecycle', 'Value': 'archive-after-90-days'},
            {'Key': 'importance', 'Value': 'high'}
        ]
    }
)
```

## Troubleshooting

### Connection Issues

```python
# Test rs3gw connectivity
try:
    response = s3_client.list_buckets()
    print(f"Connected successfully. Buckets: {len(response['Buckets'])}")
except Exception as e:
    print(f"Connection failed: {e}")
    print("Check RS3GW_ENDPOINT, access keys, and network connectivity")
```

### Large File Upload Failures

```python
# Enable debug logging
import logging
boto3.set_stream_logger('boto3.resources', logging.DEBUG)

# Increase timeout for large uploads
s3_client = boto3.client(
    's3',
    endpoint_url=endpoint,
    config=Config(
        signature_version='s3v4',
        connect_timeout=300,
        read_timeout=300
    )
)
```

### Memory Issues with Large Models

```python
# Stream large files instead of loading into memory
with open('large_model.pt', 'rb') as f:
    s3_client.upload_fileobj(f, bucket, key)

# For downloads
with open('large_model.pt', 'wb') as f:
    s3_client.download_fileobj(bucket, key, f)
```

## Additional Resources

- [rs3gw Documentation](../README.md)
- [AWS SDK for Python (Boto3)](https://boto3.amazonaws.com/v1/documentation/api/latest/index.html)
- [PyTorch Documentation](https://pytorch.org/docs/)
- [TensorFlow Documentation](https://www.tensorflow.org/api_docs)

## Contributing

Feel free to contribute additional examples! Areas of interest:
- JAX/Flax integration
- scikit-learn model persistence
- MLflow integration
- Kubeflow pipelines
- Ray Train/Tune integration

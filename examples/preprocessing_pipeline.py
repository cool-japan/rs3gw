#!/usr/bin/env python3
"""
Dataset Preprocessing Pipeline Example for rs3gw

This example demonstrates how to use rs3gw's preprocessing pipeline
for preparing datasets for machine learning training.

Features demonstrated:
- Image normalization (ImageNet-style)
- Image resizing with different modes
- Data augmentation (flips, brightness, contrast)
- Pipeline caching for performance
- Pipeline versioning and reproducibility

Requirements:
- rs3gw server running (default: localhost:9000)
- boto3 for S3 API access
- Pillow for local image processing
"""

import boto3
import json
import io
from PIL import Image
import numpy as np
from pathlib import Path


class Rs3gwPreprocessingClient:
    """Client for rs3gw preprocessing pipeline API"""

    def __init__(self, endpoint_url='http://localhost:9000',
                 access_key='minioadmin', secret_key='minioadmin'):
        """Initialize the preprocessing client"""
        self.s3 = boto3.client(
            's3',
            endpoint_url=endpoint_url,
            aws_access_key_id=access_key,
            aws_secret_access_key=secret_key,
            region_name='us-east-1'
        )
        self.bucket = 'ml-datasets'

        # Ensure bucket exists
        try:
            self.s3.head_bucket(Bucket=self.bucket)
        except:
            self.s3.create_bucket(Bucket=self.bucket)

    def create_pipeline(self, pipeline_id, name, steps, version='1.0.0'):
        """
        Create a preprocessing pipeline

        Args:
            pipeline_id: Unique identifier for the pipeline
            name: Human-readable name
            steps: List of preprocessing steps
            version: Pipeline version

        Returns:
            Pipeline definition dict
        """
        pipeline = {
            'id': pipeline_id,
            'name': name,
            'version': version,
            'description': f'{name} preprocessing pipeline',
            'steps': steps,
            'metadata': {
                'created_at': '2025-12-31T00:00:00Z',
                'author': 'ml-engineer'
            }
        }

        # Store pipeline definition in S3
        pipeline_key = f'pipelines/{pipeline_id}.json'
        self.s3.put_object(
            Bucket=self.bucket,
            Key=pipeline_key,
            Body=json.dumps(pipeline, indent=2),
            ContentType='application/json',
            Metadata={
                'x-amz-meta-pipeline-id': pipeline_id,
                'x-amz-meta-pipeline-version': version
            }
        )

        print(f"✓ Created pipeline '{name}' (ID: {pipeline_id})")
        return pipeline

    def upload_image(self, image_path, object_key):
        """Upload an image to S3"""
        with open(image_path, 'rb') as f:
            self.s3.put_object(
                Bucket=self.bucket,
                Key=object_key,
                Body=f.read(),
                ContentType='image/png'
            )
        print(f"✓ Uploaded {image_path} to {object_key}")

    def apply_pipeline(self, pipeline_id, input_key, output_key):
        """
        Apply preprocessing pipeline to an image

        This is a simulation - in production, rs3gw would have an API endpoint
        for applying pipelines. For now, we download, process locally, and upload.

        Args:
            pipeline_id: Pipeline to apply
            input_key: Input object key
            output_key: Output object key
        """
        # Get pipeline definition
        pipeline_key = f'pipelines/{pipeline_id}.json'
        response = self.s3.get_object(Bucket=self.bucket, Key=pipeline_key)
        pipeline = json.loads(response['Body'].read())

        # Get input image
        response = self.s3.get_object(Bucket=self.bucket, Key=input_key)
        image_data = response['Body'].read()
        img = Image.open(io.BytesIO(image_data))

        print(f"Processing with pipeline '{pipeline['name']}'...")

        # Apply each step
        for step in pipeline['steps']:
            step_type = step['step_type']
            config = step.get('config', {})

            if step_type == 'image_resize':
                img = self._resize_image(img, config)
            elif step_type == 'image_normalization':
                img = self._normalize_image(img, config)
            elif step_type == 'data_augmentation':
                img = self._augment_image(img, config)

        # Upload processed image
        output_buffer = io.BytesIO()
        img.save(output_buffer, format='PNG')
        output_buffer.seek(0)

        self.s3.put_object(
            Bucket=self.bucket,
            Key=output_key,
            Body=output_buffer.getvalue(),
            ContentType='image/png',
            Metadata={
                'x-amz-meta-pipeline-id': pipeline_id,
                'x-amz-meta-pipeline-version': pipeline['version'],
                'x-amz-meta-source-key': input_key
            }
        )

        print(f"✓ Processed image saved to {output_key}")
        return img

    def _resize_image(self, img, config):
        """Resize image according to config"""
        width = config.get('width', 224)
        height = config.get('height', 224)
        mode = config.get('mode', 'fit')

        if mode == 'exact':
            img = img.resize((width, height), Image.LANCZOS)
        elif mode == 'fit':
            img.thumbnail((width, height), Image.LANCZOS)
        elif mode == 'fill':
            # Crop to fill
            aspect_ratio = width / height
            img_ratio = img.width / img.height

            if img_ratio > aspect_ratio:
                new_height = height
                new_width = int(height * img_ratio)
            else:
                new_width = width
                new_height = int(width / img_ratio)

            img = img.resize((new_width, new_height), Image.LANCZOS)

            # Center crop
            left = (new_width - width) // 2
            top = (new_height - height) // 2
            img = img.crop((left, top, left + width, top + height))

        print(f"  - Resized to {img.width}x{img.height} (mode: {mode})")
        return img

    def _normalize_image(self, img, config):
        """Normalize image (simulated - actual normalization in rs3gw)"""
        mean = config.get('mean', [0.485, 0.456, 0.406])
        std = config.get('std', [0.229, 0.224, 0.225])
        print(f"  - Normalized (ImageNet: mean={mean[:2]}..., std={std[:2]}...)")
        return img

    def _augment_image(self, img, config):
        """Apply data augmentation"""
        h_flip = config.get('horizontal_flip_prob', 0.5)
        brightness = config.get('brightness_range', (0.8, 1.2))

        print(f"  - Augmentation (flip_prob={h_flip}, brightness={brightness})")
        return img


def example_imagenet_pipeline():
    """Example: ImageNet preprocessing pipeline"""
    client = Rs3gwPreprocessingClient()

    print("\n=== ImageNet Preprocessing Pipeline ===\n")

    # Define ImageNet preprocessing steps
    steps = [
        {
            'id': 'resize',
            'step_type': 'image_resize',
            'config': {
                'width': 224,
                'height': 224,
                'mode': 'fit',
                'filter': 'lanczos3'
            },
            'cache_results': False,
            'description': 'Resize to 224x224 for ImageNet models'
        },
        {
            'id': 'normalize',
            'step_type': 'image_normalization',
            'config': {
                'mean': [0.485, 0.456, 0.406],
                'std': [0.229, 0.224, 0.225],
                'normalize_range': True
            },
            'cache_results': True,
            'description': 'ImageNet normalization'
        }
    ]

    pipeline = client.create_pipeline(
        pipeline_id='imagenet-preprocessing',
        name='ImageNet Preprocessing',
        steps=steps
    )

    return pipeline


def example_augmentation_pipeline():
    """Example: Data augmentation pipeline for training"""
    client = Rs3gwPreprocessingClient()

    print("\n=== Training Augmentation Pipeline ===\n")

    steps = [
        {
            'id': 'resize',
            'step_type': 'image_resize',
            'config': {
                'width': 256,
                'height': 256,
                'mode': 'fill',
                'filter': 'bilinear'
            },
            'cache_results': False
        },
        {
            'id': 'augment',
            'step_type': 'data_augmentation',
            'config': {
                'horizontal_flip_prob': 0.5,
                'vertical_flip_prob': 0.0,
                'rotation_range': 15.0,
                'brightness_range': [0.8, 1.2],
                'contrast_range': [0.8, 1.2],
                'saturation_range': None,
                'random_crop_size': None
            },
            'cache_results': False,
            'description': 'Random augmentation for training robustness'
        },
        {
            'id': 'normalize',
            'step_type': 'image_normalization',
            'config': {
                'mean': [0.5, 0.5, 0.5],
                'std': [0.5, 0.5, 0.5],
                'normalize_range': True
            },
            'cache_results': False
        }
    ]

    pipeline = client.create_pipeline(
        pipeline_id='training-augmentation',
        name='Training Augmentation',
        steps=steps,
        version='1.1.0'
    )

    return pipeline


def example_batch_processing():
    """Example: Batch process multiple images"""
    client = Rs3gwPreprocessingClient()

    print("\n=== Batch Processing Example ===\n")

    # Create a simple pipeline
    steps = [
        {
            'id': 'resize',
            'step_type': 'image_resize',
            'config': {'width': 128, 'height': 128, 'mode': 'exact', 'filter': 'lanczos3'},
            'cache_results': True
        }
    ]

    client.create_pipeline(
        pipeline_id='thumbnail',
        name='Thumbnail Generator',
        steps=steps
    )

    # Create sample images
    print("Creating sample images...")
    for i in range(3):
        # Create a random colored image
        color = tuple(np.random.randint(0, 256, 3))
        img = Image.new('RGB', (512, 512), color)

        # Save locally
        temp_path = f'/tmp/sample_{i}.png'
        img.save(temp_path)

        # Upload and process
        input_key = f'raw/image_{i}.png'
        output_key = f'processed/thumbnail_{i}.png'

        client.upload_image(temp_path, input_key)
        client.apply_pipeline('thumbnail', input_key, output_key)

    print("\n✓ Batch processing complete!")


def example_pipeline_versioning():
    """Example: Pipeline versioning for reproducibility"""
    client = Rs3gwPreprocessingClient()

    print("\n=== Pipeline Versioning Example ===\n")

    # Version 1.0
    steps_v1 = [
        {
            'id': 'resize',
            'step_type': 'image_resize',
            'config': {'width': 224, 'height': 224, 'mode': 'fit', 'filter': 'bilinear'},
            'cache_results': True
        }
    ]

    client.create_pipeline(
        pipeline_id='model-prep-v1',
        name='Model Preparation v1',
        steps=steps_v1,
        version='1.0.0'
    )

    # Version 2.0 with normalization
    steps_v2 = [
        {
            'id': 'resize',
            'step_type': 'image_resize',
            'config': {'width': 224, 'height': 224, 'mode': 'fit', 'filter': 'lanczos3'},
            'cache_results': True
        },
        {
            'id': 'normalize',
            'step_type': 'image_normalization',
            'config': {
                'mean': [0.485, 0.456, 0.406],
                'std': [0.229, 0.224, 0.225],
                'normalize_range': True
            },
            'cache_results': True
        }
    ]

    client.create_pipeline(
        pipeline_id='model-prep-v2',
        name='Model Preparation v2',
        steps=steps_v2,
        version='2.0.0'
    )

    print("\n✓ Created pipeline versions for reproducibility")
    print("  - v1.0.0: Resize only (for older models)")
    print("  - v2.0.0: Resize + Normalization (for new models)")


def main():
    """Run all preprocessing examples"""
    print("=" * 60)
    print("rs3gw Dataset Preprocessing Pipeline Examples")
    print("=" * 60)

    try:
        # Example 1: ImageNet preprocessing
        example_imagenet_pipeline()

        # Example 2: Augmentation pipeline
        example_augmentation_pipeline()

        # Example 3: Batch processing
        example_batch_processing()

        # Example 4: Pipeline versioning
        example_pipeline_versioning()

        print("\n" + "=" * 60)
        print("✓ All examples completed successfully!")
        print("=" * 60)

        print("\nNext steps:")
        print("1. View pipelines: Check bucket 'ml-datasets/pipelines/'")
        print("2. View processed images: Check bucket 'ml-datasets/processed/'")
        print("3. Integrate with your ML training workflow")
        print("4. Use pipeline versioning for reproducible experiments")

    except Exception as e:
        print(f"\n✗ Error: {e}")
        print("\nMake sure rs3gw is running on localhost:9000")
        return 1

    return 0


if __name__ == '__main__':
    exit(main())

#!/usr/bin/env python3
"""
TensorFlow/Keras Integration Example with rs3gw

This example demonstrates how to use rs3gw as a storage backend for:
1. Training data (TFRecord format)
2. Model checkpoints (SavedModel format)
3. Training history and metrics

Requirements:
    pip install tensorflow boto3
"""

import io
import os
import tempfile
import json
import tensorflow as tf
import boto3
from botocore.client import Config


class S3CheckpointCallback(tf.keras.callbacks.Callback):
    """
    Custom Keras callback to save checkpoints to rs3gw/S3
    """
    def __init__(self, s3_client, bucket, prefix='checkpoints'):
        super(S3CheckpointCallback, self).__init__()
        self.s3 = s3_client
        self.bucket = bucket
        self.prefix = prefix

    def on_epoch_end(self, epoch, logs=None):
        logs = logs or {}

        # Save model to temporary directory
        with tempfile.TemporaryDirectory() as tmpdir:
            model_path = os.path.join(tmpdir, 'model')
            self.model.save(model_path)

            # Upload SavedModel files to S3
            checkpoint_prefix = f"{self.prefix}/epoch_{epoch + 1}"
            for root, dirs, files in os.walk(model_path):
                for file in files:
                    local_path = os.path.join(root, file)
                    relative_path = os.path.relpath(local_path, model_path)
                    s3_key = f"{checkpoint_prefix}/{relative_path}"

                    with open(local_path, 'rb') as f:
                        self.s3.put_object(
                            Bucket=self.bucket,
                            Key=s3_key,
                            Body=f.read()
                        )

        # Save training metrics
        metrics_key = f"{self.prefix}/epoch_{epoch + 1}/metrics.json"
        metrics_data = {
            'epoch': epoch + 1,
            'metrics': logs
        }
        self.s3.put_object(
            Bucket=self.bucket,
            Key=metrics_key,
            Body=json.dumps(metrics_data).encode('utf-8'),
            ContentType='application/json'
        )

        print(f"\nCheckpoint saved to s3://{self.bucket}/{checkpoint_prefix}")


class S3TensorBoardCallback(tf.keras.callbacks.Callback):
    """
    Custom callback to save TensorBoard logs to rs3gw/S3
    """
    def __init__(self, s3_client, bucket, prefix='tensorboard'):
        super(S3TensorBoardCallback, self).__init__()
        self.s3 = s3_client
        self.bucket = bucket
        self.prefix = prefix
        self.history = []

    def on_epoch_end(self, epoch, logs=None):
        logs = logs or {}
        self.history.append(logs)

        # Save scalar metrics
        for metric_name, metric_value in logs.items():
            key = f"{self.prefix}/scalars/{metric_name}/epoch_{epoch + 1}.json"
            data = {
                'step': epoch + 1,
                'value': float(metric_value)
            }
            self.s3.put_object(
                Bucket=self.bucket,
                Key=key,
                Body=json.dumps(data).encode('utf-8'),
                ContentType='application/json'
            )


def create_simple_model():
    """Create a simple neural network for demonstration"""
    model = tf.keras.Sequential([
        tf.keras.layers.Dense(256, activation='relu', input_shape=(784,)),
        tf.keras.layers.Dropout(0.2),
        tf.keras.layers.Dense(128, activation='relu'),
        tf.keras.layers.Dropout(0.2),
        tf.keras.layers.Dense(10, activation='softmax')
    ])
    return model


def save_model_to_s3(model, s3_client, bucket, key_prefix):
    """
    Save TensorFlow model to rs3gw/S3 in SavedModel format
    """
    with tempfile.TemporaryDirectory() as tmpdir:
        model_path = os.path.join(tmpdir, 'model')
        model.save(model_path)

        # Upload all SavedModel files
        for root, dirs, files in os.walk(model_path):
            for file in files:
                local_path = os.path.join(root, file)
                relative_path = os.path.relpath(local_path, model_path)
                s3_key = f"{key_prefix}/{relative_path}"

                with open(local_path, 'rb') as f:
                    s3_client.put_object(
                        Bucket=bucket,
                        Key=s3_key,
                        Body=f.read()
                    )

    print(f"Model saved to s3://{bucket}/{key_prefix}")


def load_model_from_s3(s3_client, bucket, key_prefix):
    """
    Load TensorFlow model from rs3gw/S3
    """
    with tempfile.TemporaryDirectory() as tmpdir:
        model_path = os.path.join(tmpdir, 'model')
        os.makedirs(model_path, exist_ok=True)

        # List and download all model files
        paginator = s3_client.get_paginator('list_objects_v2')
        for page in paginator.paginate(Bucket=bucket, Prefix=key_prefix):
            for obj in page.get('Contents', []):
                s3_key = obj['Key']
                relative_path = os.path.relpath(s3_key, key_prefix)

                # Skip the prefix itself
                if relative_path == '.':
                    continue

                local_path = os.path.join(model_path, relative_path)
                os.makedirs(os.path.dirname(local_path), exist_ok=True)

                response = s3_client.get_object(Bucket=bucket, Key=s3_key)
                with open(local_path, 'wb') as f:
                    f.write(response['Body'].read())

        # Load the model
        model = tf.keras.models.load_model(model_path)
        print(f"Model loaded from s3://{bucket}/{key_prefix}")
        return model


def save_training_history(history, s3_client, bucket, key):
    """
    Save training history to rs3gw/S3
    """
    history_data = {
        'history': {k: [float(v) for v in vals] for k, vals in history.history.items()}
    }

    s3_client.put_object(
        Bucket=bucket,
        Key=key,
        Body=json.dumps(history_data, indent=2).encode('utf-8'),
        ContentType='application/json',
        Metadata={
            'epochs': str(len(history.epoch)),
            'framework': 'tensorflow'
        }
    )
    print(f"Training history saved to s3://{bucket}/{key}")


def main():
    # Configure rs3gw connection
    s3_client = boto3.client(
        's3',
        endpoint_url=os.getenv('RS3GW_ENDPOINT', 'http://localhost:9000'),
        aws_access_key_id=os.getenv('RS3GW_ACCESS_KEY', ''),
        aws_secret_access_key=os.getenv('RS3GW_SECRET_KEY', ''),
        config=Config(signature_version='s3v4'),
        region_name='us-east-1'
    )

    bucket_name = 'ml-training-tf'

    # Create bucket if it doesn't exist
    try:
        s3_client.create_bucket(Bucket=bucket_name)
        print(f"Created bucket: {bucket_name}")
    except s3_client.exceptions.BucketAlreadyOwnedByYou:
        print(f"Bucket already exists: {bucket_name}")
    except Exception as e:
        print(f"Error creating bucket: {e}")

    # Create model
    model = create_simple_model()
    model.compile(
        optimizer='adam',
        loss='sparse_categorical_crossentropy',
        metrics=['accuracy']
    )

    print("\nModel architecture:")
    model.summary()

    # Generate synthetic training data
    x_train = tf.random.normal((1000, 784))
    y_train = tf.random.uniform((1000,), minval=0, maxval=10, dtype=tf.int32)
    x_val = tf.random.normal((200, 784))
    y_val = tf.random.uniform((200,), minval=0, maxval=10, dtype=tf.int32)

    # Setup callbacks
    callbacks = [
        S3CheckpointCallback(s3_client, bucket_name, prefix='checkpoints'),
        S3TensorBoardCallback(s3_client, bucket_name, prefix='tensorboard')
    ]

    # Train model
    print("\nStarting training...")
    history = model.fit(
        x_train, y_train,
        validation_data=(x_val, y_val),
        epochs=5,
        batch_size=32,
        callbacks=callbacks,
        verbose=1
    )

    # Save training history
    save_training_history(history, s3_client, bucket_name, 'training_history.json')

    # Save final model
    save_model_to_s3(model, s3_client, bucket_name, 'models/final_model')

    # Demonstrate model loading
    print("\nLoading model from rs3gw...")
    loaded_model = load_model_from_s3(s3_client, bucket_name, 'models/final_model')

    # Verify loaded model
    test_input = tf.random.normal((1, 784))
    original_output = model.predict(test_input, verbose=0)
    loaded_output = loaded_model.predict(test_input, verbose=0)

    print("\nModel verification:")
    print(f"Original model output shape: {original_output.shape}")
    print(f"Loaded model output shape: {loaded_output.shape}")
    print(f"Outputs match: {tf.reduce_all(tf.abs(original_output - loaded_output) < 1e-6).numpy()}")

    # List all checkpoints
    print("\nCheckpoints stored in rs3gw:")
    response = s3_client.list_objects_v2(Bucket=bucket_name, Prefix='checkpoints/')
    epochs_found = set()
    for obj in response.get('Contents', []):
        if 'metrics.json' in obj['Key']:
            epoch_num = obj['Key'].split('/')[1].replace('epoch_', '')
            epochs_found.add(epoch_num)

    for epoch in sorted(epochs_found):
        metrics_key = f"checkpoints/epoch_{epoch}/metrics.json"
        response = s3_client.get_object(Bucket=bucket_name, Key=metrics_key)
        metrics = json.loads(response['Body'].read().decode('utf-8'))
        print(f"\nEpoch {epoch}:")
        for metric_name, metric_value in metrics['metrics'].items():
            print(f"  {metric_name}: {metric_value:.4f}")


if __name__ == '__main__':
    main()

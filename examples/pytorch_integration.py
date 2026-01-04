#!/usr/bin/env python3
"""
PyTorch Integration Example with rs3gw

This example demonstrates how to use rs3gw as a storage backend for:
1. Training data (datasets)
2. Model checkpoints
3. Training metrics and logs

Requirements:
    pip install torch torchvision boto3
"""

import io
import os
import torch
import torch.nn as nn
import torch.optim as optim
from torch.utils.data import Dataset, DataLoader
import boto3
from botocore.client import Config


class S3Dataset(Dataset):
    """
    Custom PyTorch Dataset that loads data from rs3gw/S3
    """
    def __init__(self, bucket_name, prefix, s3_client, transform=None):
        self.bucket = bucket_name
        self.prefix = prefix
        self.s3 = s3_client
        self.transform = transform

        # List all objects in the prefix
        response = s3_client.list_objects_v2(Bucket=bucket_name, Prefix=prefix)
        self.objects = [obj['Key'] for obj in response.get('Contents', [])]
        print(f"Found {len(self.objects)} objects in {bucket_name}/{prefix}")

    def __len__(self):
        return len(self.objects)

    def __getitem__(self, idx):
        # Download object from rs3gw
        key = self.objects[idx]
        response = self.s3.get_object(Bucket=self.bucket, Key=key)
        data = response['Body'].read()

        # Deserialize tensor (assuming it was saved with torch.save)
        buffer = io.BytesIO(data)
        tensor = torch.load(buffer, weights_only=True)

        if self.transform:
            tensor = self.transform(tensor)

        return tensor


class SimpleModel(nn.Module):
    """Simple neural network for demonstration"""
    def __init__(self, input_size=784, hidden_size=256, output_size=10):
        super(SimpleModel, self).__init__()
        self.fc1 = nn.Linear(input_size, hidden_size)
        self.relu = nn.ReLU()
        self.fc2 = nn.Linear(hidden_size, output_size)

    def forward(self, x):
        x = self.fc1(x)
        x = self.relu(x)
        x = self.fc2(x)
        return x


def save_checkpoint_to_s3(model, optimizer, epoch, loss, s3_client, bucket, key):
    """
    Save model checkpoint to rs3gw/S3
    """
    checkpoint = {
        'epoch': epoch,
        'model_state_dict': model.state_dict(),
        'optimizer_state_dict': optimizer.state_dict(),
        'loss': loss,
    }

    # Serialize to bytes
    buffer = io.BytesIO()
    torch.save(checkpoint, buffer)
    buffer.seek(0)

    # Upload to rs3gw
    s3_client.put_object(
        Bucket=bucket,
        Key=key,
        Body=buffer.getvalue(),
        Metadata={
            'epoch': str(epoch),
            'loss': str(loss),
            'framework': 'pytorch'
        }
    )
    print(f"Checkpoint saved to s3://{bucket}/{key}")


def load_checkpoint_from_s3(model, optimizer, s3_client, bucket, key):
    """
    Load model checkpoint from rs3gw/S3
    """
    response = s3_client.get_object(Bucket=bucket, Key=key)
    data = response['Body'].read()

    buffer = io.BytesIO(data)
    checkpoint = torch.load(buffer, weights_only=False)

    model.load_state_dict(checkpoint['model_state_dict'])
    optimizer.load_state_dict(checkpoint['optimizer_state_dict'])
    epoch = checkpoint['epoch']
    loss = checkpoint['loss']

    print(f"Checkpoint loaded from s3://{bucket}/{key} (epoch: {epoch}, loss: {loss})")
    return epoch, loss


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

    bucket_name = 'ml-training'

    # Create bucket if it doesn't exist
    try:
        s3_client.create_bucket(Bucket=bucket_name)
        print(f"Created bucket: {bucket_name}")
    except s3_client.exceptions.BucketAlreadyOwnedByYou:
        print(f"Bucket already exists: {bucket_name}")
    except Exception as e:
        print(f"Error creating bucket: {e}")

    # Initialize model
    model = SimpleModel()
    optimizer = optim.Adam(model.parameters(), lr=0.001)
    criterion = nn.CrossEntropyLoss()

    # Training loop example
    num_epochs = 5
    for epoch in range(num_epochs):
        # Simulated training step
        inputs = torch.randn(32, 784)  # batch of 32, 784 features
        targets = torch.randint(0, 10, (32,))  # 32 labels

        # Forward pass
        outputs = model(inputs)
        loss = criterion(outputs, targets)

        # Backward pass
        optimizer.zero_grad()
        loss.backward()
        optimizer.step()

        print(f"Epoch {epoch + 1}/{num_epochs}, Loss: {loss.item():.4f}")

        # Save checkpoint every epoch
        checkpoint_key = f"checkpoints/model_epoch_{epoch + 1}.pt"
        save_checkpoint_to_s3(
            model, optimizer, epoch + 1, loss.item(),
            s3_client, bucket_name, checkpoint_key
        )

    print("\nTraining complete!")

    # Demonstrate checkpoint loading
    print("\nLoading checkpoint from rs3gw...")
    new_model = SimpleModel()
    new_optimizer = optim.Adam(new_model.parameters(), lr=0.001)
    loaded_epoch, loaded_loss = load_checkpoint_from_s3(
        new_model, new_optimizer, s3_client, bucket_name,
        f"checkpoints/model_epoch_{num_epochs}.pt"
    )

    # Save final model
    print("\nSaving final model...")
    final_buffer = io.BytesIO()
    torch.save(model.state_dict(), final_buffer)
    final_buffer.seek(0)

    s3_client.put_object(
        Bucket=bucket_name,
        Key='models/final_model.pt',
        Body=final_buffer.getvalue(),
        Metadata={
            'model_type': 'SimpleModel',
            'framework': 'pytorch',
            'input_size': '784',
            'output_size': '10'
        }
    )
    print("Final model saved to s3://ml-training/models/final_model.pt")

    # List all checkpoints
    print("\nCheckpoints stored in rs3gw:")
    response = s3_client.list_objects_v2(Bucket=bucket_name, Prefix='checkpoints/')
    for obj in response.get('Contents', []):
        # Get object metadata
        head = s3_client.head_object(Bucket=bucket_name, Key=obj['Key'])
        metadata = head.get('Metadata', {})
        print(f"  - {obj['Key']}")
        print(f"    Size: {obj['Size']} bytes")
        print(f"    Epoch: {metadata.get('epoch', 'N/A')}")
        print(f"    Loss: {metadata.get('loss', 'N/A')}")


if __name__ == '__main__':
    main()

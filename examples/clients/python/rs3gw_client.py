#!/usr/bin/env python3
"""
rs3gw Python Client Example

Demonstrates how to use boto3 with rs3gw for S3-compatible operations.

Requirements:
    pip install boto3 requests

Usage:
    python rs3gw_client.py
"""

import boto3
from botocore.client import Config
from botocore.exceptions import ClientError
import json
import sys
from pathlib import Path


class Rs3gwClient:
    """
    High-level client for rs3gw operations.

    Wraps boto3 S3 client with rs3gw-specific configuration.
    """

    def __init__(self, endpoint_url='http://localhost:9000',
                 access_key='', secret_key='', region='us-east-1'):
        """
        Initialize rs3gw client.

        Args:
            endpoint_url: rs3gw server endpoint
            access_key: Access key (empty for development mode)
            secret_key: Secret key (empty for development mode)
            region: AWS region (for compatibility)
        """
        self.endpoint_url = endpoint_url

        # Configure boto3 client
        self.s3 = boto3.client(
            's3',
            endpoint_url=endpoint_url,
            aws_access_key_id=access_key,
            aws_secret_access_key=secret_key,
            region_name=region,
            config=Config(
                signature_version='s3v4',
                s3={'addressing_style': 'path'}  # Use path-style URLs
            )
        )

    def create_bucket(self, bucket_name):
        """Create a new bucket."""
        try:
            self.s3.create_bucket(Bucket=bucket_name)
            print(f"✓ Created bucket: {bucket_name}")
            return True
        except ClientError as e:
            print(f"✗ Failed to create bucket: {e}")
            return False

    def list_buckets(self):
        """List all buckets."""
        try:
            response = self.s3.list_buckets()
            buckets = [b['Name'] for b in response.get('Buckets', [])]
            print(f"✓ Found {len(buckets)} buckets:")
            for bucket in buckets:
                print(f"  - {bucket}")
            return buckets
        except ClientError as e:
            print(f"✗ Failed to list buckets: {e}")
            return []

    def upload_file(self, bucket_name, file_path, object_key=None):
        """
        Upload a file to a bucket.

        Args:
            bucket_name: Target bucket
            file_path: Local file path
            object_key: S3 object key (defaults to filename)
        """
        if object_key is None:
            object_key = Path(file_path).name

        try:
            self.s3.upload_file(file_path, bucket_name, object_key)
            print(f"✓ Uploaded {file_path} to {bucket_name}/{object_key}")
            return True
        except ClientError as e:
            print(f"✗ Failed to upload file: {e}")
            return False

    def download_file(self, bucket_name, object_key, local_path):
        """Download an object to a local file."""
        try:
            self.s3.download_file(bucket_name, object_key, local_path)
            print(f"✓ Downloaded {bucket_name}/{object_key} to {local_path}")
            return True
        except ClientError as e:
            print(f"✗ Failed to download file: {e}")
            return False

    def upload_data(self, bucket_name, object_key, data, metadata=None):
        """
        Upload data (bytes or string) to an object.

        Args:
            bucket_name: Target bucket
            object_key: S3 object key
            data: Data to upload (bytes or str)
            metadata: Optional custom metadata dict
        """
        if isinstance(data, str):
            data = data.encode('utf-8')

        try:
            extra_args = {}
            if metadata:
                extra_args['Metadata'] = metadata

            self.s3.put_object(
                Bucket=bucket_name,
                Key=object_key,
                Body=data,
                **extra_args
            )
            print(f"✓ Uploaded data to {bucket_name}/{object_key}")
            return True
        except ClientError as e:
            print(f"✗ Failed to upload data: {e}")
            return False

    def get_object(self, bucket_name, object_key):
        """Get object data and metadata."""
        try:
            response = self.s3.get_object(Bucket=bucket_name, Key=object_key)
            data = response['Body'].read()
            metadata = {
                'size': response['ContentLength'],
                'content_type': response.get('ContentType', 'application/octet-stream'),
                'etag': response['ETag'].strip('"'),
                'last_modified': response['LastModified'],
                'metadata': response.get('Metadata', {})
            }
            print(f"✓ Retrieved {bucket_name}/{object_key} ({metadata['size']} bytes)")
            return data, metadata
        except ClientError as e:
            print(f"✗ Failed to get object: {e}")
            return None, None

    def list_objects(self, bucket_name, prefix='', max_keys=1000):
        """List objects in a bucket."""
        try:
            response = self.s3.list_objects_v2(
                Bucket=bucket_name,
                Prefix=prefix,
                MaxKeys=max_keys
            )

            objects = []
            for obj in response.get('Contents', []):
                objects.append({
                    'key': obj['Key'],
                    'size': obj['Size'],
                    'last_modified': obj['LastModified'],
                    'etag': obj['ETag'].strip('"')
                })

            print(f"✓ Found {len(objects)} objects in {bucket_name}")
            return objects
        except ClientError as e:
            print(f"✗ Failed to list objects: {e}")
            return []

    def delete_object(self, bucket_name, object_key):
        """Delete an object."""
        try:
            self.s3.delete_object(Bucket=bucket_name, Key=object_key)
            print(f"✓ Deleted {bucket_name}/{object_key}")
            return True
        except ClientError as e:
            print(f"✗ Failed to delete object: {e}")
            return False

    def delete_bucket(self, bucket_name, force=False):
        """
        Delete a bucket.

        Args:
            bucket_name: Bucket to delete
            force: If True, delete all objects first
        """
        if force:
            # Delete all objects first
            objects = self.list_objects(bucket_name)
            for obj in objects:
                self.delete_object(bucket_name, obj['key'])

        try:
            self.s3.delete_bucket(Bucket=bucket_name)
            print(f"✓ Deleted bucket: {bucket_name}")
            return True
        except ClientError as e:
            print(f"✗ Failed to delete bucket: {e}")
            return False

    def put_object_tagging(self, bucket_name, object_key, tags):
        """
        Set object tags.

        Args:
            bucket_name: Bucket name
            object_key: Object key
            tags: Dict of tag key-value pairs
        """
        try:
            tag_set = [{'Key': k, 'Value': v} for k, v in tags.items()]
            self.s3.put_object_tagging(
                Bucket=bucket_name,
                Key=object_key,
                Tagging={'TagSet': tag_set}
            )
            print(f"✓ Tagged {bucket_name}/{object_key}")
            return True
        except ClientError as e:
            print(f"✗ Failed to set tags: {e}")
            return False

    def get_object_tagging(self, bucket_name, object_key):
        """Get object tags."""
        try:
            response = self.s3.get_object_tagging(
                Bucket=bucket_name,
                Key=object_key
            )
            tags = {tag['Key']: tag['Value'] for tag in response.get('TagSet', [])}
            print(f"✓ Tags for {bucket_name}/{object_key}: {tags}")
            return tags
        except ClientError as e:
            print(f"✗ Failed to get tags: {e}")
            return {}

    def multipart_upload(self, bucket_name, object_key, file_path, part_size_mb=5):
        """
        Upload large file using multipart upload.

        Args:
            bucket_name: Target bucket
            object_key: S3 object key
            file_path: Local file path
            part_size_mb: Part size in MB
        """
        try:
            # Create multipart upload
            response = self.s3.create_multipart_upload(
                Bucket=bucket_name,
                Key=object_key
            )
            upload_id = response['UploadId']
            print(f"✓ Started multipart upload: {upload_id}")

            # Upload parts
            parts = []
            part_size = part_size_mb * 1024 * 1024

            with open(file_path, 'rb') as f:
                part_number = 1
                while True:
                    data = f.read(part_size)
                    if not data:
                        break

                    part_response = self.s3.upload_part(
                        Bucket=bucket_name,
                        Key=object_key,
                        PartNumber=part_number,
                        UploadId=upload_id,
                        Body=data
                    )

                    parts.append({
                        'ETag': part_response['ETag'],
                        'PartNumber': part_number
                    })

                    print(f"  ✓ Uploaded part {part_number}")
                    part_number += 1

            # Complete upload
            self.s3.complete_multipart_upload(
                Bucket=bucket_name,
                Key=object_key,
                UploadId=upload_id,
                MultipartUpload={'Parts': parts}
            )

            print(f"✓ Completed multipart upload: {bucket_name}/{object_key}")
            return True

        except ClientError as e:
            print(f"✗ Failed multipart upload: {e}")
            # Abort upload on failure
            try:
                self.s3.abort_multipart_upload(
                    Bucket=bucket_name,
                    Key=object_key,
                    UploadId=upload_id
                )
            except:
                pass
            return False


def demo():
    """Run a complete demo of rs3gw operations."""
    print("=" * 60)
    print("rs3gw Python Client Demo")
    print("=" * 60)

    # Initialize client (development mode - no auth)
    client = Rs3gwClient(
        endpoint_url='http://localhost:9000',
        access_key='',
        secret_key=''
    )

    bucket_name = 'test-bucket-python'

    print("\n1. Create Bucket")
    print("-" * 60)
    client.create_bucket(bucket_name)

    print("\n2. List Buckets")
    print("-" * 60)
    client.list_buckets()

    print("\n3. Upload Data")
    print("-" * 60)
    client.upload_data(
        bucket_name,
        'test.txt',
        'Hello from Python!',
        metadata={'author': 'python-client', 'version': '1.0'}
    )

    print("\n4. Get Object")
    print("-" * 60)
    data, metadata = client.get_object(bucket_name, 'test.txt')
    if data:
        print(f"  Data: {data.decode('utf-8')}")
        print(f"  Metadata: {json.dumps(metadata, default=str, indent=2)}")

    print("\n5. Set Tags")
    print("-" * 60)
    client.put_object_tagging(
        bucket_name,
        'test.txt',
        {'environment': 'development', 'project': 'rs3gw-demo'}
    )

    print("\n6. Get Tags")
    print("-" * 60)
    client.get_object_tagging(bucket_name, 'test.txt')

    print("\n7. List Objects")
    print("-" * 60)
    objects = client.list_objects(bucket_name)
    for obj in objects:
        print(f"  - {obj['key']} ({obj['size']} bytes)")

    print("\n8. Cleanup")
    print("-" * 60)
    client.delete_bucket(bucket_name, force=True)

    print("\n" + "=" * 60)
    print("Demo Complete!")
    print("=" * 60)


if __name__ == '__main__':
    try:
        demo()
    except KeyboardInterrupt:
        print("\n\nInterrupted by user")
        sys.exit(1)
    except Exception as e:
        print(f"\n\nError: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)

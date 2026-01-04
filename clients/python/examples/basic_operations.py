#!/usr/bin/env python3
"""
Basic operations example for rs3gw Python gRPC client.

This example demonstrates CRUD operations for buckets and objects.
"""

import grpc
import sys
from rs3gw_client import s3_pb2, s3_pb2_grpc
from rs3gw_client import bucket_pb2, bucket_pb2_grpc
from rs3gw_client import object_pb2, object_pb2_grpc


def run_basic_operations():
    """Run basic S3 operations via gRPC."""

    # Connect to rs3gw gRPC server
    with grpc.insecure_channel('localhost:9001') as channel:
        # Create clients
        s3_client = s3_pb2_grpc.S3ServiceStub(channel)
        bucket_client = bucket_pb2_grpc.BucketServiceStub(channel)
        object_client = object_pb2_grpc.ObjectServiceStub(channel)

        print("=== Bucket Operations ===\n")

        # List buckets
        print("Listing buckets...")
        try:
            response = bucket_client.ListBuckets(bucket_pb2.ListBucketsRequest())
            print(f"Found {len(response.buckets)} buckets:")
            for bucket in response.buckets:
                print(f"  - {bucket.name} (created: {bucket.creation_date})")
        except grpc.RpcError as e:
            print(f"Error listing buckets: {e.code()} - {e.details()}")

        # Create bucket
        bucket_name = "test-bucket-python"
        print(f"\nCreating bucket: {bucket_name}")
        try:
            bucket_client.CreateBucket(bucket_pb2.CreateBucketRequest(
                bucket=bucket_name,
                region="us-east-1"
            ))
            print(f"Bucket '{bucket_name}' created successfully")
        except grpc.RpcError as e:
            if e.code() == grpc.StatusCode.ALREADY_EXISTS:
                print(f"Bucket already exists")
            else:
                print(f"Error: {e.code()} - {e.details()}")

        # Head bucket (check existence)
        print(f"\nChecking if bucket exists: {bucket_name}")
        try:
            response = bucket_client.HeadBucket(bucket_pb2.HeadBucketRequest(
                bucket=bucket_name
            ))
            print(f"Bucket exists: {response.exists}")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        print("\n=== Object Operations ===\n")

        # Put object
        object_key = "data/hello.txt"
        object_data = b"Hello from Python gRPC client!"
        print(f"Uploading object: {object_key}")
        try:
            response = object_client.PutObject(object_pb2.PutObjectRequest(
                bucket=bucket_name,
                key=object_key,
                data=object_data,
                content_type="text/plain",
                metadata={"author": "python-client", "version": "1.0"}
            ))
            print(f"Object uploaded successfully")
            print(f"  ETag: {response.etag}")
            print(f"  Size: {len(object_data)} bytes")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        # Get object
        print(f"\nDownloading object: {object_key}")
        try:
            response = object_client.GetObject(object_pb2.GetObjectRequest(
                bucket=bucket_name,
                key=object_key
            ))
            print(f"Object downloaded successfully")
            print(f"  Content-Type: {response.content_type}")
            print(f"  Size: {response.size} bytes")
            print(f"  Data: {response.data.decode('utf-8')}")
            print(f"  Metadata: {dict(response.metadata)}")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        # Head object
        print(f"\nGetting object metadata: {object_key}")
        try:
            response = object_client.HeadObject(object_pb2.HeadObjectRequest(
                bucket=bucket_name,
                key=object_key
            ))
            print(f"Object metadata:")
            print(f"  Content-Type: {response.content_type}")
            print(f"  Size: {response.size} bytes")
            print(f"  ETag: {response.etag}")
            print(f"  Last-Modified: {response.last_modified}")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        # List objects
        print(f"\nListing objects in bucket: {bucket_name}")
        try:
            response = object_client.ListObjects(object_pb2.ListObjectsRequest(
                bucket=bucket_name,
                prefix="data/",
                max_keys=1000
            ))
            print(f"Found {len(response.contents)} objects:")
            for obj in response.contents:
                print(f"  - {obj.key} ({obj.size} bytes, ETag: {obj.etag})")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        print("\n=== Tagging Operations ===\n")

        # Put object tags
        print(f"Adding tags to object: {object_key}")
        try:
            object_client.PutObjectTagging(object_pb2.PutObjectTaggingRequest(
                bucket=bucket_name,
                key=object_key,
                tags={"environment": "development", "project": "rs3gw-test"}
            ))
            print("Tags added successfully")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        # Get object tags
        print(f"\nGetting tags for object: {object_key}")
        try:
            response = object_client.GetObjectTagging(object_pb2.GetObjectTaggingRequest(
                bucket=bucket_name,
                key=object_key
            ))
            print(f"Object tags: {dict(response.tags)}")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        print("\n=== Cleanup ===\n")

        # Delete object
        print(f"Deleting object: {object_key}")
        try:
            object_client.DeleteObject(object_pb2.DeleteObjectRequest(
                bucket=bucket_name,
                key=object_key
            ))
            print("Object deleted successfully")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")

        # Delete bucket
        print(f"\nDeleting bucket: {bucket_name}")
        try:
            bucket_client.DeleteBucket(bucket_pb2.DeleteBucketRequest(
                bucket=bucket_name
            ))
            print("Bucket deleted successfully")
        except grpc.RpcError as e:
            print(f"Error: {e.code()} - {e.details()}")


if __name__ == '__main__':
    print("rs3gw Python gRPC Client - Basic Operations Example\n")
    print("Make sure rs3gw server is running on localhost:9001\n")

    try:
        run_basic_operations()
        print("\n=== Example completed successfully ===")
    except Exception as e:
        print(f"\nFatal error: {e}", file=sys.stderr)
        sys.exit(1)

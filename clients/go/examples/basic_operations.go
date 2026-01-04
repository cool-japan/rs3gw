package main

// Basic operations example for rs3gw Go gRPC client.
// This example demonstrates CRUD operations for buckets and objects.

import (
	"context"
	"fmt"
	"log"
	"time"

	pb "github.com/cool-japan/rs3gw/clients/go/rs3gw"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"
)

func main() {
	fmt.Println("rs3gw Go gRPC Client - Basic Operations Example\n")
	fmt.Println("Make sure rs3gw server is running on localhost:9001\n")

	// Connect to rs3gw gRPC server
	conn, err := grpc.Dial("localhost:9001",
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
		grpc.WithTimeout(5*time.Second),
	)
	if err != nil {
		log.Fatalf("Failed to connect: %v", err)
	}
	defer conn.Close()

	// Create clients
	s3Client := pb.NewS3ServiceClient(conn)
	bucketClient := pb.NewBucketServiceClient(conn)
	objectClient := pb.NewObjectServiceClient(conn)

	ctx := context.Background()

	// Bucket operations
	fmt.Println("=== Bucket Operations ===\n")

	// List buckets
	fmt.Println("Listing buckets...")
	listResp, err := bucketClient.ListBuckets(ctx, &pb.ListBucketsRequest{})
	if err != nil {
		log.Fatalf("Failed to list buckets: %v", err)
	}
	fmt.Printf("Found %d buckets:\n", len(listResp.Buckets))
	for _, bucket := range listResp.Buckets {
		fmt.Printf("  - %s (created: %s)\n", bucket.Name, bucket.CreationDate)
	}

	// Create bucket
	bucketName := "test-bucket-go"
	fmt.Printf("\nCreating bucket: %s\n", bucketName)
	_, err = bucketClient.CreateBucket(ctx, &pb.CreateBucketRequest{
		Bucket: bucketName,
		Region: "us-east-1",
	})
	if err != nil {
		if st, ok := status.FromError(err); ok && st.Code() == codes.AlreadyExists {
			fmt.Println("Bucket already exists")
		} else {
			log.Fatalf("Failed to create bucket: %v", err)
		}
	} else {
		fmt.Printf("Bucket '%s' created successfully\n", bucketName)
	}

	// Head bucket
	fmt.Printf("\nChecking if bucket exists: %s\n", bucketName)
	headResp, err := bucketClient.HeadBucket(ctx, &pb.HeadBucketRequest{
		Bucket: bucketName,
	})
	if err != nil {
		log.Fatalf("Failed to head bucket: %v", err)
	}
	fmt.Printf("Bucket exists: %v\n", headResp.Exists)

	// Object operations
	fmt.Println("\n=== Object Operations ===\n")

	// Put object
	objectKey := "data/hello.txt"
	objectData := []byte("Hello from Go gRPC client!")
	fmt.Printf("Uploading object: %s\n", objectKey)
	putResp, err := objectClient.PutObject(ctx, &pb.PutObjectRequest{
		Bucket:      bucketName,
		Key:         objectKey,
		Data:        objectData,
		ContentType: "text/plain",
		Metadata: map[string]string{
			"author":  "go-client",
			"version": "1.0",
		},
	})
	if err != nil {
		log.Fatalf("Failed to put object: %v", err)
	}
	fmt.Println("Object uploaded successfully")
	fmt.Printf("  ETag: %s\n", putResp.Etag)
	fmt.Printf("  Size: %d bytes\n", len(objectData))

	// Get object
	fmt.Printf("\nDownloading object: %s\n", objectKey)
	getResp, err := objectClient.GetObject(ctx, &pb.GetObjectRequest{
		Bucket: bucketName,
		Key:    objectKey,
	})
	if err != nil {
		log.Fatalf("Failed to get object: %v", err)
	}
	fmt.Println("Object downloaded successfully")
	fmt.Printf("  Content-Type: %s\n", getResp.ContentType)
	fmt.Printf("  Size: %d bytes\n", getResp.Size)
	fmt.Printf("  Data: %s\n", string(getResp.Data))
	fmt.Printf("  Metadata: %v\n", getResp.Metadata)

	// Head object
	fmt.Printf("\nGetting object metadata: %s\n", objectKey)
	headObjResp, err := objectClient.HeadObject(ctx, &pb.HeadObjectRequest{
		Bucket: bucketName,
		Key:    objectKey,
	})
	if err != nil {
		log.Fatalf("Failed to head object: %v", err)
	}
	fmt.Println("Object metadata:")
	fmt.Printf("  Content-Type: %s\n", headObjResp.ContentType)
	fmt.Printf("  Size: %d bytes\n", headObjResp.Size)
	fmt.Printf("  ETag: %s\n", headObjResp.Etag)
	fmt.Printf("  Last-Modified: %s\n", headObjResp.LastModified)

	// List objects
	fmt.Printf("\nListing objects in bucket: %s\n", bucketName)
	listObjResp, err := objectClient.ListObjects(ctx, &pb.ListObjectsRequest{
		Bucket:  bucketName,
		Prefix:  "data/",
		MaxKeys: 1000,
	})
	if err != nil {
		log.Fatalf("Failed to list objects: %v", err)
	}
	fmt.Printf("Found %d objects:\n", len(listObjResp.Contents))
	for _, obj := range listObjResp.Contents {
		fmt.Printf("  - %s (%d bytes, ETag: %s)\n", obj.Key, obj.Size, obj.Etag)
	}

	// Tagging operations
	fmt.Println("\n=== Tagging Operations ===\n")

	// Put object tags
	fmt.Printf("Adding tags to object: %s\n", objectKey)
	_, err = objectClient.PutObjectTagging(ctx, &pb.PutObjectTaggingRequest{
		Bucket: bucketName,
		Key:    objectKey,
		Tags: map[string]string{
			"environment": "development",
			"project":     "rs3gw-test",
		},
	})
	if err != nil {
		log.Fatalf("Failed to put object tags: %v", err)
	}
	fmt.Println("Tags added successfully")

	// Get object tags
	fmt.Printf("\nGetting tags for object: %s\n", objectKey)
	getTagsResp, err := objectClient.GetObjectTagging(ctx, &pb.GetObjectTaggingRequest{
		Bucket: bucketName,
		Key:    objectKey,
	})
	if err != nil {
		log.Fatalf("Failed to get object tags: %v", err)
	}
	fmt.Printf("Object tags: %v\n", getTagsResp.Tags)

	// Cleanup
	fmt.Println("\n=== Cleanup ===\n")

	// Delete object
	fmt.Printf("Deleting object: %s\n", objectKey)
	_, err = objectClient.DeleteObject(ctx, &pb.DeleteObjectRequest{
		Bucket: bucketName,
		Key:    objectKey,
	})
	if err != nil {
		log.Fatalf("Failed to delete object: %v", err)
	}
	fmt.Println("Object deleted successfully")

	// Delete bucket
	fmt.Printf("\nDeleting bucket: %s\n", bucketName)
	_, err = bucketClient.DeleteBucket(ctx, &pb.DeleteBucketRequest{
		Bucket: bucketName,
	})
	if err != nil {
		log.Fatalf("Failed to delete bucket: %v", err)
	}
	fmt.Println("Bucket deleted successfully")

	fmt.Println("\n=== Example completed successfully ===")
}

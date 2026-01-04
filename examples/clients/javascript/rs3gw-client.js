/**
 * rs3gw JavaScript/Node.js Client Example
 *
 * Demonstrates how to use AWS SDK v3 with rs3gw for S3-compatible operations.
 *
 * Requirements:
 *   npm install @aws-sdk/client-s3 @aws-sdk/lib-storage
 *
 * Usage:
 *   node rs3gw-client.js
 */

const {
    S3Client,
    CreateBucketCommand,
    ListBucketsCommand,
    PutObjectCommand,
    GetObjectCommand,
    ListObjectsV2Command,
    DeleteObjectCommand,
    DeleteBucketCommand,
    PutObjectTaggingCommand,
    GetObjectTaggingCommand
} = require('@aws-sdk/client-s3');
const { Upload } = require('@aws-sdk/lib-storage');
const fs = require('fs');
const path = require('path');

/**
 * Rs3gwClient - High-level client for rs3gw operations
 */
class Rs3gwClient {
    /**
     * Initialize rs3gw client
     *
     * @param {Object} options - Configuration options
     * @param {string} options.endpoint - rs3gw server endpoint
     * @param {string} options.accessKeyId - Access key (empty for development)
     * @param {string} options.secretAccessKey - Secret key (empty for development)
     * @param {string} options.region - AWS region (for compatibility)
     */
    constructor(options = {}) {
        const {
            endpoint = 'http://localhost:9000',
            accessKeyId = '',
            secretAccessKey = '',
            region = 'us-east-1'
        } = options;

        this.s3Client = new S3Client({
            endpoint,
            region,
            credentials: {
                accessKeyId,
                secretAccessKey
            },
            forcePathStyle: true  // Use path-style URLs
        });

        this.endpoint = endpoint;
    }

    /**
     * Create a new bucket
     */
    async createBucket(bucketName) {
        try {
            await this.s3Client.send(new CreateBucketCommand({
                Bucket: bucketName
            }));
            console.log(`✓ Created bucket: ${bucketName}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed to create bucket: ${error.message}`);
            return false;
        }
    }

    /**
     * List all buckets
     */
    async listBuckets() {
        try {
            const response = await this.s3Client.send(new ListBucketsCommand({}));
            const buckets = response.Buckets || [];
            console.log(`✓ Found ${buckets.length} buckets:`);
            buckets.forEach(bucket => {
                console.log(`  - ${bucket.Name}`);
            });
            return buckets.map(b => b.Name);
        } catch (error) {
            console.error(`✗ Failed to list buckets: ${error.message}`);
            return [];
        }
    }

    /**
     * Upload data to an object
     *
     * @param {string} bucketName - Target bucket
     * @param {string} objectKey - S3 object key
     * @param {string|Buffer} data - Data to upload
     * @param {Object} metadata - Optional custom metadata
     */
    async uploadData(bucketName, objectKey, data, metadata = {}) {
        try {
            const body = typeof data === 'string' ? Buffer.from(data, 'utf-8') : data;

            await this.s3Client.send(new PutObjectCommand({
                Bucket: bucketName,
                Key: objectKey,
                Body: body,
                Metadata: metadata
            }));

            console.log(`✓ Uploaded data to ${bucketName}/${objectKey}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed to upload data: ${error.message}`);
            return false;
        }
    }

    /**
     * Upload a file
     *
     * @param {string} bucketName - Target bucket
     * @param {string} filePath - Local file path
     * @param {string} objectKey - S3 object key (defaults to filename)
     */
    async uploadFile(bucketName, filePath, objectKey = null) {
        try {
            objectKey = objectKey || path.basename(filePath);
            const fileStream = fs.createReadStream(filePath);

            await this.s3Client.send(new PutObjectCommand({
                Bucket: bucketName,
                Key: objectKey,
                Body: fileStream
            }));

            console.log(`✓ Uploaded ${filePath} to ${bucketName}/${objectKey}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed to upload file: ${error.message}`);
            return false;
        }
    }

    /**
     * Get object data and metadata
     */
    async getObject(bucketName, objectKey) {
        try {
            const response = await this.s3Client.send(new GetObjectCommand({
                Bucket: bucketName,
                Key: objectKey
            }));

            // Read stream to buffer
            const chunks = [];
            for await (const chunk of response.Body) {
                chunks.push(chunk);
            }
            const data = Buffer.concat(chunks);

            const metadata = {
                size: response.ContentLength,
                contentType: response.ContentType || 'application/octet-stream',
                etag: response.ETag.replace(/"/g, ''),
                lastModified: response.LastModified,
                metadata: response.Metadata || {}
            };

            console.log(`✓ Retrieved ${bucketName}/${objectKey} (${metadata.size} bytes)`);
            return { data, metadata };
        } catch (error) {
            console.error(`✗ Failed to get object: ${error.message}`);
            return { data: null, metadata: null };
        }
    }

    /**
     * Download object to file
     */
    async downloadFile(bucketName, objectKey, localPath) {
        try {
            const { data } = await this.getObject(bucketName, objectKey);
            if (!data) return false;

            fs.writeFileSync(localPath, data);
            console.log(`✓ Downloaded ${bucketName}/${objectKey} to ${localPath}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed to download file: ${error.message}`);
            return false;
        }
    }

    /**
     * List objects in a bucket
     */
    async listObjects(bucketName, prefix = '', maxKeys = 1000) {
        try {
            const response = await this.s3Client.send(new ListObjectsV2Command({
                Bucket: bucketName,
                Prefix: prefix,
                MaxKeys: maxKeys
            }));

            const objects = (response.Contents || []).map(obj => ({
                key: obj.Key,
                size: obj.Size,
                lastModified: obj.LastModified,
                etag: obj.ETag.replace(/"/g, '')
            }));

            console.log(`✓ Found ${objects.length} objects in ${bucketName}`);
            return objects;
        } catch (error) {
            console.error(`✗ Failed to list objects: ${error.message}`);
            return [];
        }
    }

    /**
     * Delete an object
     */
    async deleteObject(bucketName, objectKey) {
        try {
            await this.s3Client.send(new DeleteObjectCommand({
                Bucket: bucketName,
                Key: objectKey
            }));
            console.log(`✓ Deleted ${bucketName}/${objectKey}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed to delete object: ${error.message}`);
            return false;
        }
    }

    /**
     * Delete a bucket
     *
     * @param {string} bucketName - Bucket to delete
     * @param {boolean} force - If true, delete all objects first
     */
    async deleteBucket(bucketName, force = false) {
        try {
            if (force) {
                // Delete all objects first
                const objects = await this.listObjects(bucketName);
                for (const obj of objects) {
                    await this.deleteObject(bucketName, obj.key);
                }
            }

            await this.s3Client.send(new DeleteBucketCommand({
                Bucket: bucketName
            }));
            console.log(`✓ Deleted bucket: ${bucketName}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed to delete bucket: ${error.message}`);
            return false;
        }
    }

    /**
     * Set object tags
     *
     * @param {string} bucketName - Bucket name
     * @param {string} objectKey - Object key
     * @param {Object} tags - Object with tag key-value pairs
     */
    async putObjectTagging(bucketName, objectKey, tags) {
        try {
            const tagSet = Object.entries(tags).map(([Key, Value]) => ({
                Key,
                Value
            }));

            await this.s3Client.send(new PutObjectTaggingCommand({
                Bucket: bucketName,
                Key: objectKey,
                Tagging: { TagSet: tagSet }
            }));

            console.log(`✓ Tagged ${bucketName}/${objectKey}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed to set tags: ${error.message}`);
            return false;
        }
    }

    /**
     * Get object tags
     */
    async getObjectTagging(bucketName, objectKey) {
        try {
            const response = await this.s3Client.send(new GetObjectTaggingCommand({
                Bucket: bucketName,
                Key: objectKey
            }));

            const tags = {};
            (response.TagSet || []).forEach(tag => {
                tags[tag.Key] = tag.Value;
            });

            console.log(`✓ Tags for ${bucketName}/${objectKey}:`, tags);
            return tags;
        } catch (error) {
            console.error(`✗ Failed to get tags: ${error.message}`);
            return {};
        }
    }

    /**
     * Multipart upload for large files
     *
     * @param {string} bucketName - Target bucket
     * @param {string} filePath - Local file path
     * @param {string} objectKey - S3 object key (defaults to filename)
     * @param {number} partSize - Part size in bytes (default: 5MB)
     */
    async multipartUpload(bucketName, filePath, objectKey = null, partSize = 5 * 1024 * 1024) {
        try {
            objectKey = objectKey || path.basename(filePath);
            const fileStream = fs.createReadStream(filePath);

            const upload = new Upload({
                client: this.s3Client,
                params: {
                    Bucket: bucketName,
                    Key: objectKey,
                    Body: fileStream
                },
                partSize,
                queueSize: 4  // Concurrent parts
            });

            upload.on('httpUploadProgress', (progress) => {
                const percent = Math.round((progress.loaded / progress.total) * 100);
                console.log(`  Progress: ${percent}%`);
            });

            await upload.done();
            console.log(`✓ Completed multipart upload: ${bucketName}/${objectKey}`);
            return true;
        } catch (error) {
            console.error(`✗ Failed multipart upload: ${error.message}`);
            return false;
        }
    }
}

/**
 * Run a complete demo of rs3gw operations
 */
async function demo() {
    console.log('='.repeat(60));
    console.log('rs3gw JavaScript/Node.js Client Demo');
    console.log('='.repeat(60));

    // Initialize client (development mode - no auth)
    const client = new Rs3gwClient({
        endpoint: 'http://localhost:9000',
        accessKeyId: '',
        secretAccessKey: ''
    });

    const bucketName = 'test-bucket-javascript';

    console.log('\n1. Create Bucket');
    console.log('-'.repeat(60));
    await client.createBucket(bucketName);

    console.log('\n2. List Buckets');
    console.log('-'.repeat(60));
    await client.listBuckets();

    console.log('\n3. Upload Data');
    console.log('-'.repeat(60));
    await client.uploadData(
        bucketName,
        'test.txt',
        'Hello from JavaScript!',
        { author: 'javascript-client', version: '1.0' }
    );

    console.log('\n4. Get Object');
    console.log('-'.repeat(60));
    const { data, metadata } = await client.getObject(bucketName, 'test.txt');
    if (data) {
        console.log(`  Data: ${data.toString('utf-8')}`);
        console.log(`  Metadata:`, JSON.stringify(metadata, null, 2));
    }

    console.log('\n5. Set Tags');
    console.log('-'.repeat(60));
    await client.putObjectTagging(
        bucketName,
        'test.txt',
        { environment: 'development', project: 'rs3gw-demo' }
    );

    console.log('\n6. Get Tags');
    console.log('-'.repeat(60));
    await client.getObjectTagging(bucketName, 'test.txt');

    console.log('\n7. List Objects');
    console.log('-'.repeat(60));
    const objects = await client.listObjects(bucketName);
    objects.forEach(obj => {
        console.log(`  - ${obj.key} (${obj.size} bytes)`);
    });

    console.log('\n8. Cleanup');
    console.log('-'.repeat(60));
    await client.deleteBucket(bucketName, true);

    console.log('\n' + '='.repeat(60));
    console.log('Demo Complete!');
    console.log('='.repeat(60));
}

// Run demo if executed directly
if (require.main === module) {
    demo().catch(error => {
        console.error('\nError:', error);
        process.exit(1);
    });
}

module.exports = { Rs3gwClient };

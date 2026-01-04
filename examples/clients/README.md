# rs3gw Client Examples

This directory contains example client implementations for rs3gw in various programming languages.

## Available Clients

### Python (boto3)
**Location**: `python/rs3gw_client.py`

Full-featured Python client using boto3 (AWS SDK for Python).

**Features**:
- Bucket operations (create, list, delete)
- Object operations (upload, download, list, delete)
- Metadata handling
- Tagging support
- Multipart upload for large files
- Complete demo script

**Requirements**:
```bash
cd python
pip install -r requirements.txt
```

**Usage**:
```bash
# Run demo
python rs3gw_client.py

# Use in your code
from rs3gw_client import Rs3gwClient

client = Rs3gwClient(
    endpoint_url='http://localhost:9000',
    access_key='',
    secret_key=''
)

client.create_bucket('my-bucket')
client.upload_data('my-bucket', 'test.txt', 'Hello World!')
```

### JavaScript/Node.js (AWS SDK v3)
**Location**: `javascript/rs3gw-client.js`

Modern JavaScript client using AWS SDK v3 for Node.js.

**Features**:
- Async/await API
- Bucket operations
- Object operations
- Tagging support
- Multipart upload with progress
- Complete demo script

**Requirements**:
```bash
cd javascript
npm install
```

**Usage**:
```bash
# Run demo
node rs3gw-client.js
npm run demo

# Use in your code
const { Rs3gwClient } = require('./rs3gw-client');

const client = new Rs3gwClient({
    endpoint: 'http://localhost:9000',
    accessKeyId: '',
    secretAccessKey: ''
});

await client.createBucket('my-bucket');
await client.uploadData('my-bucket', 'test.txt', 'Hello World!');
```

## Common Operations

### Create a Bucket

**Python**:
```python
client.create_bucket('my-bucket')
```

**JavaScript**:
```javascript
await client.createBucket('my-bucket');
```

### Upload Data

**Python**:
```python
# Upload string
client.upload_data('my-bucket', 'file.txt', 'Hello World!')

# Upload file
client.upload_file('my-bucket', '/path/to/file.txt')

# With metadata
client.upload_data(
    'my-bucket',
    'file.txt',
    'Hello World!',
    metadata={'author': 'user@example.com', 'version': '1.0'}
)
```

**JavaScript**:
```javascript
// Upload string
await client.uploadData('my-bucket', 'file.txt', 'Hello World!');

// Upload file
await client.uploadFile('my-bucket', '/path/to/file.txt');

// With metadata
await client.uploadData(
    'my-bucket',
    'file.txt',
    'Hello World!',
    { author: 'user@example.com', version: '1.0' }
);
```

### Download Object

**Python**:
```python
# Get object data
data, metadata = client.get_object('my-bucket', 'file.txt')
print(data.decode('utf-8'))

# Download to file
client.download_file('my-bucket', 'file.txt', '/path/to/save.txt')
```

**JavaScript**:
```javascript
// Get object data
const { data, metadata } = await client.getObject('my-bucket', 'file.txt');
console.log(data.toString('utf-8'));

// Download to file
await client.downloadFile('my-bucket', 'file.txt', '/path/to/save.txt');
```

### List Objects

**Python**:
```python
objects = client.list_objects('my-bucket', prefix='documents/')
for obj in objects:
    print(f"{obj['key']}: {obj['size']} bytes")
```

**JavaScript**:
```javascript
const objects = await client.listObjects('my-bucket', 'documents/');
objects.forEach(obj => {
    console.log(`${obj.key}: ${obj.size} bytes`);
});
```

### Object Tagging

**Python**:
```python
# Set tags
client.put_object_tagging(
    'my-bucket',
    'file.txt',
    {'project': 'demo', 'environment': 'dev'}
)

# Get tags
tags = client.get_object_tagging('my-bucket', 'file.txt')
```

**JavaScript**:
```javascript
// Set tags
await client.putObjectTagging(
    'my-bucket',
    'file.txt',
    { project: 'demo', environment: 'dev' }
);

// Get tags
const tags = await client.getObjectTagging('my-bucket', 'file.txt');
```

### Multipart Upload (Large Files)

**Python**:
```python
client.multipart_upload(
    'my-bucket',
    'large-file.bin',
    '/path/to/large-file.bin',
    part_size_mb=10  # 10MB parts
)
```

**JavaScript**:
```javascript
await client.multipartUpload(
    'my-bucket',
    '/path/to/large-file.bin',
    'large-file.bin',
    10 * 1024 * 1024  // 10MB parts
);
```

### Delete Operations

**Python**:
```python
# Delete object
client.delete_object('my-bucket', 'file.txt')

# Delete bucket (force deletes all objects first)
client.delete_bucket('my-bucket', force=True)
```

**JavaScript**:
```javascript
// Delete object
await client.deleteObject('my-bucket', 'file.txt');

// Delete bucket (force deletes all objects first)
await client.deleteBucket('my-bucket', true);
```

## Authentication

### Development Mode (No Auth)

Both clients default to development mode with empty credentials:

**Python**:
```python
client = Rs3gwClient(
    endpoint_url='http://localhost:9000',
    access_key='',
    secret_key=''
)
```

**JavaScript**:
```javascript
const client = new Rs3gwClient({
    endpoint: 'http://localhost:9000',
    accessKeyId: '',
    secretAccessKey: ''
});
```

### Production Mode (With Auth)

**Python**:
```python
client = Rs3gwClient(
    endpoint_url='https://rs3gw.example.com',
    access_key='YOUR_ACCESS_KEY',
    secret_key='YOUR_SECRET_KEY'
)
```

**JavaScript**:
```javascript
const client = new Rs3gwClient({
    endpoint: 'https://rs3gw.example.com',
    accessKeyId: 'YOUR_ACCESS_KEY',
    secretAccessKey: 'YOUR_SECRET_KEY'
});
```

### Environment Variables

**Python**:
```python
import os

client = Rs3gwClient(
    endpoint_url=os.getenv('RS3GW_ENDPOINT', 'http://localhost:9000'),
    access_key=os.getenv('RS3GW_ACCESS_KEY', ''),
    secret_key=os.getenv('RS3GW_SECRET_KEY', '')
)
```

**JavaScript**:
```javascript
const client = new Rs3gwClient({
    endpoint: process.env.RS3GW_ENDPOINT || 'http://localhost:9000',
    accessKeyId: process.env.RS3GW_ACCESS_KEY || '',
    secretAccessKey: process.env.RS3GW_SECRET_KEY || ''
});
```

## Best Practices

### Error Handling

**Python**:
```python
from botocore.exceptions import ClientError

try:
    client.upload_data('my-bucket', 'file.txt', 'data')
except ClientError as e:
    error_code = e.response['Error']['Code']
    if error_code == 'NoSuchBucket':
        print("Bucket does not exist")
    else:
        print(f"Error: {e}")
```

**JavaScript**:
```javascript
try {
    await client.uploadData('my-bucket', 'file.txt', 'data');
} catch (error) {
    if (error.name === 'NoSuchBucket') {
        console.log('Bucket does not exist');
    } else {
        console.error('Error:', error.message);
    }
}
```

### Batch Operations

**Python**:
```python
# Upload multiple files
files = ['file1.txt', 'file2.txt', 'file3.txt']
for file in files:
    client.upload_file('my-bucket', file)

# Delete multiple objects
objects = client.list_objects('my-bucket', prefix='temp/')
for obj in objects:
    client.delete_object('my-bucket', obj['key'])
```

**JavaScript**:
```javascript
// Upload multiple files
const files = ['file1.txt', 'file2.txt', 'file3.txt'];
await Promise.all(files.map(file =>
    client.uploadFile('my-bucket', file)
));

// Delete multiple objects
const objects = await client.listObjects('my-bucket', 'temp/');
await Promise.all(objects.map(obj =>
    client.deleteObject('my-bucket', obj.key)
));
```

### Streaming Large Files

For very large files, use multipart upload to avoid memory issues:

**Python**:
```python
# Automatically handles large files
client.multipart_upload(
    'my-bucket',
    'large-video.mp4',
    '/path/to/large-video.mp4',
    part_size_mb=50  # 50MB parts
)
```

**JavaScript**:
```javascript
// Automatically handles large files
await client.multipartUpload(
    'my-bucket',
    '/path/to/large-video.mp4',
    'large-video.mp4',
    50 * 1024 * 1024  // 50MB parts
);
```

## Integration Examples

### Flask (Python Web App)

```python
from flask import Flask, request, jsonify
from rs3gw_client import Rs3gwClient

app = Flask(__name__)
client = Rs3gwClient()

@app.route('/upload', methods=['POST'])
def upload():
    file = request.files['file']
    bucket = request.form.get('bucket', 'uploads')

    # Upload file
    success = client.upload_data(
        bucket,
        file.filename,
        file.read(),
        metadata={'uploader': request.remote_addr}
    )

    return jsonify({'success': success})

if __name__ == '__main__':
    app.run()
```

### Express (Node.js Web App)

```javascript
const express = require('express');
const multer = require('multer');
const { Rs3gwClient } = require('./rs3gw-client');

const app = express();
const upload = multer();
const client = new Rs3gwClient();

app.post('/upload', upload.single('file'), async (req, res) => {
    try {
        const bucket = req.body.bucket || 'uploads';

        await client.uploadData(
            bucket,
            req.file.originalname,
            req.file.buffer,
            { uploader: req.ip }
        );

        res.json({ success: true });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

app.listen(3000, () => console.log('Server running on port 3000'));
```

## Troubleshooting

### Connection Refused

```
Error: connect ECONNREFUSED 127.0.0.1:9000
```

**Solution**: Ensure rs3gw is running on the specified endpoint.

```bash
# Start rs3gw
rs3gw --bind-addr 0.0.0.0:9000
```

### Authentication Failed

```
Error: The request signature we calculated does not match the signature you provided
```

**Solution**: Check that access key and secret key match rs3gw configuration.

### Bucket Already Exists

```
Error: BucketAlreadyExists
```

**Solution**: Use a different bucket name or delete the existing bucket first.

### SSL/TLS Errors

```
Error: certificate verify failed
```

**Solution**: For self-signed certificates in development:

**Python**:
```python
import boto3
from botocore.config import Config

client = boto3.client(
    's3',
    endpoint_url='https://localhost:9000',
    config=Config(signature_version='s3v4'),
    verify=False  # Disable SSL verification (development only!)
)
```

**JavaScript**:
```javascript
// Set NODE_TLS_REJECT_UNAUTHORIZED=0 (development only!)
process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
```

## Support

- Documentation: `../../docs/`
- rs3ctl CLI: `../../docs/rs3ctl.md`
- Issue Tracker: https://github.com/cool-japan/rs3gw/issues

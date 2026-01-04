# WebSocket Event Streaming

rs3gw provides real-time event notifications via WebSocket connections, enabling clients to receive live updates about S3 operations.

## Endpoint

```
ws://localhost:9000/events/stream
```

Or with TLS:
```
wss://your-domain:9000/events/stream
```

## Query Parameters

- `bucket` (optional): Filter events by bucket name
- `prefix` (optional): Filter events by object key prefix
- `events` (optional): Comma-separated list of event types to subscribe to

## Event Types

- `object-created` - Object uploaded (PUT, POST, COPY)
- `object-removed` - Object deleted
- `object-metadata-changed` - Object metadata modified
- `object-accessed` - Object retrieved (GET)
- `multipart-upload-created` - Multipart upload initiated
- `multipart-upload-completed` - Multipart upload finalized
- `multipart-upload-aborted` - Multipart upload canceled
- `bucket-created` - Bucket created
- `bucket-removed` - Bucket deleted
- `bucket-policy-changed` - Bucket policy modified
- `bucket-tagging-changed` - Bucket tags modified

## Usage Examples

### JavaScript/Browser

```javascript
// Connect to event stream
const ws = new WebSocket('ws://localhost:9000/events/stream?bucket=my-bucket&events=object-created,object-removed');

// Handle connection open
ws.onopen = () => {
  console.log('Connected to rs3gw event stream');
};

// Handle incoming events
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  if (data.type === 'welcome') {
    console.log('Welcome:', data.message);
    console.log('Filters:', data.filters);
  } else {
    // S3 event
    console.log('Event:', data.eventType);
    console.log('Bucket:', data.bucket);
    console.log('Key:', data.key);
    console.log('Size:', data.size);
    console.log('ETag:', data.etag);
  }
};

// Handle errors
ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

// Handle connection close
ws.onclose = () => {
  console.log('Disconnected from event stream');
};
```

### Python

```python
import asyncio
import websockets
import json

async def stream_events():
    uri = "ws://localhost:9000/events/stream?bucket=my-bucket&prefix=data/"

    async with websockets.connect(uri) as websocket:
        while True:
            message = await websocket.recv()
            event = json.loads(message)

            if event.get('type') == 'welcome':
                print(f"Connected: {event['message']}")
                print(f"Filters: {event['filters']}")
            else:
                print(f"Event: {event['eventType']}")
                print(f"  Bucket: {event['bucket']}")
                if 'key' in event:
                    print(f"  Key: {event['key']}")
                if 'size' in event:
                    print(f"  Size: {event['size']} bytes")

asyncio.run(stream_events())
```

### Rust

```rust
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::{StreamExt, SinkExt};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "ws://localhost:9000/events/stream?events=object-created";
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        let msg = msg?;

        if let Message::Text(text) = msg {
            let event: Value = serde_json::from_str(&text)?;

            if event["type"] == "welcome" {
                println!("Connected: {}", event["message"]);
            } else {
                println!("Event: {}", event["eventType"]);
                println!("  Bucket: {}", event["bucket"]);
                if let Some(key) = event.get("key") {
                    println!("  Key: {}", key);
                }
            }
        }
    }

    Ok(())
}
```

## Event Format

Each event is sent as a JSON object with the following structure:

```json
{
  "eventId": "550e8400-e29b-41d4-a716-446655440000",
  "eventType": "object-created",
  "eventTime": "2024-01-15T10:30:45.123Z",
  "bucket": "my-bucket",
  "key": "path/to/object.txt",
  "size": 1024,
  "etag": "abc123def456",
  "metadata": {
    "custom-key": "custom-value"
  }
}
```

### Field Descriptions

- `eventId` - Unique identifier for this event (UUID v4)
- `eventType` - Type of event (see Event Types above)
- `eventTime` - ISO 8601 timestamp when event occurred
- `bucket` - Bucket name where event happened
- `key` - Object key (if applicable, omitted for bucket-level events)
- `size` - Object size in bytes (if applicable)
- `etag` - Object ETag (if applicable)
- `metadata` - Additional event-specific metadata (optional)

## Filtering

### Filter by Bucket

Only receive events for a specific bucket:

```
ws://localhost:9000/events/stream?bucket=my-bucket
```

### Filter by Prefix

Only receive events for objects matching a prefix:

```
ws://localhost:9000/events/stream?prefix=logs/2024/
```

### Filter by Event Types

Subscribe to specific event types only:

```
ws://localhost:9000/events/stream?events=object-created,object-removed
```

### Combine Filters

All filters can be combined:

```
ws://localhost:9000/events/stream?bucket=my-bucket&prefix=data/&events=object-created
```

## Performance Considerations

- The event broadcaster uses Tokio broadcast channels with a buffer of 1000 events
- If a client cannot keep up with event rate, older events may be dropped
- Multiple concurrent WebSocket connections are supported
- Events are sent in real-time as they occur
- Connection health is maintained via ping/pong frames

## Implementation Notes

- Events are broadcast to all connected clients simultaneously
- No events are stored or replayed - clients only receive events that occur after connection
- WebSocket connections automatically handle ping/pong for keepalive
- Clients should implement reconnection logic with exponential backoff for production use
- Welcome message is sent immediately upon connection to confirm filters

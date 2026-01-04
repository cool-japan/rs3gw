//! AWS Signature V4 Chunked Transfer Encoding
//!
//! Implements aws-chunked content encoding for streaming uploads with
//! per-chunk signatures. This allows verifying large uploads without
//! buffering the entire body in memory.
//!
//! ## Format
//!
//! Each chunk has the format:
//! ```text
//! hex_size;chunk-signature=signature\r\n
//! data\r\n
//! ```
//!
//! The final chunk is:
//! ```text
//! 0;chunk-signature=signature\r\n
//! \r\n
//! ```

use bytes::{Bytes, BytesMut};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tokio::io::AsyncRead;

type HmacSha256 = Hmac<Sha256>;

/// Empty SHA256 hash (for empty chunk data)
pub const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Chunked signature errors
#[derive(Debug, Error)]
pub enum ChunkedError {
    #[error("Invalid chunk format")]
    InvalidFormat,

    #[error("Invalid chunk signature")]
    InvalidSignature,

    #[error("Unexpected end of stream")]
    UnexpectedEof,

    #[error("Chunk size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Context for verifying chunked signatures
#[derive(Debug, Clone)]
pub struct ChunkedSignatureContext {
    /// Signing key (derived from secret key, date, region, service)
    signing_key: Vec<u8>,
    /// Timestamp (YYYYMMDDTHHMMSSZ)
    timestamp: String,
    /// Credential scope (date/region/service/aws4_request)
    credential_scope: String,
    /// Previous chunk signature (starts with seed signature from request)
    previous_signature: String,
}

impl ChunkedSignatureContext {
    /// Create a new chunked signature context
    pub fn new(
        secret_key: &str,
        date: &str,
        timestamp: &str,
        region: &str,
        seed_signature: &str,
    ) -> Self {
        let signing_key = Self::derive_signing_key(secret_key, date, region, "s3");
        let credential_scope = format!("{}/{}/s3/aws4_request", date, region);

        Self {
            signing_key,
            timestamp: timestamp.to_string(),
            credential_scope,
            previous_signature: seed_signature.to_string(),
        }
    }

    /// Derive the signing key
    fn derive_signing_key(secret_key: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
        let k_secret = format!("AWS4{}", secret_key);
        let k_date = Self::hmac_sha256(k_secret.as_bytes(), date.as_bytes());
        let k_region = Self::hmac_sha256(&k_date, region.as_bytes());
        let k_service = Self::hmac_sha256(&k_region, service.as_bytes());
        Self::hmac_sha256(&k_service, b"aws4_request")
    }

    /// Verify a chunk signature
    pub fn verify_chunk(
        &mut self,
        chunk_data: &[u8],
        provided_signature: &str,
    ) -> Result<(), ChunkedError> {
        let expected_signature = self.calculate_chunk_signature(chunk_data);

        if expected_signature != provided_signature {
            return Err(ChunkedError::InvalidSignature);
        }

        // Update previous signature for next chunk
        self.previous_signature = provided_signature.to_string();
        Ok(())
    }

    /// Calculate the signature for a chunk
    pub fn calculate_chunk_signature(&self, chunk_data: &[u8]) -> String {
        // Hash the chunk data
        let chunk_hash = {
            let mut hasher = Sha256::new();
            hasher.update(chunk_data);
            hex::encode(hasher.finalize())
        };

        // Build string to sign for chunk
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256-PAYLOAD\n{}\n{}\n{}\n{}\n{}",
            self.timestamp,
            self.credential_scope,
            self.previous_signature,
            EMPTY_SHA256,
            chunk_hash
        );

        // Calculate signature
        let signature = Self::hmac_sha256(&self.signing_key, string_to_sign.as_bytes());
        hex::encode(signature)
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        // HMAC-SHA256 accepts any key size; this should never fail
        let mut mac = match HmacSha256::new_from_slice(key) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("HMAC initialization failed: {}", e);
                // Return empty vector to cause auth failure rather than panic
                return vec![];
            }
        };
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

/// Parse a chunk header line
/// Format: `hex_size;chunk-signature=signature\r\n`
pub fn parse_chunk_header(line: &str) -> Result<(usize, String), ChunkedError> {
    let parts: Vec<&str> = line.split(';').collect();
    if parts.len() != 2 {
        return Err(ChunkedError::InvalidFormat);
    }

    // Parse hex size
    let size =
        usize::from_str_radix(parts[0].trim(), 16).map_err(|_| ChunkedError::InvalidFormat)?;

    // Parse signature
    let sig_part = parts[1].trim();
    let signature = sig_part
        .strip_prefix("chunk-signature=")
        .ok_or(ChunkedError::InvalidFormat)?
        .to_string();

    Ok((size, signature))
}

/// Decoder state machine for chunked transfer
#[derive(Debug)]
enum DecoderState {
    /// Waiting for chunk header
    ReadingHeader,
    /// Reading chunk data
    ReadingData { remaining: usize, signature: String },
    /// Reading CRLF after chunk data
    ReadingTrailer,
    /// Stream finished
    Finished,
}

/// Async reader that decodes aws-chunked content and verifies signatures
pub struct ChunkedDecoder<R> {
    inner: R,
    state: DecoderState,
    context: ChunkedSignatureContext,
    buffer: BytesMut,
    chunk_buffer: BytesMut,
}

impl<R: AsyncRead + Unpin> ChunkedDecoder<R> {
    /// Create a new chunked decoder
    pub fn new(reader: R, context: ChunkedSignatureContext) -> Self {
        Self {
            inner: reader,
            state: DecoderState::ReadingHeader,
            context,
            buffer: BytesMut::with_capacity(8192),
            chunk_buffer: BytesMut::with_capacity(65536),
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ChunkedDecoder<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            match &self.state {
                DecoderState::Finished => {
                    return Poll::Ready(Ok(()));
                }
                DecoderState::ReadingHeader => {
                    // Try to read a complete header line
                    let mut temp_buf = [0u8; 256];
                    let mut temp_read_buf = tokio::io::ReadBuf::new(&mut temp_buf);

                    match Pin::new(&mut self.inner).poll_read(cx, &mut temp_read_buf) {
                        Poll::Ready(Ok(())) => {
                            if temp_read_buf.filled().is_empty() {
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "Unexpected end of chunked stream",
                                )));
                            }
                            self.buffer.extend_from_slice(temp_read_buf.filled());
                        }
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }

                    // Check for complete header line
                    if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
                        let line = String::from_utf8_lossy(&self.buffer[..pos])
                            .trim()
                            .to_string();
                        self.buffer = self.buffer.split_off(pos + 1);

                        match parse_chunk_header(&line) {
                            Ok((size, signature)) => {
                                if size == 0 {
                                    // Final chunk - verify empty signature
                                    if let Err(e) = self.context.verify_chunk(&[], &signature) {
                                        return Poll::Ready(Err(std::io::Error::new(
                                            std::io::ErrorKind::InvalidData,
                                            e.to_string(),
                                        )));
                                    }
                                    self.state = DecoderState::Finished;
                                } else {
                                    self.chunk_buffer.clear();
                                    self.state = DecoderState::ReadingData {
                                        remaining: size,
                                        signature,
                                    };
                                }
                            }
                            Err(e) => {
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    e.to_string(),
                                )));
                            }
                        }
                    }
                }
                DecoderState::ReadingData {
                    remaining,
                    signature,
                } => {
                    let remaining = *remaining;
                    let signature = signature.clone();

                    // Read from buffer first
                    let to_take = remaining.min(self.buffer.len());
                    if to_take > 0 {
                        let data = self.buffer.split_to(to_take);
                        self.chunk_buffer.extend_from_slice(&data);

                        let new_remaining = remaining - to_take;
                        if new_remaining == 0 {
                            // Verify chunk signature - clone buffer to avoid borrow conflict
                            let chunk_data = self.chunk_buffer.to_vec();
                            if let Err(e) = self.context.verify_chunk(&chunk_data, &signature) {
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    e.to_string(),
                                )));
                            }

                            // Copy to output buffer
                            let to_copy = self.chunk_buffer.len().min(buf.remaining());
                            buf.put_slice(&self.chunk_buffer[..to_copy]);
                            self.chunk_buffer = self.chunk_buffer.split_off(to_copy);

                            self.state = DecoderState::ReadingTrailer;
                            if to_copy > 0 {
                                return Poll::Ready(Ok(()));
                            }
                        } else {
                            self.state = DecoderState::ReadingData {
                                remaining: new_remaining,
                                signature,
                            };
                        }
                        continue;
                    }

                    // Need to read more data
                    let mut temp_buf = vec![0u8; remaining.min(8192)];
                    let mut temp_read_buf = tokio::io::ReadBuf::new(&mut temp_buf);

                    match Pin::new(&mut self.inner).poll_read(cx, &mut temp_read_buf) {
                        Poll::Ready(Ok(())) => {
                            if temp_read_buf.filled().is_empty() {
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "Unexpected end of chunk data",
                                )));
                            }
                            self.buffer.extend_from_slice(temp_read_buf.filled());
                        }
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                DecoderState::ReadingTrailer => {
                    // Read the trailing CRLF after chunk data
                    if self.buffer.len() >= 2 {
                        if &self.buffer[..2] == b"\r\n" {
                            self.buffer = self.buffer.split_off(2);
                        }
                        self.state = DecoderState::ReadingHeader;
                        continue;
                    }

                    let mut temp_buf = [0u8; 2];
                    let mut temp_read_buf = tokio::io::ReadBuf::new(&mut temp_buf);

                    match Pin::new(&mut self.inner).poll_read(cx, &mut temp_read_buf) {
                        Poll::Ready(Ok(())) => {
                            self.buffer.extend_from_slice(temp_read_buf.filled());
                        }
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }
                }
            }
        }
    }
}

/// Encode data as aws-chunked format with signatures
pub fn encode_chunk(data: &[u8], context: &mut ChunkedSignatureContext) -> Bytes {
    let signature = context.calculate_chunk_signature(data);
    context.previous_signature = signature.clone();

    let header = format!("{:x};chunk-signature={}\r\n", data.len(), signature);

    let mut result = BytesMut::with_capacity(header.len() + data.len() + 2);
    result.extend_from_slice(header.as_bytes());
    result.extend_from_slice(data);
    result.extend_from_slice(b"\r\n");

    result.freeze()
}

/// Encode the final (empty) chunk
pub fn encode_final_chunk(context: &mut ChunkedSignatureContext) -> Bytes {
    let signature = context.calculate_chunk_signature(&[]);
    context.previous_signature = signature.clone();

    let trailer = format!("0;chunk-signature={}\r\n\r\n", signature);
    Bytes::from(trailer)
}

/// Check if a request uses aws-chunked encoding
pub fn is_aws_chunked(content_encoding: Option<&str>) -> bool {
    content_encoding
        .map(|ce| ce.contains("aws-chunked"))
        .unwrap_or(false)
}

/// Extract the decoded content length from x-amz-decoded-content-length header
pub fn get_decoded_content_length(headers: &[(String, String)]) -> Option<u64> {
    headers
        .iter()
        .find(|(name, _)| name.to_lowercase() == "x-amz-decoded-content-length")
        .and_then(|(_, value)| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chunk_header() {
        let header = "1000;chunk-signature=abcd1234";
        let (size, sig) = parse_chunk_header(header).expect("Failed to parse chunk header");
        assert_eq!(size, 0x1000);
        assert_eq!(sig, "abcd1234");
    }

    #[test]
    fn test_parse_final_chunk_header() {
        let header = "0;chunk-signature=finalsig";
        let (size, sig) = parse_chunk_header(header).expect("Failed to parse final chunk header");
        assert_eq!(size, 0);
        assert_eq!(sig, "finalsig");
    }

    #[test]
    fn test_parse_invalid_header() {
        assert!(parse_chunk_header("invalid").is_err());
        assert!(parse_chunk_header("100").is_err());
        assert!(parse_chunk_header("100;wrong=sig").is_err());
    }

    #[test]
    fn test_chunk_signature_context() {
        let mut ctx = ChunkedSignatureContext::new(
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "20130524",
            "20130524T000000Z",
            "us-east-1",
            "seed_signature",
        );

        // Calculate signature for a chunk
        let data = b"Hello, World!";
        let sig1 = ctx.calculate_chunk_signature(data);

        // Signature should be 64 hex characters
        assert_eq!(sig1.len(), 64);

        // Verify the chunk updates previous signature
        ctx.previous_signature = sig1.clone();
        let sig2 = ctx.calculate_chunk_signature(data);

        // Different previous signature should produce different result
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_encode_chunk() {
        let mut ctx = ChunkedSignatureContext::new(
            "secret",
            "20230101",
            "20230101T000000Z",
            "us-east-1",
            "seed",
        );

        let data = b"test data";
        let encoded = encode_chunk(data, &mut ctx);

        // Should contain hex size, signature, data, and CRLF
        let encoded_str = String::from_utf8_lossy(&encoded);
        assert!(encoded_str.contains(";chunk-signature="));
        assert!(encoded_str.ends_with("\r\n"));
        assert!(encoded_str.contains("test data"));
    }

    #[test]
    fn test_encode_final_chunk() {
        let mut ctx = ChunkedSignatureContext::new(
            "secret",
            "20230101",
            "20230101T000000Z",
            "us-east-1",
            "seed",
        );

        let final_chunk = encode_final_chunk(&mut ctx);
        let encoded_str = String::from_utf8_lossy(&final_chunk);

        assert!(encoded_str.starts_with("0;chunk-signature="));
        assert!(encoded_str.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_is_aws_chunked() {
        assert!(is_aws_chunked(Some("aws-chunked")));
        assert!(is_aws_chunked(Some("gzip, aws-chunked")));
        assert!(!is_aws_chunked(Some("gzip")));
        assert!(!is_aws_chunked(None));
    }

    #[test]
    fn test_get_decoded_content_length() {
        let headers = vec![
            (
                "x-amz-decoded-content-length".to_string(),
                "12345".to_string(),
            ),
            (
                "content-type".to_string(),
                "application/octet-stream".to_string(),
            ),
        ];

        assert_eq!(get_decoded_content_length(&headers), Some(12345));

        let empty_headers: Vec<(String, String)> = vec![];
        assert_eq!(get_decoded_content_length(&empty_headers), None);
    }

    #[test]
    fn test_verify_chunk() {
        let ctx = ChunkedSignatureContext::new(
            "secret",
            "20230101",
            "20230101T000000Z",
            "us-east-1",
            "seed_signature",
        );

        let data = b"chunk data";
        let expected_sig = ctx.calculate_chunk_signature(data);

        // Reset context for verification
        let mut verify_ctx = ChunkedSignatureContext::new(
            "secret",
            "20230101",
            "20230101T000000Z",
            "us-east-1",
            "seed_signature",
        );

        // Should succeed with correct signature
        assert!(verify_ctx.verify_chunk(data, &expected_sig).is_ok());

        // Previous signature should be updated
        assert_eq!(verify_ctx.previous_signature, expected_sig);
    }

    #[test]
    fn test_verify_chunk_invalid_signature() {
        let mut ctx = ChunkedSignatureContext::new(
            "secret",
            "20230101",
            "20230101T000000Z",
            "us-east-1",
            "seed_signature",
        );

        let data = b"chunk data";
        let result = ctx.verify_chunk(data, "invalid_signature");

        assert!(matches!(result, Err(ChunkedError::InvalidSignature)));
    }
}

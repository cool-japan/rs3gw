fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(
            &[
                "proto/s3.proto",
                "proto/bucket.proto",
                "proto/object.proto",
                "proto/multipart.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}

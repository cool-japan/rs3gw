fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pure-Rust proto compilation: protox parses the .protos (no system protoc),
    // producing a FileDescriptorSet that tonic-prost-build turns into Rust.
    let protos = [
        "proto/s3.proto",
        "proto/bucket.proto",
        "proto/object.proto",
        "proto/multipart.proto",
    ];
    let includes = ["proto"];

    // protox does the parsing that protoc previously did (and handles proto3
    // optional natively), so re-declare the rerun triggers prost-build used to
    // emit for each source file to preserve incremental rebuild behavior.
    for proto in protos {
        println!("cargo:rerun-if-changed={proto}");
    }
    for include in includes {
        println!("cargo:rerun-if-changed={include}");
    }

    // Pure Rust: protox::compile returns a prost_types::FileDescriptorSet that
    // unifies with tonic-prost-build's prost-types, so no protoc is invoked.
    let file_descriptor_set = protox::compile(protos, includes)?;

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(file_descriptor_set)?;

    Ok(())
}

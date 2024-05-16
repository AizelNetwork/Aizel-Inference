fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/infernece.proto")?;
    tonic_build::compile_protos("proto/gate_grpc.proto")?;
    Ok(())
}

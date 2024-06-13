fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_files = ["proto/infernece.proto", "proto/gate.proto"];
    let includes = ["proto"];
    tonic_build::configure().compile(&proto_files, &includes)?;
    // tonic_build::compile_protos("proto/infernece.proto")?;
    // tonic_build::compile_protos("proto/gate.proto")?;
    Ok(())
}

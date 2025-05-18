fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Generating gRPC code...");
    
    tonic_build::compile_protos("proto/geyser.proto")?;
    
    Ok(())
}

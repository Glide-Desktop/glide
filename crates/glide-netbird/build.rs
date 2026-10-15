fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/daemon.proto");
    let descriptors = protox::compile(["proto/daemon.proto"], ["proto"])?;
    tonic_prost_build::configure().build_server(false).build_transport(false).compile_fds(descriptors)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false) // Rust hanya sebagai client
        .compile(
            &["../proto/financial.proto"], // Path ke file proto
            &["../proto"],                 // Include path
        )?;
    Ok(())
}
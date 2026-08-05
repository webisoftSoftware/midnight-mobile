fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let request = midnight_mobile_runtime::deterministic_zswap_output_request()?;
    std::io::stdout().write_all(&request)?;
    Ok(())
}

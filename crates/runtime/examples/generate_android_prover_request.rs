#[cfg(feature = "local-prover")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let request = midnight_mobile_runtime::deterministic_zswap_spend_request()?;
    std::io::stdout().write_all(&request)?;
    Ok(())
}

#[cfg(not(feature = "local-prover"))]
fn main() {
    eprintln!("enable the local-prover feature");
    std::process::exit(2);
}

#[cfg(feature = "android-prover-spike")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let request = midnight_native_runtime::deterministic_zswap_spend_request()?;
    std::io::stdout().write_all(&request)?;
    Ok(())
}

#[cfg(not(feature = "android-prover-spike"))]
fn main() {
    eprintln!("enable the android-prover-spike feature");
    std::process::exit(2);
}

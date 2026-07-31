#[cfg(feature = "local-prover")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;

    use midnight_serialize::tagged_deserialize;

    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    let result: Vec<Option<u64>> = tagged_deserialize(&mut &bytes[..])?;
    if result.is_empty() {
        return Err("check response contains no public-input positions".into());
    }
    Ok(())
}

#[cfg(not(feature = "local-prover"))]
fn main() {
    eprintln!("enable the local-prover feature");
    std::process::exit(2);
}

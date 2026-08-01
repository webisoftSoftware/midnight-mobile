#[cfg(feature = "local-prover")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;

    use midnight_ledger::structure::ProofVersioned;
    use midnight_serialize::tagged_deserialize;

    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    let proof: ProofVersioned = tagged_deserialize(&mut &bytes[..])?;
    if !matches!(proof, ProofVersioned::V2(_)) {
        return Err("proof is not ProofVersioned::V2".into());
    }
    Ok(())
}

#[cfg(not(feature = "local-prover"))]
fn main() {
    eprintln!("enable the local-prover feature");
    std::process::exit(2);
}

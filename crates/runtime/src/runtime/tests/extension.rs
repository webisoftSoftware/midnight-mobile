use super::hardening::*;
use super::*;
use crate::extension::*;

struct Proofs(Vec<RemoteProofRequest>);

struct TestExtension;

impl Extension for TestExtension {
    fn begin(
        &self,
        _ctx: &mut SessionContext<'_>,
        kind: &str,
        _json: &str,
    ) -> Option<Result<ExtensionStep, MidnightRuntimeError>> {
        let step = |op: ExtensionOp, effects| Some(Ok(ExtensionStep { op, effects }));
        match kind {
            "testEcho" => step(
                Box::new(()),
                vec![ExtensionEffect {
                    effect: "query",
                    endpoint_role: "indexer",
                    body_base64: encode_base64(b"hi"),
                }],
            ),
            "testProofs" => {
                let requests = (0..2)
                    .map(|i| RemoteProofRequest {
                        key: format!("k{i}"),
                        kind: RemoteProofKind::Prove,
                        body: vec![i],
                    })
                    .collect::<Vec<_>>();
                let effects = proof_effects(&requests);
                step(Box::new(Proofs(requests)), effects)
            }
            "testFail" => Some(Err(MidnightRuntimeError::InvalidArgument)),
            _ => None,
        }
    }

    fn resume(
        &self,
        _ctx: &mut SessionContext<'_>,
        op: ExtensionOp,
        effect_id: &str,
        results: &[NetworkResult],
    ) -> Result<ExtensionOutcome, MidnightRuntimeError> {
        if let Some(Proofs(pending)) = op.downcast_ref::<Proofs>() {
            let bodies = decode_proof_batch(results, pending, effect_id)?;
            return Ok(ExtensionOutcome::Complete(format!(
                "{{\"bodies\":{}}}",
                bodies.len()
            )));
        }
        if results[0].outcome != "accepted" {
            return Ok(ExtensionOutcome::Retry(
                op,
                MidnightRuntimeError::InvalidArgument,
            ));
        }
        // First round asks for a second network step, the second completes.
        if results[0].body_base64.is_none() {
            return Ok(ExtensionOutcome::Continue(ExtensionStep {
                op,
                effects: vec![ExtensionEffect {
                    effect: "query",
                    endpoint_role: "indexer",
                    body_base64: encode_base64(b"again"),
                }],
            }));
        }
        Ok(ExtensionOutcome::Complete("{\"done\":true}".to_owned()))
    }

    fn pending_requests<'a>(
        &self,
        op: &'a (dyn std::any::Any + Send),
    ) -> Option<&'a [RemoteProofRequest]> {
        op.downcast_ref::<Proofs>()
            .map(|proofs| proofs.0.as_slice())
    }
}

fn install() {
    set_extension(TestExtension);
}

fn resume(
    step: &serde_json::Value,
    results: serde_json::Value,
) -> Result<serde_json::Value, MidnightRuntimeError> {
    let operation = &step["operation"];
    resume_operation(
        operation["id"].as_u64().unwrap(),
        operation["generation"].as_u64().unwrap(),
        Some(results.to_string()),
    )
    .map(|value| serde_json::from_str(&value).unwrap())
}

fn accepted(effect_id: &serde_json::Value, body: Option<&str>) -> serde_json::Value {
    serde_json::json!({"effectId": effect_id, "outcome": "accepted", "bodyBase64": body})
}

#[test]
fn extension_flow_continues_retries_and_completes() {
    let _runtime = isolated_runtime();
    install();
    let handle = open_test_session("ext-flow", None);
    let step = run_command(&handle, serde_json::json!({"kind": "testEcho", "extra": 1})).unwrap();
    assert_eq!(step["kind"], "network");
    assert_eq!(step["endpointRole"], "indexer");
    let bad = serde_json::json!({"effectId": step["effectId"], "outcome": "rejected"});
    assert_runtime_error(resume(&step, bad), "INVALID_ARGUMENT");
    // Retry left the operation in place; the first accepted result continues it.
    let next = resume(&step, accepted(&step["effectId"], None)).unwrap();
    assert_eq!(next["kind"], "network");
    assert_ne!(next["effectId"], step["effectId"]);
    let done = resume(&next, accepted(&next["effectId"], Some("AA=="))).unwrap();
    assert_eq!(done["kind"], "complete");
    assert_eq!(done["result"]["done"], true);
    run_command(&handle, serde_json::json!({"kind": "testEcho"})).unwrap();
}

#[test]
fn extension_proof_batch_resumes_together() {
    let _runtime = isolated_runtime();
    install();
    let handle = open_test_session("ext-proofs", None);
    let step = run_command(&handle, serde_json::json!({"kind": "testProofs"})).unwrap();
    let effects = step["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 2);
    // A one-result payload for a two-request round is refused and keeps the operation.
    let single = accepted(&effects[0]["effectId"], Some("AA=="));
    assert_runtime_error(resume(&step, single.clone()), "INVALID_ARGUMENT");
    let batch = effects
        .iter()
        .map(|effect| accepted(&effect["effectId"], Some("AA==")))
        .collect::<Vec<_>>();
    let done = resume(&step, serde_json::Value::Array(batch)).unwrap();
    assert_eq!(done["result"]["bodies"], 2);
}

#[test]
fn unknown_kind_is_unsupported_and_extension_errors_keep_the_session() {
    let _runtime = isolated_runtime();
    install();
    let handle = open_test_session("ext-errors", None);
    assert_runtime_error(
        run_command(&handle, serde_json::json!({"kind": "nobody"})),
        "UNSUPPORTED_COMMAND",
    );
    assert_runtime_error(
        run_command(&handle, serde_json::json!({"kind": "testFail"})),
        "INVALID_ARGUMENT",
    );
    assert!(run_command(&handle, serde_json::json!({"kind": "testEcho"})).is_ok());
}

#[test]
fn sdk_kinds_match_every_runtime_command_variant() {
    // serde lists every expected variant in its unknown-variant error.
    let error = serde_json::from_str::<RuntimeCommand>(r#"{"kind":"_"}"#)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    let mut variants: Vec<&str> = error.split('`').skip(3).step_by(2).collect();
    let mut kinds = super::super::extension::SDK_KINDS.to_vec();
    variants.sort_unstable();
    kinds.sort_unstable();
    assert_eq!(variants, kinds, "{error}");
}

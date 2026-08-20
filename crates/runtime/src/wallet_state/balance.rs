use super::*;

mod dust;
mod imbalance;
mod manifest;
mod shielded;
mod unshielded;

pub(crate) use manifest::{BalanceManifest, DustEstimate, WalletContribution};

pub(super) type UnsealedTransaction =
    Transaction<Signature, ProofMarker, PedersenRandomness, InMemoryDB>;
pub(super) type UnsealedIntent = Intent<Signature, ProofMarker, PedersenRandomness, InMemoryDB>;
pub(super) type UnprovenIntent =
    Intent<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>;
pub(super) type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>;
pub(super) type ErasedTransaction = Transaction<Signature, (), Pedersen, InMemoryDB>;

const SHIELDED: &str = "shielded";
const UNSHIELDED: &str = "unshielded";

/// Where the transaction's fee comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FeeMode {
    /// The wallet spends its own DUST.
    LocalDust,
    /// The fee is settled outside this transaction, so no DUST is selected and
    /// the returned transaction is token-balanced only.
    Sponsored,
}

pub(crate) struct BalanceRequest<'a> {
    pub(crate) network_id: &'a str,
    pub(crate) original_raw: &'a [u8],
    pub(crate) sealed: bool,
    pub(crate) ledger_parameters: &'a LedgerParameters,
    pub(crate) fee_blocks_margin: usize,
    pub(crate) additional_fee_overhead: u128,
    pub(crate) fee_mode: FeeMode,
    pub(crate) night_external_key: &'a [u8],
    pub(crate) zswap_seed: &'a [u8],
    pub(crate) dust_seed: &'a [u8],
    pub(crate) current_time_seconds: u64,
    pub(crate) ttl_seconds: u64,
}

#[derive(Debug)]
pub(crate) struct BalancePlan {
    /// The wallet state after the plan's inputs are reserved. A balance-only
    /// preparation discards this so a preview never reserves a coin.
    pub(crate) proposed: NativeWalletState,
    /// The transaction the balancing counterpart is merged into. `None` keeps
    /// the caller's bytes exactly as supplied.
    pub(crate) base_raw: Option<Vec<u8>>,
    /// The wallet's own counterpart, still to be proven. `None` means the
    /// transaction was already balanced and only needs sealing.
    pub(crate) balancing_raw: Option<Vec<u8>>,
    pub(crate) manifest: BalanceManifest,
}

/// A token-keyed accumulator for the figures shown on the approval screen.
#[derive(Default)]
struct Figures {
    contributions: BTreeMap<(&'static str, String), u128>,
    change: BTreeMap<(&'static str, String), u128>,
}

impl Figures {
    fn add(
        target: &mut BTreeMap<(&'static str, String), u128>,
        wallet_type: &'static str,
        token: String,
        amount: u128,
    ) {
        let entry = target.entry((wallet_type, token)).or_default();
        *entry = entry.saturating_add(amount);
    }

    fn absorb_shielded(
        &mut self,
        plan: &shielded::ShieldedPlan,
    ) -> Result<(), MidnightRuntimeError> {
        for (token, amount) in &plan.contributions {
            Self::add(
                &mut self.contributions,
                SHIELDED,
                hex::encode(token.0.0),
                *amount,
            );
        }
        for (token, amount) in &plan.change {
            Self::add(&mut self.change, SHIELDED, hex::encode(token.0.0), *amount);
        }
        Ok(())
    }

    fn absorb_unshielded(&mut self, plan: &unshielded::UnshieldedPlan) {
        for (token, amount) in &plan.contributions {
            Self::add(
                &mut self.contributions,
                UNSHIELDED,
                hex::encode(token.0.0),
                *amount,
            );
        }
        for (token, amount) in &plan.change {
            Self::add(
                &mut self.change,
                UNSHIELDED,
                hex::encode(token.0.0),
                *amount,
            );
        }
    }

    fn into_lists(self) -> (Vec<WalletContribution>, Vec<WalletContribution>) {
        let convert = |values: BTreeMap<(&'static str, String), u128>| {
            values
                .into_iter()
                .map(|((wallet_type, token_type), amount)| WalletContribution {
                    wallet_type: wallet_type.to_owned(),
                    token_type,
                    amount: amount.to_string(),
                })
                .collect::<Vec<_>>()
        };
        (convert(self.contributions), convert(self.change))
    }
}

/// Fingerprints the parts of the wallet state a plan actually depends on: which
/// coins exist, not what they are currently worth. DUST values change with every
/// clock tick, so hashing them would make every approval stale on arrival, while
/// hashing the coin identities still catches a coin spent elsewhere.
fn state_digest(state: &NativeWalletState) -> Result<String, MidnightRuntimeError> {
    let mut entries = Vec::new();
    for coin in &state.unshielded.available_utxos {
        entries.push(format!(
            "u:{}:{}:{}:{}",
            coin.utxo.intent_hash, coin.utxo.output_no, coin.utxo.type_, coin.utxo.value
        ));
    }
    for (nullifier, coin) in state.shielded.coins.iter() {
        entries.push(format!(
            "s:{}:{}:{}",
            serializable_hex(&nullifier)?,
            hex::encode(coin.type_.0.0),
            coin.value
        ));
    }
    for coin in state.dust.utxos() {
        entries.push(format!("d:{}", hex::encode(coin.nonce.as_le_bytes())));
    }
    entries.sort();
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update((entry.len() as u64).to_le_bytes());
        hasher.update(entry.as_bytes());
    }
    Ok(hex::encode(hasher.finalize()))
}

fn free_segments(used: &BTreeSet<u16>, count: usize) -> Result<Vec<u16>, MidnightRuntimeError> {
    let mut taken = used.clone();
    let mut allocated = Vec::with_capacity(count);
    for _ in 0..count {
        let segment = (1_u16..=u16::MAX)
            .find(|candidate| !taken.contains(candidate))
            .ok_or(MidnightRuntimeError::UnsupportedTransaction)?;
        taken.insert(segment);
        allocated.push(segment);
    }
    Ok(allocated)
}

fn standard(
    transaction: &ErasedTransaction,
) -> Result<&StandardTransaction<Signature, (), Pedersen, InMemoryDB>, MidnightRuntimeError> {
    match transaction {
        Transaction::Standard(standard) => Ok(standard),
        // A rewards claim carries no dApp intent to balance.
        Transaction::ClaimRewards(_) => Err(MidnightRuntimeError::UnsupportedTransaction),
    }
}

impl NativeWalletState {
    /// Plans everything the wallet contributes to a dApp transaction, without
    /// touching the live wallet state.
    pub(crate) fn plan_balance(
        &self,
        request: BalanceRequest<'_>,
    ) -> Result<BalancePlan, MidnightRuntimeError> {
        self.plan_balance_with_rng(request, &mut OsRng)
    }

    pub(crate) fn plan_balance_with_rng<R: Rng + CryptoRng>(
        &self,
        request: BalanceRequest<'_>,
        rng: &mut R,
    ) -> Result<BalancePlan, MidnightRuntimeError> {
        if request.network_id.is_empty() || request.ttl_seconds <= request.current_time_seconds {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let original_erased = crate::transaction::decode_balance_original(
            request.original_raw,
            request.sealed,
            request.network_id,
        )?;
        let transaction_digest = crate::transaction::canonical_digest(request.original_raw);
        let wallet_state_digest = state_digest(self)?;
        let imbalances = imbalance::read_imbalances(&original_erased)?;
        let base_segments = standard(&original_erased)
            .map(|standard| standard.intents.keys().collect::<BTreeSet<_>>())?;

        let signing_key = SigningKey::from_bytes(request.night_external_key)
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let verifying_key = signing_key.verifying_key();
        let owner = UserAddress::from(verifying_key.clone());
        let mut zswap_seed: [u8; 32] = request
            .zswap_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let zswap_keys = ZswapSecretKeys::from(ZswapSeed::from(zswap_seed));
        zswap_seed.zeroize();

        let mut proposed = self.clone();
        let mut figures = Figures::default();

        // Shielded counteroffers, one per imbalanced segment.
        let mut guaranteed_coins = None;
        let mut fallible_coins: BTreeMap<u16, ZswapOffer<ProofPreimage, InMemoryDB>> =
            BTreeMap::new();
        let shielded_segments = imbalances
            .shielded
            .values()
            .flatten()
            .map(|entry| entry.segment)
            .collect::<BTreeSet<_>>();
        for segment in shielded_segments {
            let targets = imbalances
                .shielded
                .iter()
                .filter_map(|(token, entries)| {
                    entries
                        .iter()
                        .find(|entry| entry.segment == segment)
                        .map(|entry| (*token, entry.amount))
                })
                .collect::<BTreeMap<_, _>>();
            let plan = shielded::plan_segment(&mut proposed, &zswap_keys, segment, &targets, rng)?;
            figures.absorb_shielded(&plan)?;
            if let Some(offer) = plan.offer {
                if segment == 0 {
                    guaranteed_coins = Some(offer);
                } else {
                    fallible_coins.insert(segment, offer);
                }
            }
        }

        // Unshielded balancing: rewritten in place when the transaction is
        // still unsealed, added as a separate intent when it is not.
        let mut spent = Vec::new();
        let mut balancing_intents: BTreeMap<u16, UnprovenIntent> = BTreeMap::new();
        let mut base_raw = None;
        let unshielded_segments = imbalances.unshielded_segments();
        if request.sealed {
            for segment in &unshielded_segments {
                // A bound intent's fallible section cannot be balanced from
                // outside that intent, and the intent can no longer be edited.
                if *segment != 0 {
                    return Err(MidnightRuntimeError::UnsupportedTransaction);
                }
            }
            if unshielded_segments.contains(&0) {
                let plan = unshielded::plan_segment(
                    self,
                    &verifying_key,
                    owner,
                    &imbalances.unshielded_for(0),
                    &spent,
                )?;
                figures.absorb_unshielded(&plan);
                spent.extend(plan.spent.iter().cloned());
                if let Some(intent) = unshielded::balancing_intent(&plan, request.ttl_seconds, rng)?
                {
                    let segment = free_segments(&base_segments, 1)?[0];
                    balancing_intents.insert(
                        segment,
                        unshielded::sign_balancing_intent(&intent, segment, &signing_key, rng)?,
                    );
                }
            }
        } else if !unshielded_segments.is_empty() {
            let original = crate::transaction::decode_unsealed_original(
                request.original_raw,
                request.network_id,
            )?;
            let Transaction::Standard(mut standard) = original else {
                return Err(MidnightRuntimeError::UnsupportedTransaction);
            };
            let first_intent = standard
                .intents
                .keys()
                .min()
                .ok_or(MidnightRuntimeError::UnsupportedTransaction)?;
            let mut edits: BTreeMap<u16, unshielded::IntentEdit> = BTreeMap::new();
            for segment in &unshielded_segments {
                let target = if *segment == 0 {
                    first_intent
                } else {
                    *segment
                };
                let intent = standard
                    .intents
                    .get(&target)
                    .map(|intent| (*intent).clone())
                    .ok_or(MidnightRuntimeError::UnsupportedTransaction)?;
                let plan = unshielded::plan_segment(
                    self,
                    &verifying_key,
                    owner,
                    &imbalances.unshielded_for(*segment),
                    &spent,
                )?;
                figures.absorb_unshielded(&plan);
                spent.extend(plan.spent.iter().cloned());
                edits
                    .entry(target)
                    .or_default()
                    .stage(&intent, *segment == 0, &plan);
            }
            for (target, edit) in &edits {
                let intent = standard
                    .intents
                    .get(target)
                    .map(|intent| (*intent).clone())
                    .ok_or(MidnightRuntimeError::UnsupportedTransaction)?;
                standard.intents = standard
                    .intents
                    .insert(*target, edit.apply(&intent, *target, &signing_key, rng)?);
            }
            let rewritten: UnsealedTransaction = Transaction::Standard(standard);
            let mut raw = Vec::new();
            tagged_serialize(&rewritten, &mut raw)
                .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            base_raw = Some(raw);
        }

        let token_balancing = (!balancing_intents.is_empty()
            || guaranteed_coins.is_some()
            || !fallible_coins.is_empty())
        .then(|| {
            UnprovenTransaction::new(
                request.network_id.to_owned(),
                balancing_intents.clone().into_iter().collect(),
                guaranteed_coins.clone(),
                fallible_coins.clone().into_iter().collect(),
            )
        });

        let base_erased = match &base_raw {
            Some(raw) => {
                crate::transaction::decode_balance_original(raw, false, request.network_id)?
            }
            None => original_erased,
        };
        let token_balanced = match &token_balancing {
            Some(balancing) => base_erased
                .merge(&balancing.erase_proofs())
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
            None => base_erased,
        };

        let mut used_segments = base_segments;
        used_segments.extend(balancing_intents.keys().copied());
        let (dust_estimate, dust_intent) = match request.fee_mode {
            FeeMode::Sponsored => (DustEstimate::Sponsored, None),
            FeeMode::LocalDust => {
                let segment = free_segments(&used_segments, 1)?[0];
                let balance = dust::plan(
                    &proposed.dust,
                    dust::DustRequest {
                        network_id: request.network_id,
                        base: &token_balanced,
                        ledger_parameters: request.ledger_parameters,
                        fee_blocks_margin: request.fee_blocks_margin,
                        additional_fee_overhead: request.additional_fee_overhead,
                        dust_seed: request.dust_seed,
                        current_time_seconds: request.current_time_seconds,
                        ttl_seconds: request.ttl_seconds,
                        segment,
                    },
                )?;
                proposed.dust = balance.state;
                (
                    DustEstimate::Local(balance.spent),
                    balance.intent.map(|intent| (segment, intent)),
                )
            }
        };

        let dust_balancing = dust_intent.map(|(segment, intent)| {
            UnprovenTransaction::new(
                request.network_id.to_owned(),
                [(segment, intent)].into_iter().collect(),
                None,
                LedgerHashMap::new(),
            )
        });
        // `token_balanced` already carries the token counteroffers, so only the
        // DUST leg is still missing from the merged view.
        let final_erased = match &dust_balancing {
            Some(dust) => token_balanced
                .merge(&dust.erase_proofs())
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
            None => token_balanced,
        };
        let balancing = match (token_balancing, dust_balancing) {
            (Some(tokens), Some(dust)) => Some(
                tokens
                    .merge(&dust)
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
            ),
            (Some(tokens), None) => Some(tokens),
            (None, Some(dust)) => Some(dust),
            (None, None) => None,
        };
        verify_balanced(&final_erased, &request, &dust_estimate)?;

        unshielded::reserve(&mut proposed, &spent);
        let balancing_raw = match balancing {
            Some(balancing) => {
                let mut raw = Vec::new();
                tagged_serialize(&balancing, &mut raw)
                    .map_err(|_| MidnightRuntimeError::NativeInternal)?;
                Some(raw)
            }
            None => None,
        };
        let (contributions, change) = figures.into_lists();
        Ok(BalancePlan {
            proposed,
            base_raw,
            balancing_raw,
            manifest: BalanceManifest::new(
                transaction_digest,
                request.sealed,
                contributions,
                change,
                &dust_estimate,
                wallet_state_digest,
            ),
        })
    }
}

/// Refuses to hand back a plan that does not actually balance. Under fee
/// sponsorship the DUST leg is settled outside this transaction, so only the
/// token legs are required to be zero.
fn verify_balanced(
    transaction: &ErasedTransaction,
    request: &BalanceRequest<'_>,
    dust: &DustEstimate,
) -> Result<(), MidnightRuntimeError> {
    let fees = transaction
        .fees_with_margin(request.ledger_parameters, request.fee_blocks_margin)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?
        .checked_add(request.additional_fee_overhead)
        .ok_or(MidnightRuntimeError::InvalidArgument)?;
    let balances = transaction
        .balance(Some(fees))
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    for ((token, _), value) in &balances {
        if *value == 0 {
            continue;
        }
        if *token == TokenType::Dust && *dust == DustEstimate::Sponsored {
            continue;
        }
        return Err(MidnightRuntimeError::NativeInternal);
    }
    Ok(())
}

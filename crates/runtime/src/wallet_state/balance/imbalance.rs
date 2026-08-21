use super::*;

/// A single token imbalance the wallet has to resolve.
///
/// `amount` is signed with the ledger's own convention: negative means the
/// transaction spends more of the token than it supplies (the wallet must add
/// inputs), positive means it supplies more than it spends (the wallet takes
/// the surplus back as an output).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SegmentImbalance {
    pub(super) segment: u16,
    pub(super) amount: i128,
}

/// Every non-DUST imbalance of a proof-erased transaction, grouped by token.
///
/// DUST is excluded on purpose: it is never balanced with wallet coins, only
/// with DUST spends or fee sponsorship, and it is the one token whose figure
/// depends on the fee of the transaction being assembled.
#[derive(Clone, Debug, Default)]
pub(super) struct Imbalances {
    pub(super) shielded: BTreeMap<ShieldedTokenType, Vec<SegmentImbalance>>,
    pub(super) unshielded: BTreeMap<UnshieldedTokenType, Vec<SegmentImbalance>>,
}

impl Imbalances {
    /// The set of segments carrying an unshielded imbalance, in ascending order.
    pub(super) fn unshielded_segments(&self) -> BTreeSet<u16> {
        self.unshielded
            .values()
            .flatten()
            .map(|entry| entry.segment)
            .collect()
    }

    /// The unshielded imbalances of one segment, keyed by token type.
    pub(super) fn unshielded_for(&self, segment: u16) -> BTreeMap<UnshieldedTokenType, i128> {
        self.unshielded
            .iter()
            .filter_map(|(token, entries)| {
                entries
                    .iter()
                    .find(|entry| entry.segment == segment)
                    .map(|entry| (*token, entry.amount))
            })
            .collect()
    }
}

/// Reads the imbalances of `original` straight out of the ledger's own balance
/// calculation, so the planner can never disagree with the rule that decides
/// whether the final transaction is well formed.
pub(super) fn read_imbalances(
    original: &Transaction<Signature, (), Pedersen, InMemoryDB>,
) -> Result<Imbalances, MidnightRuntimeError> {
    let balances = original
        .balance(None)
        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    let mut imbalances = Imbalances::default();
    for ((token, segment), amount) in balances {
        if amount == 0 {
            continue;
        }
        match token {
            TokenType::Shielded(token) => imbalances
                .shielded
                .entry(token)
                .or_default()
                .push(SegmentImbalance { segment, amount }),
            TokenType::Unshielded(token) => imbalances
                .unshielded
                .entry(token)
                .or_default()
                .push(SegmentImbalance { segment, amount }),
            // DUST is resolved by the fee stage, never by coin selection.
            TokenType::Dust => {}
        }
    }
    Ok(imbalances)
}

use super::*;

/// One net movement of value between the wallet and a dApp transaction.
///
/// `amount` is the *net* figure the user is asked to approve: selected inputs
/// minus the change returned to the wallet. Showing the raw UTXO value would
/// overstate the cost whenever a large coin has to be split.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WalletContribution {
    pub(crate) wallet_type: String,
    pub(crate) token_type: String,
    pub(crate) amount: String,
}

/// The DUST cost the wallet is asked to accept.
///
/// `Local` carries the maximum the wallet may spend; a cheaper execution needs
/// no new approval. `Sponsored` means no wallet DUST is spent at all because
/// the fee is settled by the proof service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DustEstimate {
    Local(u128),
    Sponsored,
}

pub(crate) const SPONSORED_DUST: &str = "sponsored";

impl DustEstimate {
    fn encode(&self) -> String {
        match self {
            Self::Local(value) => value.to_string(),
            Self::Sponsored => SPONSORED_DUST.to_owned(),
        }
    }
}

/// The exact set of facts the user approves, and the only thing execution is
/// allowed to act on. Execution recomputes the plan and refuses to continue
/// unless the fresh manifest is authorized by the approved one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BalanceManifest {
    pub(crate) transaction_digest: String,
    pub(crate) variant: String,
    pub(crate) contributions: Vec<WalletContribution>,
    pub(crate) change: Vec<WalletContribution>,
    pub(crate) dust: String,
    pub(crate) wallet_state_digest: String,
}

fn absorb(hasher: &mut Sha256, label: &str, value: &str) {
    // Length-prefix every field so no two distinct manifests can share a
    // digest by shifting bytes across a field boundary.
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label.as_bytes());
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn absorb_contributions(hasher: &mut Sha256, label: &str, values: &[WalletContribution]) {
    absorb(hasher, label, &values.len().to_string());
    for value in values {
        absorb(hasher, "walletType", &value.wallet_type);
        absorb(hasher, "tokenType", &value.token_type);
        absorb(hasher, "amount", &value.amount);
    }
}

fn contribution_order(value: &WalletContribution) -> (String, String) {
    (value.wallet_type.clone(), value.token_type.clone())
}

impl BalanceManifest {
    pub(crate) fn new(
        transaction_digest: String,
        sealed: bool,
        mut contributions: Vec<WalletContribution>,
        mut change: Vec<WalletContribution>,
        dust: &DustEstimate,
        wallet_state_digest: String,
    ) -> Self {
        contributions.sort_by_key(contribution_order);
        change.sort_by_key(contribution_order);
        Self {
            transaction_digest,
            variant: if sealed { "sealed" } else { "unsealed" }.to_owned(),
            contributions,
            change,
            dust: dust.encode(),
            wallet_state_digest,
        }
    }

    pub(crate) fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        absorb(&mut hasher, "transactionDigest", &self.transaction_digest);
        absorb(&mut hasher, "variant", &self.variant);
        absorb_contributions(&mut hasher, "contributions", &self.contributions);
        absorb_contributions(&mut hasher, "change", &self.change);
        absorb(&mut hasher, "dust", &self.dust);
        absorb(&mut hasher, "walletStateDigest", &self.wallet_state_digest);
        hex::encode(hasher.finalize())
    }

    fn dust_ceiling(&self) -> Result<Option<u128>, MidnightRuntimeError> {
        if self.dust == SPONSORED_DUST {
            return Ok(None);
        }
        self.dust
            .parse::<u128>()
            .map(Some)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)
    }

    /// Fail closed unless `fresh` costs the user no more than this approved
    /// manifest. Every field except the DUST ceiling must match exactly; a
    /// strictly cheaper DUST outcome is allowed, an increase is not.
    pub(crate) fn authorizes(&self, fresh: &Self) -> Result<(), MidnightRuntimeError> {
        if self.transaction_digest != fresh.transaction_digest
            || self.variant != fresh.variant
            || self.contributions != fresh.contributions
            || self.change != fresh.change
            || self.wallet_state_digest != fresh.wallet_state_digest
        {
            return Err(MidnightRuntimeError::BalanceApprovalChanged);
        }
        match (self.dust_ceiling()?, fresh.dust_ceiling()?) {
            (None, None) => Ok(()),
            (Some(approved), Some(actual)) if actual <= approved => Ok(()),
            _ => Err(MidnightRuntimeError::BalanceApprovalChanged),
        }
    }
}

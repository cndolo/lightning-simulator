mod adversaries;
mod deanonymisation;
mod diversity;
mod failures;

use crate::payment::Payment;
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Adversaries {
    pub(crate) selection_strategy: crate::AdversarySelection,
    pub(crate) statistics: Vec<Statistics>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    /// how many adversaries
    pub(crate) number: usize,
    /// Number of times an adversary was included a payment path
    pub hits: usize,
    /// Number of times an adversary was included a successful payment path
    pub hits_successful: usize,
    // independent of the number of adversaries
    pub(crate) anonymity_sets: Vec<AnonymitySet>, // one for each adversary in a payment path (MPP payments are treated like separate payments
    /// Contains the updated sim results when some nodes are removed
    pub targeted_attack: TargetedAttack,
}

/// All the distances in the simulated payments' paths
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct PathDistances(pub Vec<usize>);

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnonymitySet {
    /// Possible senders
    sender: usize,
    /// Possible recipients
    recipient: usize,
    /// True if the recipient is included in the recipient anonymity set
    correct_recipient: bool,
    correct_source: bool,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TargetedAttack {
    pub total_num: usize,
    pub num_successful: usize,
    pub num_failed: usize,
    pub successful_payments: Vec<Payment>,
    pub failed_payments: Vec<Payment>,
    pub path_distances: PathDistances,
}

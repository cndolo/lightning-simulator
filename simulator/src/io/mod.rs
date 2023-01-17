use crate::{
    payment::Payment, traversal::pathfinding::CandidatePath, Adversaries, PathDistances,
    WeightPartsCombi,
};
use serde::Serialize;

pub mod output;
pub use output::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output(Vec<Results>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub(crate) amount: usize,
    pub(crate) total_num: usize,
    pub(crate) num_succesful: usize,
    pub(crate) num_failed: usize,
    pub(crate) payments: Vec<PaymentInfo>,
    pub(crate) adversaries: Vec<Adversaries>,
    pub(crate) path_distances: PathDistances,
}

/// run and reports
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Results {
    pub scenario: WeightPartsCombi,
    pub run: u64,
    pub reports: Vec<Report>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaymentInfo {
    pub(crate) id: usize,
    pub(crate) succeeded: bool,
    /// Number of parts this payment has been split into
    pub(crate) num_parts: usize,
    pub(crate) htlc_attempts: usize,
    pub(crate) used_paths: Vec<PathInfo>,
    pub(crate) failed_paths: Vec<PathInfo>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
/// Describes the path used by amounts - may or may not have failed
pub(crate) struct PathInfo {
    /// The aggregated path fees describing how costly the path is
    pub(crate) total_fees: usize,
    pub(crate) total_time: usize,
    pub(crate) path_len: usize,
}

impl PathInfo {
    pub(super) fn from_payment(paths: &[CandidatePath]) -> Vec<Self> {
        paths
            .iter()
            .filter(|p| !p.path.hops.is_empty())
            .map(|path| Self {
                total_fees: path.path_fees(),
                total_time: path.time,
                path_len: path.path.path_length(),
            })
            .collect()
    }
}

impl PaymentInfo {
    pub(super) fn from_payment(payment: &Payment) -> Self {
        let used_paths = PathInfo::from_payment(&payment.used_paths);
        let failed_paths = PathInfo::from_payment(&payment.failed_paths);
        Self {
            id: payment.payment_id,
            succeeded: payment.succeeded,
            num_parts: payment.num_parts,
            htlc_attempts: payment.htlc_attempts,
            used_paths,
            failed_paths,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traversal::pathfinding::{CandidatePath, Path};
    use std::collections::VecDeque;

    #[test]
    fn payment_info_from_payment() {
        let used_paths = vec![
            CandidatePath {
                path: Path {
                    src: "bob".to_string(),
                    dest: "alice".to_string(),
                    hops: VecDeque::from([
                        ("bob".to_string(), 6010, 5, "bob-carol".to_string()),
                        ("carol".to_string(), 10, 5, "carol-alice".to_string()),
                        ("alice".to_string(), 6000, 0, "alice-carol".to_string()),
                    ]),
                },
                weight: 10.0,
                amount: 2010,
                time: 5,
            },
            CandidatePath {
                path: Path {
                    src: "bob".to_string(),
                    dest: "alice".to_string(),
                    hops: VecDeque::from([
                        ("bob".to_string(), 6030, 10, "bob-eve".to_string()),
                        ("eve".to_string(), 20, 5, "eve-carol".to_string()),
                        ("carol".to_string(), 10, 5, "carol-alice".to_string()),
                        ("alice".to_string(), 6000, 0, "alice-carol".to_string()),
                    ]),
                },
                weight: 30.0,
                amount: 2030,
                time: 10,
            },
        ];
        let source = "bob".to_string();
        let dest = "alice".to_string();
        let amount_msat = 2000;
        let payment = Payment {
            payment_id: 0,
            source: source.clone(),
            dest: dest.clone(),
            amount_msat,
            succeeded: false,
            min_shard_amt: 10,
            htlc_attempts: 2,
            num_parts: 1,
            used_paths,
            failed_amounts: Vec::default(),
            successful_shards: Vec::default(),
            failed_paths: vec![],
        };
        let actual = PaymentInfo::from_payment(&payment);
        let expected = PaymentInfo {
            id: 0,
            num_parts: 1,
            htlc_attempts: 2,
            succeeded: false,
            used_paths: vec![
                PathInfo {
                    total_fees: 10,
                    total_time: 5,
                    path_len: 2,
                },
                PathInfo {
                    total_fees: 30,
                    total_time: 10,
                    path_len: 3,
                },
            ],
            failed_paths: vec![],
        };
        assert_ne!(actual, expected);
    }
}

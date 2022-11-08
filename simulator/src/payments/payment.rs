use crate::{traversal::Path, Node, PaymentId};

#[derive(Debug, Clone)]
pub struct Payment {
    payment_id: PaymentId,
    source: Node,
    dest: Node,
    amt: usize,
    path: Path,
}

impl Eq for Payment {}
impl PartialEq for Payment {
    fn eq(&self, other: &Self) -> bool {
        self.payment_id == other.payment_id
    }
}
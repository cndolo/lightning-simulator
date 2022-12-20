use crate::{
    core_types::graph::Graph, event::*, payment::Payment, sim::SimResult, time::Time, Invoice,
    PaymentId, PaymentParts, RoutingMetric, WeightPartsCombi, ID,
};
use log::{debug, error, info};
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};

pub struct Simulation {
    /// Graph describing LN topology
    pub(crate) graph: Graph,
    /// Payment amount to simulate
    amount: usize,
    /// Sim seed
    run: u64,
    /// Fee minimisation or probability maximisation
    pub(crate) routing_metric: RoutingMetric,
    /// Single or multi-path
    pub(crate) payment_parts: PaymentParts,
    /// Queue of events to be simulated
    pub(crate) event_queue: EventQueue,
    /// Assigned to each new payment
    current_payment_id: PaymentId,
    /// Invoices each node has issued; map of <node, <invoice id, invoice>
    outstanding_invoices: BTreeMap<ID, HashMap<usize, Invoice>>,
    total_num_payments: usize,
    pub(crate) num_successful: usize,
    pub(crate) successful_payments: Vec<Payment>,
    pub(crate) num_failed: usize,
    pub(crate) failed_payments: Vec<Payment>,
}

impl Simulation {
    pub fn new(
        run: u64,
        graph: Graph,
        amount: usize,
        routing_metric: RoutingMetric,
        payment_parts: PaymentParts,
    ) -> Self {
        info!("Initialising simulation...");
        let mut rng = crate::RNG.lock().unwrap();
        *rng = SeedableRng::seed_from_u64(run);
        let event_queue = EventQueue::new();
        let outstanding_invoices: BTreeMap<String, HashMap<usize, Invoice>> = BTreeMap::new();
        let successful_payments = Vec::new();
        Self {
            graph,
            amount,
            run,
            routing_metric,
            payment_parts,
            event_queue,
            current_payment_id: 0,
            outstanding_invoices,
            num_successful: 0,
            successful_payments,
            num_failed: 0,
            failed_payments: Vec::new(),
            total_num_payments: 0,
        }
    }

    pub fn new_batch_simulator(
        run: u64,
        graph: Graph,
        amount: usize,
        weight_parts: WeightPartsCombi,
    ) -> Self {
        let (routing_metric, payment_parts) = match weight_parts {
            WeightPartsCombi::MinFeeSingle => (RoutingMetric::MinFee, PaymentParts::Single),
            WeightPartsCombi::MinFeeMulti => (RoutingMetric::MinFee, PaymentParts::Split),
            WeightPartsCombi::MaxProbSingle => (RoutingMetric::MaxProb, PaymentParts::Single),
            WeightPartsCombi::MaxProbMulti => (RoutingMetric::MaxProb, PaymentParts::Split),
        };
        Self::new(run, graph, amount, routing_metric, payment_parts)
    }

    pub fn run(&mut self, payment_pairs: impl Iterator<Item = (ID, ID)> + Clone) -> SimResult {
        info!(
            "# Payment pairs = {}, Pathfinding weight = {:?}, Single/MMP payments: {:?}",
            payment_pairs.size_hint().0,
            self.routing_metric,
            self.payment_parts
        );
        let mut now = Time::from_secs(0.0); // start simulation at (0)
        for (src, dest) in payment_pairs {
            let payment_id = self.next_payment_id();
            let invoice = Invoice::new(payment_id, self.amount, &src, &dest);
            self.add_invoice(invoice);
            let payment = Payment::new(payment_id, src, dest, self.amount);
            let event = PaymentEvent::Scheduled { payment };
            self.event_queue.schedule(now, event);
            now += Time::from_secs(crate::SIM_DELAY_IN_SECS);
        }
        self.total_num_payments = self.event_queue.queue_length();
        debug!(
            "Queued {} events for simulation.",
            self.event_queue.queue_length()
        );

        info!("Starting simulation.");
        // this is where the actual simulation happens
        while let Some(event) = self.event_queue.next() {
            match event {
                PaymentEvent::Scheduled { mut payment } => {
                    debug!(
                        "Dispatching scheduled payment {} at simulation time = {}.",
                        payment.payment_id,
                        self.event_queue.now()
                    );
                    let _ = match self.payment_parts {
                        PaymentParts::Single => self.send_single_payment(&mut payment),
                        PaymentParts::Split => self.send_mpp_payment(&mut payment),
                    };
                }
                PaymentEvent::UpdateFailed { payment } => {
                    self.num_failed += 1;
                    self.failed_payments.push(payment.to_owned());
                }
                PaymentEvent::UpdateSuccesful { payment } => {
                    self.num_successful += 1;
                    self.successful_payments.push(payment.to_owned());
                }
            }
        }
        assert_eq!(
            self.num_successful + self.num_failed,
            self.total_num_payments,
            "Something went wrong. Expected a different number simulation events."
        );
        info!(
            "Completed simulation after {} simulation secs.",
            now.as_secs(),
        );
        info!(
            "# Total payments = {}, # successful {}, # failed = {}.",
            self.total_num_payments, self.num_successful, self.num_failed
        );
        SimResult {
            run: self.run,
            amount: self.amount,
            total_num: self.total_num_payments,
            num_succesful: self.num_successful,
            num_failed: self.num_failed,
            successful_payments: self.successful_payments.clone(),
            failed_payments: self.failed_payments.clone(),
        }
    }

    // TODO: pair should be made up of distinct nodes
    pub fn draw_n_pairs_for_simulation(
        graph: &Graph,
        n: usize,
    ) -> (impl Iterator<Item = (ID, ID)> + Clone) {
        info!("Drawing {} sender-receiver pairs for simulation.", n,);
        let g = graph.clone();
        g.get_random_pairs_of_nodes(n)
    }

    pub(crate) fn add_invoice(&mut self, invoice: Invoice) {
        // Has this node already issued invoices?
        match self.outstanding_invoices.get_mut(&invoice.destination) {
            Some(node_invoices) => {
                node_invoices.insert(invoice.id, invoice);
            }
            None => {
                let mut node_invoices = HashMap::new();
                node_invoices.insert(invoice.id, invoice.clone());
                self.outstanding_invoices
                    .insert(invoice.destination, node_invoices);
            }
        };
    }

    /// Invoices each node has issued; map of <node, <invoice id, invoice>
    pub(crate) fn get_invoices_for_node(&self, node: &ID) -> Option<&HashMap<usize, Invoice>> {
        match self.outstanding_invoices.get(node) {
            Some(invoices_map) => Some(invoices_map),
            None => None,
        }
    }

    #[allow(unused)]
    pub(crate) fn remove_invoice(&mut self, invoice: &Invoice) {
        let id = invoice.id;
        match self.outstanding_invoices.get_mut(&invoice.destination) {
            Some(invoices_map) => {
                invoices_map.retain(|k, v| *k != id && v.id != id);
                self.outstanding_invoices.retain(|_, v| !v.is_empty());
            }
            None => error!("Requested invoice with id {} not found.", id),
        };
    }

    fn next_payment_id(&mut self) -> usize {
        let current_id = self.current_payment_id;
        self.current_payment_id += 1;
        current_id
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::path::Path;

    #[test]
    fn init_simulator() {
        let seed = 0;
        let amount = 100;
        let path_to_file = Path::new("../test_data/trivial.json");
        let graph = Graph::to_sim_graph(&network_parser::from_json_file(path_to_file).unwrap());
        let routing_metric = RoutingMetric::MinFee;
        let payment_parts = PaymentParts::Single;
        let actual = Simulation::new(seed, graph, amount, routing_metric, payment_parts);
        assert_eq!(actual.amount, amount);
        assert_eq!(actual.run, seed);
    }

    #[test]
    fn get_n_random_node_pairs() {
        let path_to_file = Path::new("../test_data/trivial.json");
        let graph = Graph::to_sim_graph(&network_parser::from_json_file(path_to_file).unwrap());
        let n = 2;
        let actual = Simulation::draw_n_pairs_for_simulation(&graph, n);
        assert_eq!(actual.size_hint(), (n, Some(n)));
    }

    #[test]
    fn add_invoice() {
        let seed = 1;
        let amount = 100;
        let path_to_file = Path::new("../test_data/trivial.json");
        let graph = Graph::to_sim_graph(&network_parser::from_json_file(path_to_file).unwrap());
        let routing_metric = RoutingMetric::MinFee;
        let payment_parts = PaymentParts::Single;
        let mut simulator = Simulation::new(seed, graph, amount, routing_metric, payment_parts);
        let invoice = Invoice::new(
            simulator.next_payment_id(),
            1234,
            &"alice".to_string(),
            &"dina".to_string(),
        );
        simulator.add_invoice(invoice.clone());
        let invoice2 = Invoice::new(
            simulator.next_payment_id(),
            4321,
            &"alice".to_string(),
            &"dina".to_string(),
        );
        simulator.add_invoice(invoice2.clone());
        assert_eq!(simulator.outstanding_invoices.len(), 1);
        let actual = simulator
            .outstanding_invoices
            .get(&"dina".to_owned())
            .unwrap()
            .clone();
        let expected = HashMap::from([(invoice.id, invoice), (invoice2.id, invoice2)]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn get_invoices_for_node() {
        let seed = 1;
        let amount = 100;
        let path_to_file = Path::new("../test_data/trivial.json");
        let graph = Graph::to_sim_graph(&network_parser::from_json_file(path_to_file).unwrap());
        let routing_metric = RoutingMetric::MinFee;
        let payment_parts = PaymentParts::Single;
        let mut simulator = Simulation::new(seed, graph, amount, routing_metric, payment_parts);
        let invoice = Invoice::new(
            simulator.next_payment_id(),
            1234,
            &"alice".to_string(),
            &"dina".to_string(),
        );
        simulator.add_invoice(invoice.clone());
        let invoice2 = Invoice::new(
            simulator.next_payment_id(),
            4321,
            &"alice".to_string(),
            &"chan".to_string(),
        );
        simulator.add_invoice(invoice2.clone());
        let actual = simulator.get_invoices_for_node(&"dina".to_string());
        assert!(actual.is_some());
        let actual = actual.unwrap().clone();
        let expected = HashMap::from([(invoice.id, invoice)]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn delete_invoice() {
        let seed = 1;
        let amount = 100;
        let path_to_file = Path::new("../test_data/trivial.json");
        let graph = Graph::to_sim_graph(&network_parser::from_json_file(path_to_file).unwrap());
        let routing_metric = RoutingMetric::MinFee;
        let payment_parts = PaymentParts::Single;
        let mut simulator = Simulation::new(seed, graph, amount, routing_metric, payment_parts);
        let invoice = Invoice::new(
            simulator.next_payment_id(),
            1234,
            &"alice".to_string(),
            &"dina".to_string(),
        );
        simulator.add_invoice(invoice.clone());
        simulator.remove_invoice(&invoice);
        let actual = simulator.get_invoices_for_node(&"dina".to_string());
        assert!(actual.is_none());
    }
}

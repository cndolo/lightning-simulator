use crate::{
    core_types::graph::Graph,
    event::*,
    payment::{Payment, PaymentShard},
    time::Time,
    traversal::pathfinding::PathFinder,
    PaymentId, PaymentParts, RoutingMetric, ID,
};
use log::{debug, error, info};
use rand::SeedableRng;
use std::time::Instant;

pub struct Simulation {
    /// Graph describing LN topology
    graph: Graph,
    /// Payment amount to simulate
    amount: usize,
    /// Sim seed
    run: u64,
    /// Number of payments to simulate
    num_pairs: usize,
    /// Fee minimisation or probability maximisation
    routing_metric: RoutingMetric,
    /// Single or multi-path
    payment_parts: PaymentParts,
    /// Queue of events to be simulated
    event_queue: EventQueue,
    /// Assigned to each new payment
    current_payment_id: PaymentId,
}

impl Simulation {
    pub fn new(
        run: u64,
        graph: Graph,
        amount: usize,
        num_pairs: usize,
        routing_metric: RoutingMetric,
        payment_parts: PaymentParts,
    ) -> Self {
        info!("Initialising simulation...");
        info!(
            "# Payment pairs = {}, Pathfinding weight = {:?}, Single/MMP payments: {:?}",
            num_pairs, routing_metric, payment_parts
        );
        let mut rng = crate::RNG.lock().unwrap();
        *rng = SeedableRng::seed_from_u64(run);
        let event_queue = EventQueue::new();
        Self {
            graph,
            amount,
            run,
            num_pairs,
            routing_metric,
            payment_parts,
            event_queue,
            current_payment_id: 0,
        }
    }

    // 1. Create and queue payments in event queue
    // 2. Process event queue
    // 3. Evaluate and report simulation results
    pub fn run(&mut self) {
        info!(
            "Drawing {} sender-receiver pairs for simulation.",
            self.num_pairs
        );
        let random_pairs_iter = Self::draw_n_pairs_for_simulation(&self.graph, self.num_pairs);
        let mut now = Time::from_secs(0.0); // start simulation at (0)
        for (src, dest) in random_pairs_iter {
            let payment = Payment::new(self.next_payment_id(), src, dest, self.amount);
            let event = EventType::ScheduledPayment { payment };
            self.event_queue.schedule(now, event);
            now += Time::from_secs(crate::SIM_DELAY_IN_SECS);
        }
        debug!(
            "Queued {} events for simulation.",
            self.event_queue.queue_length()
        );

        info!("Starting simulation.");
        // this is where the actual simulation happens
        while let Some(event) = self.event_queue.next() {
            match event {
                EventType::ScheduledPayment { mut payment } => {
                    debug!(
                        "Dispatching scheduled payment {} at simulation time = {}.",
                        payment.payment_id,
                        self.event_queue.now()
                    );
                    match self.payment_parts {
                        PaymentParts::Single => self.send_single_payment(&mut payment),
                        PaymentParts::Split => self.send_mpp_payment(&mut payment),
                    }
                }
            }
        }
    }

    // 2. Send payment (Try each path in order until payment succeeds (the trial-and-error loop))
    // 2.0. create payment
    // 2.1. try candidate paths sequentially (trial-and-error loop)
    // 2.2. record success or failure (where?)
    // 2.3. update states (node balances, ???)
    fn send_single_payment(&self, mut payment: &mut Payment) {
        let graph = Box::new(self.graph.clone());
        if graph.get_max_edge_balance(&payment.source, &payment.dest) < payment.amount_msat {
            // TODO: immediate failure
        }
        let mut path_finder = PathFinder::new(
            payment.source.clone(),
            payment.dest.clone(),
            payment.amount_msat,
            graph,
            self.routing_metric,
            self.payment_parts,
        );
        let start = Instant::now();
        if let Some(candidate_paths) = path_finder.find_path() {
            payment.paths = candidate_paths.clone();
            let duration_in_ms = start.elapsed().as_millis();
            info!(
                "Found {} paths after {} ms.",
                candidate_paths.len(),
                duration_in_ms
            );
            // fail immediately if sender's balance < amount
            let payment_shard = payment.to_shard(payment.amount_msat);
            let success = self.attempt_payment(&payment_shard);
            if success {
                // TODO
                payment.failed = !success;
            }
        } else {
            error!("No paths found.");
        }
    }

    // 1. Split payment into n parts
    //  - observe min amount
    //  2. Find paths for all parts
    //  TODO: Maybe expect a shard
    fn send_mpp_payment(&self, mut payment: &mut Payment) {
        let graph = Box::new(self.graph.clone());
        if graph.get_max_edge_balance(&payment.source, &payment.dest) < payment.amount_msat {
            // TODO: immediate failure
        }
        let mut path_finder = PathFinder::new(
            payment.source.clone(),
            payment.dest.clone(),
            payment.amount_msat,
            graph,
            self.routing_metric,
            self.payment_parts,
        );

        let start = Instant::now();
        if let Some(candidate_paths) = path_finder.find_path() {
            payment.paths = candidate_paths.clone();
            let duration_in_ms = start.elapsed().as_millis();
            info!(
                "Found {} paths after {} ms.",
                candidate_paths.len(),
                duration_in_ms
            );
            let payment_shard = payment.to_shard(payment.amount_msat);
            let success = self.attempt_payment(&payment_shard);
            if success {
                // TODO
                payment.failed = success;
            } else {
                if let Some(split_shard) = payment_shard.split_payment() {
                    let (shard1, shard2) = (split_shard.0, split_shard.1);
                }
            }
        }
    }

    fn attempt_payment(&self, payment_shard: &PaymentShard) -> bool {
        info!(
            "{} attempting to send {} msats to {}.",
            payment_shard.source, payment_shard.amount, payment_shard.dest
        );
        false
    }

    fn draw_n_pairs_for_simulation(
        graph: &Graph,
        n: usize,
    ) -> (impl Iterator<Item = (ID, ID)> + Clone) {
        let g = graph.clone();
        (0..n)
            .collect::<Vec<_>>()
            .into_iter()
            .map(move |_| g.clone().get_random_pair_of_nodes())
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
        let seed = 1;
        let amount = 100;
        let pairs = 2;
        let path_to_file = Path::new("../test_data/trivial.json");
        let graph = Graph::to_sim_graph(&network_parser::from_json_file(path_to_file).unwrap());
        let routing_metric = RoutingMetric::MinFee;
        let payment_parts = PaymentParts::Single;
        let actual = Simulation::new(seed, graph, amount, pairs, routing_metric, payment_parts);
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
}

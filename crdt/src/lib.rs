use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GCounter{
    node_id: String,
    counts: HashMap<String, u64>,
}

impl GCounter {
    pub fn new(node_id: String) -> Self{
        let mut counts = HashMap::new();
        counts.insert(node_id.clone(),0);

        Self {
            node_id, counts,
        }

    }

    pub fn increment(&mut self){
        let key = self.node_id.clone();
        self.counts.entry(key).and_modify(|v| *v += 1);
    }

    pub fn merge(&mut self, other: &GCounter){
        for (node, incoming) in &other.counts{
            self.counts.entry(node.clone()).and_modify(|v| *v=(*v).max(*incoming)).or_insert(*incoming);
        }
    }

    pub fn value(&self) -> u64{
        self.counts.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_counter_starts_at_zero() {
        let counter = GCounter::new("node_a".to_string());

        assert_eq!(counter.value(), 0);
    }

    #[test]
    fn increment_only_updates_own_slot() {
        let mut counter = GCounter::new("node_a".to_string());

        counter.increment();
        counter.increment();

        assert_eq!(counter.value(), 2);
    }

    #[test]
    fn merge_keeps_maximum_value_per_node() {
        let mut a = GCounter::new("node_a".to_string());
        let mut b = GCounter::new("node_b".to_string());

        a.increment();
        a.increment();

        b.increment();
        b.increment();
        b.increment();

        a.merge(&b);

        assert_eq!(a.value(), 5);
    }

    #[test]
    fn merge_is_idempotent() {
        let mut a = GCounter::new("node_a".to_string());
        let mut b = GCounter::new("node_b".to_string());

        a.increment();
        b.increment();
        b.increment();

        a.merge(&b);
        a.merge(&b);

        assert_eq!(a.value(), 3);
    }

    #[test]
    fn merge_does_not_modify_the_argument() {
        let mut a = GCounter::new("node_a".to_string());
        let mut b = GCounter::new("node_b".to_string());

        a.increment();
        a.increment();

        b.increment();

        let before = b.value();

        a.merge(&b);

        assert_eq!(b.value(), before);
    }

    #[test]
    fn merge_overlapping_slots_keeps_maximum() {
        let mut first = GCounter::new("node_a".to_string());
        let mut second = GCounter::new("node_a".to_string());

        first.increment();
        first.increment();

        second.increment();
        second.increment();
        second.increment();

        first.merge(&second);

        assert_eq!(first.value(), 3);
    }
}
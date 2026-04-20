use crdt::GCounter;
use std::time::{Instant, Duration};

#[derive(Debug, Clone)]
pub struct SlidingWindow{
    current: GCounter,
    previous: GCounter,
    window_start: Instant,
    window_size: Duration,

}

impl SlidingWindow {
    pub fn new(node_id: String, timeout: Duration) -> Self{
        let current = GCounter::new(node_id.clone());
        let previous = GCounter::new(node_id);
        let window_start = Instant::now();
        let window_size = timeout;

        Self {
            current, previous, window_start, window_size
        }

    }

    pub fn increment(&mut self) {
    if self.window_start.elapsed() >= self.window_size {
        let node_id = self.current.node_id.clone();
        let old_current = std::mem::replace(
            &mut self.current,
            GCounter::new(node_id),
        );
        self.previous = old_current;
        self.window_start = Instant::now();
    }
    self.current.increment();
}

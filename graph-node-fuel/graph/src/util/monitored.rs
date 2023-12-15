use prometheus::{Counter, Gauge};
use std::collections::VecDeque;

pub struct MonitoredVecDeque<T> {
    vec_deque: VecDeque<T>,
    depth: Gauge,
    popped: Counter,
}

impl<T> MonitoredVecDeque<T> {
    pub fn new(depth: Gauge, popped: Counter) -> Self {
        Self {
            vec_deque: VecDeque::new(),
            depth,
            popped,
        }
    }

    pub fn push_back(&mut self, item: T) {
        self.vec_deque.push_back(item);
        self.depth.set(self.vec_deque.len() as f64);
    }

    pub fn push_front(&mut self, item: T) {
        self.vec_deque.push_front(item);
        self.depth.set(self.vec_deque.len() as f64);
    }

    pub fn pop_front(&mut self) -> Option<T> {
        let item = self.vec_deque.pop_front();
        self.depth.set(self.vec_deque.len() as f64);
        if item.is_some() {
            self.popped.inc();
        }
        item
    }

    pub fn is_empty(&self) -> bool {
        self.vec_deque.is_empty()
    }
}

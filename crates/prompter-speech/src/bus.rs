//! Transcript bus: broadcast to any number of consumers (CLAUDE.md rule 5).
//! The matcher, the take recorder and the operator panel all subscribe here.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;

use prompter_core::TranscriptEvent;

#[derive(Default)]
pub struct TranscriptBus {
    senders: Mutex<Vec<Sender<TranscriptEvent>>>,
}

impl TranscriptBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> Receiver<TranscriptEvent> {
        let (tx, rx) = channel();
        self.senders.lock().expect("bus").push(tx);
        rx
    }

    pub fn publish(&self, ev: TranscriptEvent) {
        let mut senders = self.senders.lock().expect("bus");
        // Drop subscribers that went away.
        senders.retain(|s| s.send(ev.clone()).is_ok());
    }

    pub fn subscriber_count(&self) -> usize {
        self.senders.lock().expect("bus").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn every_subscriber_gets_every_event_and_dead_ones_are_dropped() {
        let bus = TranscriptBus::new();
        let a = bus.subscribe();
        let b = bus.subscribe();
        bus.publish(TranscriptEvent::silence(Duration::from_secs(1), Instant::now()));
        assert!(matches!(a.recv().unwrap(), TranscriptEvent::Silence { .. }));
        assert!(matches!(b.recv().unwrap(), TranscriptEvent::Silence { .. }));
        drop(b);
        bus.publish(TranscriptEvent::silence(Duration::from_secs(2), Instant::now()));
        assert_eq!(bus.subscriber_count(), 1);
        assert!(a.recv().is_ok());
    }
}

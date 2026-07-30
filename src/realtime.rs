use tokio::sync::broadcast;

/// In-process update notifications for the single-instance MVP.
#[derive(Clone, Debug)]
pub struct RoomUpdateHub {
    sender: broadcast::Sender<String>,
}

impl Default for RoomUpdateHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }
}

impl RoomUpdateHub {
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }

    pub fn notify(&self, slug: &str) {
        // No connected participants is a normal state, so a send error is ignored.
        let _ = self.sender.send(slug.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::RoomUpdateHub;

    #[tokio::test]
    async fn subscribers_receive_room_updates() {
        let hub = RoomUpdateHub::default();
        let mut receiver = hub.subscribe();

        hub.notify("room-42");

        assert_eq!(receiver.recv().await.unwrap(), "room-42");
    }
}

use bevy::prelude::*;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetState {
    pub kind: String,
    pub nickname: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
}

#[derive(Resource)]
pub struct NetworkBridge {
    pub outgoing: Sender<NetState>,
    pub incoming: Receiver<NetState>,
}

#[derive(Component)]
pub struct RemotePlayer {
    pub nickname: String,
}

pub fn spawn_bridge(url: String) -> NetworkBridge {
    let (to_net_tx, to_net_rx) = unbounded::<NetState>();
    let (from_net_tx, from_net_rx) = unbounded::<NetState>();

    thread::spawn(move || {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(err) => { eprintln!("network runtime: {err}"); return; }
        };
        runtime.block_on(async move {
            let Ok((stream, _)) = tokio_tungstenite::connect_async(&url).await else {
                eprintln!("could not connect to bi-lan bridge at {url}");
                return;
            };
            let (mut sink, mut source) = stream.split();
            let mut ticker = tokio::time::interval(Duration::from_millis(25));
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        while let Ok(state) = to_net_rx.try_recv() {
                            if let Ok(text) = serde_json::to_string(&state) {
                                if sink.send(Message::Text(text.into())).await.is_err() { return; }
                            }
                        }
                    }
                    maybe = source.next() => {
                        let Some(Ok(msg)) = maybe else { return; };
                        if let Ok(text) = msg.to_text() {
                            if let Ok(state) = serde_json::from_str::<NetState>(text) {
                                let _ = from_net_tx.send(state);
                            }
                        }
                    }
                }
            }
        });
    });

    NetworkBridge { outgoing: to_net_tx, incoming: from_net_rx }
}

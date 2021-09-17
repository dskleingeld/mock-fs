use crate::server_conn::protocol::{FromRS, ToRs};
use client_protocol::connection;
use discovery::Chart;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::timeout;
use tokio::{net::TcpStream, time};

pub mod election;
mod replicate;
pub use replicate::update;
mod state;
pub use state::State;

const HB_TIMEOUT: Duration = Duration::from_secs(2);
pub async fn maintain_heartbeat(state: &State, chart: &Chart) {
    loop {
        let term = state.term();
        let heartbeats = chart
            .map
            .iter()
            .map(|m| m.value().clone())
            .map(|addr| send_hb(addr, term, state.change_idx()));

        let send_all = futures::future::join_all(heartbeats);
        let _ = timeout(Duration::from_millis(500), send_all).await;
        time::sleep(HB_TIMEOUT / 2).await;
    }
}

async fn send_hb(addr: SocketAddr, term: u64, change_idx: u64) -> Option<()> {
    use futures::SinkExt;
    type RsStream = connection::MsgStream<FromRS, ToRs>;

    let socket = TcpStream::connect(addr).await.ok()?;
    let mut stream: RsStream = connection::wrap(socket);
    stream.send(ToRs::HeartBeat(term, change_idx)).await.ok()?;
    Some(())
}

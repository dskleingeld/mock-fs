use client_protocol::connection;
use discovery::Chart;
use futures::future::join_all;
use futures::SinkExt;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::consensus::{State, HB_TIMEOUT};
use crate::server_conn::protocol::{Change, FromRS, ToRs};

type RsStream = connection::MsgStream<FromRS, ToRs>;

#[derive(Clone, Debug)]
pub struct ReadServers(Arc<Mutex<Inner>>);

impl ReadServers {
    pub fn new(chart: Chart, port: u16) -> Self {
        Self(Arc::new(Mutex::new(Inner::new(chart, port))))
    }
    pub async fn publish(&self, state: &State, change: Change) -> PubResult {
        let msg = ToRs::DirectoryChange(state.term(), state.increase_change_idx(), change);
        let reached = self.0.lock().await.send_to_readservers(msg).await as u16;

        let majority = state.cluster_size / 2;
        if reached == state.cluster_size {
            PubResult::ReachedAll
        } else if reached > majority {
            PubResult::ReachedMajority
        } else {
            PubResult::ReachedMinority
        }
    }
}

#[derive(Debug)]
struct Inner {
    port: u16,
    conns: HashMap<IpAddr, RsStream>,
    chart: Chart,
}

pub enum PubResult {
    ReachedAll,
    ReachedMajority,
    ReachedMinority,
}

async fn send(msg: ToRs, ip: IpAddr, conn: &mut RsStream) -> Result<IpAddr, IpAddr> {
    match timeout(HB_TIMEOUT, conn.send(msg)).await {
        Err(_) => Err(ip),
        Ok(Err(_)) => Err(ip),
        Ok(Ok(_)) => Ok(ip),
    }
}

async fn conn_and_send(msg: ToRs, ip: IpAddr, port: u16) -> Result<(IpAddr, RsStream), ()> {
    let addr = SocketAddr::from((ip, port));
    let stream = TcpStream::connect(addr).await.map_err(|_| ())?;
    let mut conn: RsStream = connection::wrap(stream);
    match timeout(HB_TIMEOUT, conn.send(msg)).await {
        Err(_) => Err(()),
        Ok(Err(_)) => Err(()),
        Ok(Ok(_)) => Ok((ip, conn)),
    }
}

impl Inner {
    pub fn new(chart: Chart, port: u16) -> Self {
        Self {
            port,
            conns: HashMap::new(),
            chart,
        }
    }

    async fn send_to_readservers(&mut self, msg: ToRs) -> usize {
        let conn_ips: HashSet<_> = self.conns.keys().cloned().collect();

        let jobs = self
            .conns
            .iter_mut()
            .map(|(ip, conn)| send(msg.clone(), *ip, conn));

        let results = join_all(jobs).await.into_iter();
        for failed in results.filter_map(Result::err) {
            self.conns.remove(&failed);
        }

        let untried = self
            .chart
            .adresses()
            .into_iter()
            .map(|addr| addr.ip())
            .filter(|addr| !conn_ips.contains(&addr));

        let jobs = untried.map(|ip| conn_and_send(msg.clone(), ip, self.port));
        let new_ok_conns = join_all(jobs).await.into_iter().filter_map(Result::ok);
        self.conns.extend(new_ok_conns);
        self.conns.len()
    }
}

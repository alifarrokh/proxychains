use crate::connection::Connection;
use futures::stream::Stream;
use futures::task::{Context, Poll, Waker};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::mpsc::{Receiver, Sender};

// A stream of Connections
#[derive(Debug)]
pub struct ConnectionListener {
    // A one-time used channel initialize the stream waker (CONNECTION_LISTENER_WAKER)
    waker_sender: Sender<Waker>,
    // Check whether if waker is sent or not
    is_waker_sent: bool,
    // Receiver half of a channel which is responsible to give new Connection(s) to
    // ConnectionListener
    connection_receiver: Receiver<(u32, SocketAddr)>,
}

impl ConnectionListener {
    pub fn new(sender: Sender<Waker>, receiver: Receiver<(u32, SocketAddr)>) -> Self {
        ConnectionListener {
            is_waker_sent: false,
            waker_sender: sender,
            connection_receiver: receiver,
        }
    }
}

impl Stream for ConnectionListener {
    type Item = Connection;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if !self.is_waker_sent {
            if let Ok(_) = self.waker_sender.send(cx.waker().clone()) {
                self.is_waker_sent = true;
            }
            return Poll::Pending;
        }
        if let Ok(socket) = self.connection_receiver.recv() {
            return Poll::Ready(Some(Connection::new(socket.0, socket.1)));
        }
        Poll::Ready(None)
    }
}

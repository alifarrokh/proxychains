use futures::task::{Context, Poll, Waker};
use std::{
    net::SocketAddr,
    pin::Pin,
    sync::mpsc::{channel, Receiver, Sender},
};
use tokio::io::{AsyncRead, AsyncWrite};

// Connection is like an asynchronous TCP stream.
// Think of it as a stream that reads and writes data to main app.
// It uses 2 channels.
// Write channel is reponsible to send response back to hooked read function.
// Read channel is reponsible to get data from hooked write function.
#[derive(Debug)]
pub struct Connection {
    pub fd: u32,
    pub target_addr: SocketAddr,

    reader: Reader,
    reader_sender: Sender<Vec<u8>>,

    writer: Writer,
    writer_receiver: Receiver<Vec<u8>>,
}

impl Connection {
    pub fn new(fd: u32, socket_addr: SocketAddr) -> Connection {
        // Read channel
        let (reader_sender, reader_receiver) = channel::<Vec<u8>>();

        // Write channel
        let (writer_sender, writer_receiver) = channel::<Vec<u8>>();

        Connection {
            fd,
            target_addr: socket_addr,
            reader_sender,
            writer_receiver,
            reader: Reader {
                waker: None,
                receiver: reader_receiver,
            },
            writer: Writer {
                sender: writer_sender,
            },
        }
    }

    pub fn get_reader_waker(&mut self) -> Option<Waker> {
        self.reader.waker.clone()
    }

    pub fn get_reader_sender(&mut self) -> &mut Sender<Vec<u8>> {
        &mut self.reader_sender
    }

    pub fn get_writer_receiver(&mut self) -> &mut Receiver<Vec<u8>> {
        &mut self.writer_receiver
    }

    pub fn split(&mut self) -> (&mut Reader, &mut Writer) {
        (&mut self.reader, &mut self.writer)
    }
}

// Get data from hooked write function
#[derive(Debug)]
pub struct Reader {
    // This waker is available to hooked write function
    pub waker: Option<Waker>,
    // Data receiver (from hooked write function)
    pub receiver: Receiver<Vec<u8>>,
}
impl AsyncRead for Reader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if let None = self.waker {
            self.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        match self.receiver.try_recv() {
            Ok(data) => {
                data.iter().enumerate().for_each(|(i, byte)| {
                    buf[i] = *byte;
                });
                Poll::Ready(Ok(data.len()))
            }
            Err(_) => Poll::Pending,
        }
    }
}

// Send response back to hooked read function
#[derive(Debug)]
pub struct Writer {
    // Data sender (to hooked write function)
    sender: Sender<Vec<u8>>,
}
impl AsyncWrite for Writer {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let data = Vec::from(buf);
        let len = data.len();
        match self.sender.send(data) {
            Ok(_) => Poll::Ready(Ok(len)),
            Err(err) => Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, err))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }
}

//! Multi-producer single-consumer channel.

use std::sync::mpsc;

pub struct Sender<T> {
    inner: mpsc::Sender<T>,
}

pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
}

pub fn channel<T: Send>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel();
    (Sender { inner: tx }, Receiver { inner: rx })
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        self.inner.send(value)
    }
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> Result<T, mpsc::RecvError> {
        self.inner.recv()
    }

    pub fn try_recv(&self) -> Result<T, mpsc::TryRecvError> {
        self.inner.try_recv()
    }
}

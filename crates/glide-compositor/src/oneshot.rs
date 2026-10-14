use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, PoisonError},
    task::{Context, Poll, Waker},
};

struct Slot<T> {
    value: Option<T>,
    waker: Option<Waker>,
    closed: bool,
}

pub struct Sender<T>(Arc<Mutex<Slot<T>>>);

pub struct Receiver<T>(Arc<Mutex<Slot<T>>>);

#[must_use]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let slot = Arc::new(Mutex::new(Slot { value: None, waker: None, closed: false }));
    (Sender(slot.clone()), Receiver(slot))
}

impl<T> Sender<T> {
    pub fn send(self, value: T) {
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        slot.value = Some(value);
        if let Some(waker) = slot.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        slot.closed = true;
        if let Some(waker) = slot.waker.take() {
            waker.wake();
        }
    }
}

impl<T> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("oneshot::Sender")
    }
}

impl<T> Future for Receiver<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(value) = slot.value.take() {
            return Poll::Ready(Some(value));
        }
        if slot.closed {
            return Poll::Ready(None);
        }
        slot.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::Arc,
        task::{Wake, Waker},
        thread::Thread,
    };

    struct Unpark(Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            std::thread::park();
        }
    }

    #[test]
    fn a_value_sent_from_another_thread_wakes_the_receiver() {
        let (sender, receiver) = channel();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            sender.send(42);
        });
        assert_eq!(block_on(receiver), Some(42));
        worker.join().expect("the sender thread finishes");
    }

    #[test]
    fn dropping_the_sender_resolves_the_receiver_empty() {
        let (sender, receiver) = channel::<u8>();
        drop(sender);
        assert_eq!(block_on(receiver), None);
    }

    #[test]
    fn a_value_sent_before_polling_is_not_lost() {
        let (sender, receiver) = channel();
        sender.send("ready");
        assert_eq!(block_on(receiver), Some("ready"));
    }
}

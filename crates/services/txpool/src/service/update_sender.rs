use std::{
    collections::HashMap,
    pin::Pin,
    time::Duration,
};

use super::*;
use parking_lot::Mutex;
use tokio::{
    sync::{
        mpsc::{
            self,
            error::TrySendError,
        },
        OwnedSemaphorePermit,
        Semaphore,
    },
    time::Instant,
};
use tokio_stream::{
    wrappers::ReceiverStream,
    Stream,
};

use tx_status_stream::TxUpdateStream;

#[cfg(test)]
mod tests;
mod tx_status_stream;

/// Subscriber channel buffer size.
/// Subscribers will only ever get at most a submitted
/// and final transaction status update.
const BUFFER_SIZE: usize = 2;

/// UpdateSender is responsible for managing subscribers
/// and sending transaction status updates to them.
///
/// Subscribers are added only once a permit is available.
#[derive(Debug)]
pub struct UpdateSender {
    /// Map of senders, indexed by transaction hash.
    senders: Arc<Mutex<SenderMap<Permit, Tx>>>,
    /// Semaphore used to limit the number of concurrent subscribers.
    permits: GetPermit,
    /// TTL for senders
    ttl: Duration,
}

/// Error returned when a transaction status update cannot be sent.
#[derive(Debug)]
pub enum SendError {
    /// The subscriber channel is full.
    Full,
    /// The subscriber channel is closed.
    Closed,
}

pub trait PermitTrait: Send + Sync {}

/// A permit to subscribe to transaction status updates.
/// Permits are freed when dropped.
pub type Permit = Box<dyn PermitTrait + Send + Sync + 'static>;

/// The sending end of a subscriber channel.
pub type Tx = Box<dyn SendStatus + Send + Sync + 'static>;

/// A map of senders, indexed by transaction hash.
type SenderMap<P, Tx> = HashMap<Bytes32, Vec<Sender<P, Tx>>>;

/// A stream of transaction status updates.
pub type TxStatusStream = Pin<Box<dyn Stream<Item = TxStatusMessage> + Send + Sync>>;

/// Gives permits to subscribe once they are available.
type GetPermit = Arc<dyn PermitsDebug + Send + Sync>;

/// A sender that is subscribed to transaction status updates
/// for a specific transaction hash.
struct Sender<P = OwnedSemaphorePermit, Tx = mpsc::Sender<TxStatusMessage>> {
    /// The permit used to subscribe to transaction status updates.
    _permit: P,
    /// The stream of transaction status updates.
    stream: TxUpdateStream,
    /// The sending end of the subscriber channel.
    tx: Tx,
    /// time that this sender was created
    created: Instant,
}

/// A trait for sending transaction status updates.
#[cfg_attr(test, mockall::automock)]
pub trait SendStatus {
    /// Try to send a transaction status message to the receiver.
    fn try_send(&mut self, msg: TxStatusMessage) -> Result<(), SendError>;

    /// Check if the receiver is closed.
    fn is_closed(&self) -> bool;

    /// Check if the receiver is full.
    fn is_full(&self) -> bool;
}

/// A trait for creating a new channel.
pub trait CreateChannel {
    /// Create a new channel.
    /// Returns the sending end of the channel and
    /// a stream of transaction status messages.
    fn channel() -> (Tx, TxStatusStream);
}

/// A trait for getting a new permit.
#[cfg_attr(test, mockall::automock(type P = ();))]
trait Permits {
    /// Try to acquire a permit.
    fn try_acquire(self: Arc<Self>) -> Option<Permit>;
}

/// Combines `Permits` and `std::fmt::Debug`.
trait PermitsDebug: Permits + std::fmt::Debug {}

impl<T: Permits + std::fmt::Debug> PermitsDebug for T {}

/// Creates a `tokio::sync::mpsc` channel
/// with a buffer size of `BUFFER_SIZE`.
pub struct MpscChannel;

impl CreateChannel for MpscChannel {
    fn channel() -> (Tx, TxStatusStream) {
        let (tx, rx) = mpsc::channel(BUFFER_SIZE);
        (Box::new(tx), Box::pin(ReceiverStream::from(rx)))
    }
}

impl Permits for Semaphore {
    fn try_acquire(self: Arc<Self>) -> Option<Permit> {
        Semaphore::try_acquire_owned(self).ok().map(|p| {
            let b: Permit = Box::new(p);
            b
        })
    }
}

impl PermitTrait for OwnedSemaphorePermit {}

impl<P, Tx> SendStatus for Sender<P, Tx>
where
    Tx: SendStatus,
{
    fn try_send(&mut self, msg: TxStatusMessage) -> Result<(), SendError> {
        // Add the message to the stream.
        self.stream.add_msg(msg);

        // Try to send the next message in the stream.
        if let Some(msg) = self.stream.try_next() {
            // Send the message to the outgoing channel.
            match self.tx.try_send(msg) {
                Ok(()) => (),
                // If the channel is full, add a failure to the stream.
                Err(SendError::Full) => self.stream.add_failure(),
                // If the channel is closed, close the stream.
                Err(SendError::Closed) => self.stream.close_recv(),
            }
        }

        // Check if the stream is now closed.
        if self.stream.is_closed() {
            Err(SendError::Closed)
        } else {
            Ok(())
        }
    }

    fn is_closed(&self) -> bool {
        self.stream.is_closed()
    }

    fn is_full(&self) -> bool {
        self.tx.is_full()
    }
}

impl SendStatus for mpsc::Sender<TxStatusMessage> {
    fn try_send(&mut self, msg: TxStatusMessage) -> Result<(), SendError> {
        match (*self).try_send(msg) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SendError::Full),
            Err(TrySendError::Closed(_)) => Err(SendError::Closed),
        }
    }

    fn is_closed(&self) -> bool {
        self.is_closed()
    }

    fn is_full(&self) -> bool {
        self.capacity() == 0
    }
}

impl UpdateSender {
    /// Create a new UpdateSender with a specified capacity for the semaphore
    pub fn new(capacity: usize, ttl: Duration) -> UpdateSender {
        UpdateSender {
            senders: Default::default(),
            permits: Arc::new(Semaphore::new(capacity)),
            ttl,
        }
    }

    /// Try to subscribe for updates, returns a TxStatusStream if successful
    pub fn try_subscribe<C>(&self, tx_id: Bytes32) -> Option<TxStatusStream>
    where
        C: CreateChannel,
    {
        // Remove closed senders from the list
        remove_closed_and_expired(&mut self.senders.lock(), self.ttl);

        // Try to acquire a permit from the semaphore
        let permit = Arc::clone(&self.permits).try_acquire()?;

        // Call subscribe_inner with the acquired permit
        Some(self.subscribe_inner::<C>(tx_id, permit))
    }

    /// Subscribe to updates with the given transaction id and a permit.
    fn subscribe_inner<C>(&self, tx_id: Bytes32, permit: Permit) -> TxStatusStream
    where
        C: CreateChannel,
    {
        // Lock the senders Mutex
        let mut senders = self.senders.lock();

        // Remove closed senders from the list
        remove_closed_and_expired(&mut senders, self.ttl);

        // Call the subscribe function with the tx_id, senders, and permit
        subscribe::<_, C>(tx_id, &mut (*senders), permit)
    }

    /// Send updates to all subscribed senders.
    pub fn send(&self, update: TxUpdate) {
        // Lock the senders Mutex.
        let mut senders = self.senders.lock();

        // Remove closed senders from the list
        remove_closed_and_expired(&mut senders, self.ttl);

        // Initialize a flag to check if there are no senders
        // left for a given tx_id.
        let mut empty = false;

        if let Some(senders) = senders.get_mut(update.tx_id()) {
            // Retain only senders that are able to receive the update.
            senders
                .retain_mut(|sender| sender.try_send(update.clone().into_msg()).is_ok());

            // Check if the list of senders for the tx_id is empty.
            empty = senders.is_empty();
        }

        // Remove the tx_id from senders if there are no senders left
        if empty {
            senders.remove(update.tx_id());
        }
    }
}

// Create and subscribe a new channel to the senders map
fn subscribe<P, C>(
    tx_id: Bytes32,                 // transaction ID
    senders: &mut SenderMap<P, Tx>, // mutable senders map reference
    permit: P,                      // permit of type P
) -> TxStatusStream
where
    C: CreateChannel,
{
    // Create a new channel of type C
    let (tx, rx) = C::channel();

    // Insert a new vec into the senders map if not exists,
    // and then push the sender to the vec.
    senders.entry(tx_id).or_default().push(Sender {
        _permit: permit,
        stream: TxUpdateStream::new(),
        tx,
        created: Instant::now(),
    });

    // Return the receiver part of the channel
    rx
}

// Remove closed senders from the senders map
fn remove_closed_and_expired<P, Tx>(senders: &mut SenderMap<P, Tx>, ttl: Duration)
where
    Tx: SendStatus,
{
    // Iterate over the senders map, retaining only the senders that are not closed
    senders.retain(|_, senders| {
        senders.retain(|sender| !sender.is_closed() && sender.created.elapsed() < ttl);
        // Continue retaining if the senders list is not empty
        !senders.is_empty()
    });
}

impl<T> SendStatus for Box<T>
where
    T: SendStatus + ?Sized,
{
    fn try_send(&mut self, msg: TxStatusMessage) -> Result<(), SendError> {
        (**self).try_send(msg)
    }

    fn is_closed(&self) -> bool {
        (**self).is_closed()
    }

    fn is_full(&self) -> bool {
        (**self).is_full()
    }
}

impl Clone for UpdateSender {
    fn clone(&self) -> Self {
        Self {
            senders: self.senders.clone(),
            permits: self.permits.clone(),
            ttl: self.ttl,
        }
    }
}

impl<P, Tx> std::fmt::Debug for Sender<P, Tx> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender")
            .field("stream", &self.stream)
            .finish()
    }
}

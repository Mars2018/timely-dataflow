//! A simple communication infrastructure providing typed exchange channels.
//!
//! This crate is part of the timely dataflow system, used primarily for its inter-worker communication.
//! It may be independently useful, but it is separated out mostly to make clear boundaries in the project.
//!
//! Threads are spawned with an [`allocator::Generic`](./allocator/generic/enum.Generic.html), whose `allocate` method returns a pair of several send endpoints and one
//! receive endpoint. Messages sent into a send endpoint will eventually be received by the corresponding worker,
//! if it receives often enough. The point-to-point channels are each FIFO, but with no fairness guarantees.
//!
//! To be communicated, a type must implement the [`Serialize`](./trait.Serialize.html) trait. A default implementation of `Serialize` is
//! provided for any type implementing [`Abomonation`](../abomonation/trait.Abomonation.html). To implement other serialization strategies, wrap your type
//! and implement `Serialize` for your wrapper.
//!
//! Channel endpoints also implement a lower-level `push` and `pull` interface (through the [`Push`](./trait.Push.html) and [`Pull`](./trait.Pull.html)
//! traits), which is used for more precise control of resources.
//!
//! # Examples
//! ```
//! use timely_communication::Allocate;
//!
//! // configure for two threads, just one process.
//! let config = timely_communication::Configuration::Process(2);
//!
//! // initializes communication, spawns workers
//! let guards = timely_communication::initialize(config, |mut allocator| {
//!     println!("worker {} started", allocator.index());
//!
//!     // allocates pair of senders list and one receiver.
//!     let (mut senders, mut receiver) = allocator.allocate(0);
//!
//!     // send typed data along each channel
//!     use timely_communication::Message;
//!     senders[0].send(Message::from_typed(format!("hello, {}", 0)));
//!     senders[1].send(Message::from_typed(format!("hello, {}", 1)));
//!
//!     // no support for termination notification,
//!     // we have to count down ourselves.
//!     let mut expecting = 2;
//!     while expecting > 0 {
//!
//!         allocator.pre_work();
//!         if let Some(message) = receiver.recv() {
//!             use std::ops::Deref;
//!             println!("worker {}: received: <{}>", allocator.index(), message.deref());
//!             expecting -= 1;
//!         }
//!         allocator.post_work();
//!     }
//!
//!     // optionally, return something
//!     allocator.index()
//! });
//!
//! // computation runs until guards are joined or dropped.
//! if let Ok(guards) = guards {
//!     for guard in guards.join() {
//!         println!("result: {:?}", guard);
//!     }
//! }
//! else { println!("error in computation"); }
//! ```
//!
//! The should produce output like:
//!
//! ```ignore
//! worker 0 started
//! worker 1 started
//! worker 0: received: <hello, 0>
//! worker 1: received: <hello, 1>
//! worker 0: received: <hello, 0>
//! worker 1: received: <hello, 1>
//! result: Ok(0)
//! result: Ok(1)
//! ```

#![forbid(missing_docs)]

#[cfg(feature = "getopts")]
extern crate getopts;
#[cfg(feature = "bincode")]
extern crate bincode;
#[cfg(feature = "bincode")]
extern crate serde;

extern crate abomonation;
#[macro_use] extern crate abomonation_derive;

extern crate timely_bytes as bytes;
extern crate timely_logging as logging_core;

pub mod allocator;
pub mod networking;
pub mod initialize;
pub mod logging;
pub mod message;

use std::any::Any;

#[cfg(feature = "bincode")]
use serde::{Serialize, Deserialize};
#[cfg(not(feature = "bincode"))]
use abomonation::Abomonation;

pub use allocator::Generic as Allocator;
pub use allocator::Allocate;
pub use initialize::{initialize, initialize_from, Configuration, WorkerGuards};
pub use message::Message;

/// A composite trait for types that may be used with channels.
#[cfg(not(feature = "bincode"))]
pub trait Data : Send+Sync+Any+Abomonation+'static { }
#[cfg(not(feature = "bincode"))]
impl<T: Send+Sync+Any+Abomonation+'static> Data for T { }

/// A composite trait for types that may be used with channels.
#[cfg(feature = "bincode")]
pub trait Data : Send+Sync+Any+Serialize+for<'a>Deserialize<'a>+'static { }
#[cfg(feature = "bincode")]
impl<T: Send+Sync+Any+Serialize+for<'a>Deserialize<'a>+'static> Data for T { }

/// Pushing elements of type `T`.
///
/// This trait moves data around using references rather than ownership,
/// which provides the opportunity for zero-copy operation. In the call
/// to `push(element)` the implementor can *swap* some other value to
/// replace `element`, effectively returning the value to the caller.
///
/// Conventionally, a sequence of calls to `push()` should conclude with
/// a call of `push(&mut None)` or `done()` to signal to implementors that
/// another call to `push()` may not be coming.
pub trait Push<T> {
    /// Pushes `element` with the opportunity to take ownership.
    fn push(&mut self, element: &mut Option<T>);
    /// Pushes `element` and drops any resulting resources.
    #[inline(always)]
    fn send(&mut self, element: T) { self.push(&mut Some(element)); }
    /// Pushes `None`, conventionally signalling a flush.
    #[inline(always)]
    fn done(&mut self) { self.push(&mut None); }
}

impl<T, P: ?Sized + Push<T>> Push<T> for Box<P> {
    #[inline(always)]
    fn push(&mut self, element: &mut Option<T>) { (**self).push(element) }
}

/// Pulling elements of type `T`.
pub trait Pull<T> {
    /// Pulls an element and provides the opportunity to take ownership.
    ///
    /// The puller may mutate the result, in particular take ownership of the data by
    /// replacing it with other data or even `None`. This allows the puller to return
    /// resources to the implementor.
    ///
    /// If `pull` returns `None` this conventionally signals that no more data is available
    /// at the moment, and the puller should find something better to do.
    fn pull(&mut self) -> &mut Option<T>;
    /// Takes an `Option<T>` and leaves `None` behind.
    #[inline(always)]
    fn recv(&mut self) -> Option<T> { self.pull().take() }
}

impl<T, P: ?Sized + Pull<T>> Pull<T> for Box<P> {
    #[inline(always)]
    fn pull(&mut self) -> &mut Option<T> { (**self).pull() }
}

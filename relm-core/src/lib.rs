/*
 * Copyright (c) 2017 Boucher, Antoni <bouanto@zoho.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

//! Core primitive types for relm.
//!
//! The primary type is `EventStream`.

#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results,
)]

extern crate futures;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Error;
use std::rc::Rc;

use futures::{Async, Poll, Stream};
use futures::task::{self, Task};

/// A lock is used to temporarily stop emitting messages.
#[must_use]
pub struct Lock<MSG> {
    stream: Rc<RefCell<_EventStream<MSG>>>,
}

impl<MSG> Drop for Lock<MSG> {
    fn drop(&mut self) {
        self.stream.borrow_mut().locked = false;
    }
}

struct _EventStream<MSG> {
    events: VecDeque<MSG>,
    locked: bool,
    observers: Vec<Rc<Fn(&MSG)>>,
    task: Option<Task>,
    terminated: bool,
}

/// A stream of messages to be used for widget/signal communication and inter-widget communication.
pub struct EventStream<MSG> {
    stream: Rc<RefCell<_EventStream<MSG>>>,
}

impl<MSG> Clone for EventStream<MSG> {
    fn clone(&self) -> Self {
        EventStream {
            stream: self.stream.clone(),
        }
    }
}

impl<MSG> EventStream<MSG> {
    /// Create a new event stream.
    pub fn new() -> Self {
        EventStream {
            stream: Rc::new(RefCell::new(_EventStream {
                events: VecDeque::new(),
                locked: false,
                observers: vec![],
                task: None,
                terminated: false,
            })),
        }
    }

    /// Close the event stream, i.e. stop processing messages.
    pub fn close(&self) -> Result<(), Error> {
        let mut stream = self.stream.borrow_mut();
        stream.terminated = true;
        // TODO: document why it is needed.
        if let Some(ref task) = stream.task {
            task.notify();
        }
        Ok(())
    }

    /// Send the `event` message to the stream and the observers.
    pub fn emit(&self, event: MSG) {
        if !self.stream.borrow().locked {
            if let Some(ref task) = self.stream.borrow().task {
                task.notify();
            }

            let len = self.stream.borrow().observers.len();
            for i in 0..len {
                let observer = self.stream.borrow().observers[i].clone();
                observer(&event);
            }

            self.stream.borrow_mut().events.push_back(event);
        }
    }

    fn get_event(&self) -> Option<MSG> {
        self.stream.borrow_mut().events.pop_front()
    }

    /// Lock the stream (don't emit message) until the `Lock` goes out of scope.
    pub fn lock(&self) -> Lock<MSG> {
        self.stream.borrow_mut().locked = true;
        Lock {
            stream: self.stream.clone(),
        }
    }

    fn is_terminated(&self) -> bool {
        let stream = self.stream.borrow();
        stream.terminated
    }

    /// Add an observer to the event stream.
    /// This callback will be called every time a message is emmited.
    pub fn observe<CALLBACK: Fn(&MSG) + 'static>(&self, callback: CALLBACK) {
        self.stream.borrow_mut().observers.push(Rc::new(callback));
    }
}

impl<MSG: 'static> Stream for EventStream<MSG> {
    type Item = MSG;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_terminated() {
            Ok(Async::Ready(None))
        }
        else {
            match self.get_event() {
                Some(event) => {
                    let mut stream = self.stream.borrow_mut();
                    stream.task = None;
                    Ok(Async::Ready(Some(event)))
                },
                None => {
                    let mut stream = self.stream.borrow_mut();
                    stream.task = Some(task::current());
                    Ok(Async::NotReady)
                },
            }
        }
    }
}

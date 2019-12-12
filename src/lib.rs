#![feature(weak_into_raw)]

mod channel;
mod oneshot;
mod executor;
pub mod action;

use std::io;
use std::rc::{ Rc, Weak };
use std::cell::RefCell;
use std::marker::PhantomData;
use io_uring::{ squeue, cqueue, opcode, IoUring };
use crate::channel::{ Channel, Sender };
use crate::action::{ CompletionEntry, Action };


pub struct Proactor<C: Channel<CompletionEntry>> {
    timeout: Box<opcode::Timespec>,
    ring: Rc<RefCell<IoUring>>,
    _mark: PhantomData<C>
}

pub struct Handle<C: Channel<CompletionEntry>> {
    ring: Weak<RefCell<IoUring>>,
    _mark: PhantomData<C>
}

impl<C: Channel<CompletionEntry>> Handle<C> {
    pub fn submit<A: Action<C>>(&self, action: A) -> io::Result<()> {
        if let Some(ring) = self.ring.upgrade() {
            unsafe {
                let (sender, entry) = action.build_request();

                // TODO
                let entry = entry.user_data(sender.into_raw() as _);

                let mut ring = ring.borrow_mut();

                let ret = {
                    let mut sq = ring.submission().available();
                    sq.push(entry)
                };
                if let Err(entry) = ret {
                    // TODO
                    return Err(io::ErrorKind::WouldBlock.into());
                }
            }

            Ok(())
        } else {
            Err(io::ErrorKind::NotConnected.into())
        }
    }
}

impl<C: Channel<CompletionEntry>> Proactor<C> {
    pub fn complete(&mut self) -> io::Result<()> {
        let mut ring = self.ring.borrow_mut();

        ring.submit_and_wait(1)?;

        for entry in ring.completion().available() {
            unsafe {
                let sender = oneshot::Sender::from_raw(entry.user_data() as _);
                let _ = sender.send(entry);
            }
        }

        Ok(())
    }
}

use futures::Stream;

pub mod source;

mod ffi;

#[derive(Debug)]
pub struct Message {
    pub sender: String,
    pub content: Option<String>,
    pub donated: Option<u64>,
}

pub trait MessageStream: Stream<Item = Message> + Unpin {}

impl<T> MessageStream for T where T: Stream<Item = Message> + Unpin {}

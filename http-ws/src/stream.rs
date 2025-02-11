use core::{
    fmt,
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};

use std::error;

use bytes::{Bytes, BytesMut};
use futures_core::stream::Stream;
use pin_project_lite::pin_project;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use super::{
    codec::{Codec, Message},
    error::ProtocolError,
};

pin_project! {
    /// Decode `S` type into Stream of websocket [Message].
    /// `S` type must impl `Stream` trait and output `Result<T, E>` as `Stream::Item`
    /// where `T` type impl `AsRef<[u8]>` trait. (`&[u8]` is needed for parsing messages)
    pub struct DecodeStream<S, E> {
        #[pin]
        stream: Option<S>,
        buf: BytesMut,
        codec: Codec,
        err: Option<DecodeError<E>>
    }
}

impl<S, T, E> DecodeStream<S, E>
where
    S: Stream<Item = Result<T, E>>,
    T: AsRef<[u8]>,
{
    pub fn new(stream: S) -> Self {
        Self::with_codec(stream, Codec::new())
    }

    pub fn with_codec(stream: S, codec: Codec) -> Self {
        Self {
            stream: Some(stream),
            buf: BytesMut::new(),
            codec,
            err: None,
        }
    }

    /// Make an [EncodeStream] from current DecodeStream.
    ///
    /// This API is to share the same codec for both decode and encode stream.
    pub fn encode_stream(&self) -> (EncodeSender, EncodeStream) {
        let codec = self.codec.duplicate();
        EncodeStream::new(codec)
    }
}

pub enum DecodeError<E> {
    Protocol(ProtocolError),
    Stream(E),
}

impl<E> fmt::Debug for DecodeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Protocol(ref e) => fmt::Debug::fmt(e, f),
            Self::Stream(..) => f.write_str("Input Stream error"),
        }
    }
}

impl<E> fmt::Display for DecodeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Protocol(ref e) => fmt::Debug::fmt(e, f),
            Self::Stream(..) => f.write_str("Input Stream error"),
        }
    }
}

impl<E> std::error::Error for DecodeError<E> {}

impl<E> From<ProtocolError> for DecodeError<E> {
    fn from(e: ProtocolError) -> Self {
        Self::Protocol(e)
    }
}

impl<S, T, E> Stream for DecodeStream<S, E>
where
    S: Stream<Item = Result<T, E>>,
    T: AsRef<[u8]>,
{
    type Item = Result<Message, DecodeError<E>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        while let Some(stream) = this.stream.as_mut().as_pin_mut() {
            match stream.poll_next(cx) {
                Poll::Ready(Some(Ok(item))) => {
                    this.buf.extend_from_slice(item.as_ref());
                    if this.buf.len() > this.codec.max_size() {
                        break;
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    *this.err = Some(DecodeError::Stream(e));
                    this.stream.set(None);
                }
                Poll::Ready(None) => this.stream.set(None),
                Poll::Pending => break,
            }
        }

        match this.codec.decode(this.buf)? {
            Some(msg) => Poll::Ready(Some(Ok(msg))),
            None => {
                if this.stream.is_none() {
                    Poll::Ready(this.err.take().map(Err))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

/// Encode a stream of [Message] into [Bytes].
pub struct EncodeStream {
    codec: Codec,
    buf: BytesMut,
    rx: Receiver<Message>,
}

impl EncodeStream {
    /// Construct new stream with given codec.
    #[inline]
    pub fn new(codec: Codec) -> (EncodeSender, Self) {
        let cap = codec.capacity();
        let (tx, rx) = channel(cap);

        let stream = EncodeStream {
            codec,
            buf: BytesMut::new(),
            rx,
        };

        (EncodeSender::new(tx), stream)
    }
}

impl Stream for EncodeStream {
    type Item = Result<Bytes, ProtocolError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match ready!(this.rx.poll_recv(cx)) {
            Some(msg) => {
                this.codec.encode(msg, &mut this.buf)?;
                Poll::Ready(Some(Ok(this.buf.split().freeze())))
            }
            None => Poll::Ready(None),
        }
    }
}

/// channel sender that can add [Message] to [EncodeStream]. new message would be encoded and sent
/// to client asynchronously.
#[derive(Debug, Clone)]
pub struct EncodeSender(Sender<Message>);

#[derive(Debug)]
pub struct SendError(Message);

impl SendError {
    pub fn into_inner(self) -> Message {
        self.0
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("EncodeStream already finished")
    }
}

impl error::Error for SendError {}

impl EncodeSender {
    fn new(tx: Sender<Message>) -> Self {
        Self(tx)
    }

    /// add [Message] to [EncodeStream].
    #[inline]
    pub async fn send(&self, msg: Message) -> Result<(), SendError> {
        self.0.send(msg).await.map_err(|e| SendError(e.0))
    }

    /// add [Message::Text] variant to [EncodeStream].
    #[inline]
    pub fn text(&self, txt: impl Into<String>) -> impl Future<Output = Result<(), SendError>> + '_ {
        self.send(Message::Text(Bytes::from(txt.into())))
    }

    /// add [Message::Binary] variant to [EncodeStream].
    #[inline]
    pub fn binary(&self, bin: impl Into<Bytes>) -> impl Future<Output = Result<(), SendError>> + '_ {
        self.send(Message::Binary(bin.into()))
    }
}

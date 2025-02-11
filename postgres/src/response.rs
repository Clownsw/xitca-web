use std::{
    future::poll_fn,
    task::{ready, Context, Poll},
};

use postgres_protocol::message::backend;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xitca_io::bytes::BytesMut;

use super::error::{unexpected_eof_err, Error};

pub struct Response {
    rx: ResponseReceiver,
    buf: BytesMut,
}

pub type ResponseSender = UnboundedSender<BytesMut>;

pub type ResponseReceiver = UnboundedReceiver<BytesMut>;

impl Response {
    pub(crate) fn new(rx: UnboundedReceiver<BytesMut>) -> Self {
        Self {
            rx,
            buf: BytesMut::new(),
        }
    }

    pub(crate) async fn recv(&mut self) -> Result<backend::Message, Error> {
        poll_fn(|cx| self.poll_recv(cx)).await
    }

    pub(crate) fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Result<backend::Message, Error>> {
        if self.buf.is_empty() {
            match ready!(self.rx.poll_recv(cx)) {
                Some(buf) => self.buf = buf,
                None => return Poll::Ready(Err(unexpected_eof_err())),
            }
        }

        match backend::Message::parse(&mut self.buf)? {
            // TODO: error response.
            Some(backend::Message::ErrorResponse(_body)) => Poll::Ready(Err(Error::ToDo)),
            Some(msg) => Poll::Ready(Ok(msg)),
            // TODO: partial response.
            None => Poll::Ready(Err(Error::ToDo)),
        }
    }
}

pub enum ResponseMessage {
    Normal { buf: BytesMut, complete: bool },
    Async(backend::Message),
}

impl ResponseMessage {
    pub(crate) fn try_from_buf(buf: &mut BytesMut) -> Result<Option<Self>, Error> {
        let mut idx = 0;
        let mut complete = false;

        while let Some(header) = backend::Header::parse(&buf[idx..])? {
            let len = header.len() as usize + 1;
            if buf[idx..].len() < len {
                break;
            }

            match header.tag() {
                backend::NOTICE_RESPONSE_TAG | backend::NOTIFICATION_RESPONSE_TAG | backend::PARAMETER_STATUS_TAG => {
                    if idx == 0 {
                        let message = backend::Message::parse(buf)?.unwrap();
                        return Ok(Some(ResponseMessage::Async(message)));
                    } else {
                        break;
                    }
                }
                _ => {}
            }

            idx += len;

            if header.tag() == backend::READY_FOR_QUERY_TAG {
                complete = true;
                break;
            }
        }

        if idx == 0 {
            Ok(None)
        } else {
            Ok(Some(ResponseMessage::Normal {
                buf: buf.split_to(idx),
                complete,
            }))
        }
    }
}

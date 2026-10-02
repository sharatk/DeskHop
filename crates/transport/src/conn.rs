//! One task per connection: `Hello` and frames on the control stream. Every
//! decision belongs to the node; this task only moves bytes (design D1, D4).

use std::time::Duration;

use pairing::ConnId;
use proto::CloseReason;
use proto::frame::{self, Decoded};
use proto::hello::Hello;
use proto::session::{ControlEvent, Session};
use tokio::sync::mpsc;

use crate::node::Internal;

/// What the node asks a connection task to do.
#[derive(Debug)]
pub(crate) enum Out {
    /// Write this encoded frame on the control stream.
    Frame(Vec<u8>),
    /// Let what was written reach the peer, then close with this reason.
    Close(CloseReason),
}

/// How a connection ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// This side closed it.
    Local,
    /// The peer closed it with this reason (`None`: a code this release does
    /// not know).
    Remote(Option<CloseReason>),
    /// Nothing heard within the idle timeout.
    TimedOut,
    /// A transport error, a reset, or the endpoint going away.
    Failed,
}

impl Cause {
    pub(crate) fn from_error(e: &quinn::ConnectionError) -> Self {
        match e {
            quinn::ConnectionError::ApplicationClosed(c) => {
                Self::Remote(CloseReason::from_code(c.error_code.into_inner()))
            }
            quinn::ConnectionError::TimedOut => Self::TimedOut,
            quinn::ConnectionError::LocallyClosed => Self::Local,
            _ => Self::Failed,
        }
    }

    /// The peer's close reason, if it gave one this release knows.
    pub(crate) fn reason(self) -> Option<CloseReason> {
        match self {
            Self::Remote(reason) => reason,
            _ => None,
        }
    }
}

/// Closes `connection` at once with `reason`.
pub(crate) fn close(connection: &quinn::Connection, reason: CloseReason) {
    // Close codes are small; they always fit a VarInt.
    let code = quinn::VarInt::from_u64(reason.code()).unwrap_or(quinn::VarInt::from_u32(1));
    connection.close(code, b"");
}

/// How long a close waits for written frames to be acknowledged.
const FLUSH: Duration = Duration::from_millis(500);

/// How long after the peer finishes the control stream its close may take.
const LINGER: Duration = Duration::from_secs(2);

/// Runs one connection until it closes, then reports how.
pub(crate) async fn run(
    id: ConnId,
    connection: quinn::Connection,
    dialer: bool,
    mut out: mpsc::UnboundedReceiver<Out>,
    node: mpsc::UnboundedSender<Internal>,
) {
    // A peer may never open the control stream; the node can still close it.
    let open = async {
        if dialer {
            connection.open_bi().await
        } else {
            connection.accept_bi().await
        }
    };
    let streams = tokio::select! {
        streams = open => streams,
        next = out.recv() => {
            let reason = match next {
                Some(Out::Close(reason)) => reason,
                _ => CloseReason::Normal,
            };
            close(&connection, reason);
            let e = connection.closed().await;
            let _ = node.send(Internal::Closed(id, Cause::from_error(&e)));
            return;
        }
    };
    let (mut send, mut recv) = match streams {
        Ok(streams) => streams,
        Err(e) => {
            let _ = node.send(Internal::Closed(id, Cause::from_error(&e)));
            return;
        }
    };
    let mut session = Session::new(Hello::this_release());
    let mut hello = Vec::new();
    // `Hello` is 4 bytes, far under the frame limit.
    let _ = session.local_hello().encode_frame(&mut hello);
    let _ = send.write_all(&hello).await;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        tokio::select! {
            read = recv.read(&mut chunk) => match read {
                Ok(Some(n)) => {
                    buf.extend_from_slice(chunk.get(..n).unwrap_or_default());
                    if let Err(reason) = drain(&mut buf, &mut session, id, &node) {
                        close(&connection, reason);
                        break;
                    }
                }
                // A peer finishes the control stream just before closing the
                // connection; wait for its close reason. One that keeps the
                // connection open without the stream breaks the protocol.
                Ok(None) => {
                    if let Ok(e) = tokio::time::timeout(LINGER, connection.closed()).await {
                        let _ = node.send(Internal::Closed(id, Cause::from_error(&e)));
                        return;
                    }
                    close(&connection, CloseReason::ProtocolError);
                    break;
                }
                Err(_) => break,
            },
            next = out.recv() => match next {
                Some(Out::Frame(bytes)) => {
                    if send.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Some(Out::Close(reason)) => {
                    let _ = send.finish();
                    let _ = tokio::time::timeout(FLUSH, send.stopped()).await;
                    close(&connection, reason);
                    break;
                }
                None => {
                    close(&connection, CloseReason::Normal);
                    break;
                }
            },
            e = connection.closed() => {
                let _ = node.send(Internal::Closed(id, Cause::from_error(&e)));
                return;
            }
        }
    }
    let e = connection.closed().await;
    let _ = node.send(Internal::Closed(id, Cause::from_error(&e)));
}

/// Decodes every complete frame in `buf` and passes it on; keeps the rest.
fn drain(
    buf: &mut Vec<u8>,
    session: &mut Session,
    id: ConnId,
    node: &mpsc::UnboundedSender<Internal>,
) -> Result<(), CloseReason> {
    let mut start = 0;
    loop {
        let rest = buf.get(start..).unwrap_or_default();
        match frame::decode(rest) {
            Ok(Decoded::NeedMore) => break,
            Ok(Decoded::Frame { frame, consumed }) => {
                match session.on_control_frame(frame) {
                    Ok(ControlEvent::Agreed(_)) => {
                        let _ = node.send(Internal::Ready(id));
                    }
                    Ok(ControlEvent::Message(m)) => {
                        let _ = node.send(Internal::Message(id, m.info.ty, m.payload.to_vec()));
                    }
                    Err(e) => return Err(e.close_reason()),
                }
                start += consumed;
            }
            Err(_) => return Err(CloseReason::ProtocolError),
        }
    }
    buf.drain(..start);
    Ok(())
}

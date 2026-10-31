//! Rateless UDP file transfer on top of [`fountain_transfer_core`].

#![cfg_attr(
    not(any(feature = "raptor-q", feature = "raptor-10")),
    allow(unreachable_code, unused_variables, unused_mut)
)]

pub mod net;
pub mod progress;
pub mod protocol;
pub mod receiver;
pub mod sender;
pub mod session;

pub use net::{NetError, bind_udp, send_message};
pub use progress::{IgnoredDatagramReason, ProgressCallback, ProgressEvent};
pub use protocol::{
    ProtocolError, WireMessage, codec_kind_from_cli, decode_message, encode_message,
    session_meta_from_spec,
};
pub use receiver::{
    RecvConfig, RecvOutcome, receive_object, receive_object_with_progress,
    receive_object_with_socket, receive_object_with_socket_with_progress, receive_to_file,
    receive_to_file_with_progress,
};
pub use sender::{
    SendConfig, send_file, send_file_with_progress, send_object, send_object_with_progress,
    send_object_with_socket, send_object_with_socket_with_progress,
};
pub use session::{SessionParams, random_session_id};

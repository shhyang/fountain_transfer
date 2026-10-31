//! Rateless UDP file transfer on top of [`fountain_transfer_core`].

#![cfg_attr(
    not(any(feature = "raptor-q", feature = "raptor-10")),
    allow(unreachable_code, unused_variables, unused_mut)
)]

pub mod net;
pub mod protocol;
pub mod receiver;
pub mod sender;
pub mod session;

pub use net::{NetError, bind_udp, send_message};
pub use protocol::{
    ProtocolError, WireMessage, codec_kind_from_cli, decode_message, encode_message,
    session_meta_from_spec,
};
pub use receiver::{
    RecvConfig, RecvOutcome, receive_object, receive_object_with_socket, receive_to_file,
};
pub use sender::{SendConfig, send_file, send_object, send_object_with_socket};
pub use session::{SessionParams, random_session_id};

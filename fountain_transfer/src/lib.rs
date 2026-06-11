//! Rateless UDP file transfer on top of [`fountain_transfer_core`].

pub mod net;
pub mod protocol;
pub mod receiver;
pub mod sender;
pub mod session;

pub use net::{bind_udp, send_message, NetError};
pub use protocol::{
    codec_kind_from_cli, decode_message, encode_message, session_meta_from_spec, ProtocolError,
    WireMessage,
};
pub use receiver::{
    receive_object, receive_object_with_socket, receive_to_file, RecvConfig, RecvOutcome,
};
pub use sender::{send_file, send_object, send_object_with_socket, SendConfig};
pub use session::{random_session_id, SessionParams};

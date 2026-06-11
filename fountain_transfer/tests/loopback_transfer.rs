//! End-to-end loopback UDP transfer with SHA-256 verification.

use std::time::Duration;

use fountain_transfer::{
    receive_object_with_socket, send_object_with_socket, RecvConfig, SendConfig, SessionParams,
};
use fountain_transfer_core::CodecKind;
use sha2::{Digest, Sha256};
use tokio::net::UdpSocket;

fn sample_object(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

async fn loopback_transfer(codec: CodecKind, object_len: usize, symbol_size: usize) {
    let object = sample_object(object_len);
    let expected_hash = sha256(&object);
    let session_id = 0xDEAD_BEEF_CAFE_u64;

    let recv_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind recv");
    let listen_addr = recv_socket.local_addr().expect("local addr");

    let session = SessionParams::from_file_and_cli(
        session_id,
        object.len(),
        symbol_size,
        codec,
    )
    .expect("session params");

    let send_config = SendConfig {
        repair_count: 128,
        repair_rounds: 1,
        inter_packet_delay: Duration::ZERO,
    };
    let recv_config = RecvConfig {
        recv_timeout: Duration::from_secs(10),
        expected_session_id: Some(session_id),
    };

    let object_clone = object.clone();
    let session_clone = session.clone();
    let (outcome, send_result) = tokio::join!(
        receive_object_with_socket(&recv_socket, &recv_config),
        async move {
            let send_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind send");
            send_object_with_socket(
                &send_socket,
                &object_clone,
                listen_addr,
                &session_clone,
                &send_config,
            )
            .await
        }
    );
    let outcome = outcome.expect("recv");
    send_result.expect("send");

    assert_eq!(sha256(&outcome.object), expected_hash);
    assert_eq!(outcome.object, object);
}

fn run_loopback_test(codec: CodecKind, object_len: usize, symbol_size: usize) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(loopback_transfer(codec, object_len, symbol_size));
        })
        .expect("spawn test thread")
        .join()
        .expect("join test thread");
}

#[test]
fn loopback_raptor_q() {
    run_loopback_test(CodecKind::RaptorQ, 199, 10);
}

#[test]
fn loopback_raptor_10() {
    run_loopback_test(CodecKind::Raptor10, 77, 8);
}

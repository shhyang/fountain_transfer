//! End-to-end loopback UDP transfer with SHA-256 verification.

#![cfg(any(feature = "raptor-q", feature = "raptor-10"))]

use std::future::Future;
use std::time::Duration;

use fountain_transfer::protocol::{packet_message, session_meta_from_spec};
use fountain_transfer::{
    IgnoredDatagramReason, ProgressEvent, RecvConfig, SendConfig, SessionParams,
    receive_object_with_socket, receive_object_with_socket_with_progress, send_message,
    send_object_with_socket, send_object_with_socket_with_progress,
};
use fountain_transfer_core::{CodecKind, TransferEncoder, TransferEncoderImpl};
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

    let session = SessionParams::from_file_and_cli(session_id, object.len(), symbol_size, codec)
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
    run_async_test(move || loopback_transfer(codec, object_len, symbol_size));
}

fn run_async_test<F, Fut>(test: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(test());
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

#[test]
fn loopback_reports_sender_and_receiver_progress() {
    run_async_test(|| async {
        let object = sample_object(199);
        let session_id = 0xABBA_1234;
        let recv_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind recv");
        let listen_addr = recv_socket.local_addr().expect("local addr");
        let session =
            SessionParams::from_file_and_cli(session_id, object.len(), 10, CodecKind::RaptorQ)
                .expect("session");
        let send_config = SendConfig {
            repair_count: 8,
            repair_rounds: 1,
            inter_packet_delay: Duration::ZERO,
        };
        let recv_config = RecvConfig {
            recv_timeout: Duration::from_secs(10),
            expected_session_id: None,
        };

        let mut send_events = Vec::new();
        let mut recv_events = Vec::new();
        let send = async {
            let send_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind send");
            let mut on_progress = |event| send_events.push(event);
            send_object_with_socket_with_progress(
                &send_socket,
                &object,
                listen_addr,
                &session,
                &send_config,
                &mut on_progress,
            )
            .await
        };
        let receive = async {
            let mut on_progress = |event| recv_events.push(event);
            receive_object_with_socket_with_progress(&recv_socket, &recv_config, &mut on_progress)
                .await
        };
        let (send_result, receive_result) = tokio::join!(send, receive);
        send_result.expect("send");
        let outcome = receive_result.expect("receive");
        assert_eq!(outcome.object, object);

        assert!(matches!(
            send_events.first(),
            Some(ProgressEvent::SenderStarted {
                session_id: actual,
                ..
            }) if *actual == session_id
        ));
        assert!(
            send_events.iter().any(|event| matches!(
                event,
                ProgressEvent::SenderSourcePacketSent { sent: 1, .. }
            ))
        );
        assert!(
            send_events.iter().any(|event| matches!(
                event,
                ProgressEvent::SenderRepairPacketSent { sent: 1, .. }
            ))
        );
        let Some(ProgressEvent::SenderFinished {
            source_packets,
            repair_packets,
            bytes_sent,
            planned_bytes,
        }) = send_events.last()
        else {
            panic!("missing sender finish event");
        };
        assert!(*source_packets > 0);
        assert_eq!(*repair_packets, 8);
        assert_eq!(bytes_sent, planned_bytes);

        assert!(matches!(
            recv_events.first(),
            Some(ProgressEvent::ReceiverSessionEstablished {
                session_id: actual,
                ..
            }) if *actual == session_id
        ));
        assert!(recv_events.iter().any(|event| matches!(
            event,
            ProgressEvent::ReceiverPacketAccepted {
                packets_accepted: 1,
                decode_percent: 0..=99,
                ..
            }
        )));
        assert!(matches!(
            recv_events.last(),
            Some(ProgressEvent::ReceiverDecodeComplete {
                decode_percent: 100,
                ..
            })
        ));
    });
}

#[test]
fn corrupt_datagram_is_skipped_and_receive_continues() {
    run_async_test(|| async {
        let object = sample_object(77);
        let session_id = 0xC0DE_0001;
        let recv_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind recv");
        let listen_addr = recv_socket.local_addr().expect("local addr");
        let sender = UdpSocket::bind("127.0.0.1:0").await.expect("bind sender");
        sender
            .send_to(&[0xff; 32], listen_addr)
            .await
            .expect("send corrupt datagram");

        let session =
            SessionParams::from_file_and_cli(session_id, object.len(), 8, CodecKind::Raptor10)
                .expect("session");
        let config = RecvConfig {
            recv_timeout: Duration::from_secs(10),
            expected_session_id: None,
        };
        let send_config = SendConfig {
            repair_count: 8,
            repair_rounds: 1,
            inter_packet_delay: Duration::ZERO,
        };
        let send = async {
            send_object_with_socket(&sender, &object, listen_addr, &session, &send_config).await
        };
        let receive = receive_object_with_socket(&recv_socket, &config);
        let (send_result, receive_result) = tokio::join!(send, receive);
        send_result.expect("send");
        assert_eq!(
            receive_result.expect("receive after corrupt").object,
            object
        );
    });
}

#[test]
fn foreign_session_after_establishment_is_ignored() {
    run_async_test(|| async {
        let object = sample_object(80);
        let session_id = 0x1111_2222;
        let foreign_id = 0x9999_AAAA;
        let recv_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind recv");
        let listen_addr = recv_socket.local_addr().expect("local addr");
        let sender = UdpSocket::bind("127.0.0.1:0").await.expect("bind sender");
        let session =
            SessionParams::from_file_and_cli(session_id, object.len(), 8, CodecKind::Raptor10)
                .expect("session");
        let foreign =
            SessionParams::from_file_and_cli(foreign_id, object.len(), 8, CodecKind::Raptor10)
                .expect("foreign session");
        let mut encoder = TransferEncoderImpl::new(session.spec.clone(), &object).expect("encoder");
        let source_packets = encoder.source_packets();

        send_message(
            &sender,
            listen_addr,
            &session_meta_from_spec(session_id, &session.spec),
        )
        .await
        .expect("legit meta");
        send_message(
            &sender,
            listen_addr,
            &session_meta_from_spec(foreign_id, &foreign.spec),
        )
        .await
        .expect("foreign meta");
        send_message(
            &sender,
            listen_addr,
            &packet_message(foreign_id, &source_packets[0]),
        )
        .await
        .expect("foreign packet");
        for packet in &source_packets {
            send_message(&sender, listen_addr, &packet_message(session_id, packet))
                .await
                .expect("legit packet");
        }

        let config = RecvConfig {
            recv_timeout: Duration::from_secs(10),
            expected_session_id: None,
        };
        let mut events = Vec::new();
        let mut on_progress = |event| events.push(event);
        let outcome =
            receive_object_with_socket_with_progress(&recv_socket, &config, &mut on_progress)
                .await
                .expect("receive despite foreign traffic");
        assert_eq!(outcome.object, object);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ProgressEvent::ReceiverSessionEstablished { .. }))
                .count(),
            1
        );
        assert!(events.iter().any(|event| matches!(
            event,
            ProgressEvent::ReceiverDatagramIgnored {
                reason: IgnoredDatagramReason::ForeignSession,
                ..
            }
        )));
    });
}

#[test]
fn same_session_metadata_resets_decoder_for_retransmission() {
    run_async_test(|| async {
        let good_object = vec![0x5a; 80];
        let stale_object = vec![0x00; 80];
        let session_id = 0x1234_5678;
        let recv_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind recv");
        let listen_addr = recv_socket.local_addr().expect("local addr");
        let sender = UdpSocket::bind("127.0.0.1:0").await.expect("bind sender");
        let session =
            SessionParams::from_file_and_cli(session_id, good_object.len(), 8, CodecKind::Raptor10)
                .expect("session");
        let mut stale_encoder =
            TransferEncoderImpl::new(session.spec.clone(), &stale_object).expect("stale encoder");
        let stale_packets = stale_encoder.source_packets();
        let mut good_encoder =
            TransferEncoderImpl::new(session.spec.clone(), &good_object).expect("good encoder");
        let good_packets = good_encoder.source_packets();

        send_message(
            &sender,
            listen_addr,
            &session_meta_from_spec(session_id, &session.spec),
        )
        .await
        .expect("first meta");
        send_message(
            &sender,
            listen_addr,
            &packet_message(session_id, &stale_packets[0]),
        )
        .await
        .expect("stale packet");
        send_message(
            &sender,
            listen_addr,
            &session_meta_from_spec(session_id, &session.spec),
        )
        .await
        .expect("restart meta");
        for packet in &good_packets {
            send_message(&sender, listen_addr, &packet_message(session_id, packet))
                .await
                .expect("retransmitted packet");
        }

        let config = RecvConfig {
            recv_timeout: Duration::from_secs(10),
            expected_session_id: None,
        };
        let mut events = Vec::new();
        let mut on_progress = |event| events.push(event);
        let outcome =
            receive_object_with_socket_with_progress(&recv_socket, &config, &mut on_progress)
                .await
                .expect("receive after restart");
        assert_eq!(outcome.object, good_object);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ProgressEvent::ReceiverSessionEstablished { .. }))
                .count(),
            2
        );
        assert!(events.iter().any(|event| matches!(
            event,
            ProgressEvent::ReceiverPacketAccepted {
                packets_accepted: 1,
                ..
            }
        )));
    });
}

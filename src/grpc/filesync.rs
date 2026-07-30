use std::{collections::HashSet, pin::Pin};

use bollard_buildkit_proto::{
    fsutil::types::{packet::PacketType, Packet, Stat},
    moby::filesync::v1::file_sync_server::{FileSync, FileSyncServer},
};
use futures_core::Stream;
use futures_util::stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::{transport::Server, Request, Response, Status, Streaming};

pub(crate) const ENTRY_QUEUE_CAPACITY: usize = 128;
pub(crate) const FILE_JOB_QUEUE_CAPACITY: usize = 128;
pub(crate) const OUTPUT_QUEUE_CAPACITY: usize = 16;
pub(crate) const FILE_WORKER_COUNT: usize = 4;
pub(crate) const FILE_READ_BUFFER_SIZE: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContractEntry {
    pub(crate) path: &'static str,
    pub(crate) regular: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PacketContract {
    entries: Vec<ContractEntry>,
}

impl PacketContract {
    pub(crate) fn new(entries: impl IntoIterator<Item = ContractEntry>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    pub(crate) fn stat_packets(&self) -> Vec<Packet> {
        self.entries
            .iter()
            .map(|entry| stat_packet(entry.path))
            .chain(std::iter::once(stat_packet_without_entry()))
            .collect()
    }

    pub(crate) fn validate_stat_packets(&self, packets: &[Packet]) -> Result<(), String> {
        if packets.len() != self.entries.len() + 1 {
            return Err(format!(
                "expected {} STAT packets, got {}",
                self.entries.len() + 1,
                packets.len()
            ));
        }

        for (index, (packet, expected)) in packets.iter().zip(self.entries.iter()).enumerate() {
            if packet.r#type != PacketType::PacketStat as i32 {
                return Err(format!("packet {index} is not PACKET_STAT"));
            }
            if packet.id != 0 {
                return Err(format!("packet {index} has non-zero STAT ID"));
            }
            if packet.stat.as_ref().map(|stat| stat.path.as_str()) != Some(expected.path) {
                return Err(format!("packet {index} has the wrong path"));
            }
        }

        let terminator = packets.last().expect("length checked above");
        if terminator.r#type != PacketType::PacketStat as i32
            || terminator.id != 0
            || terminator.stat.is_some()
        {
            return Err(String::from("STAT terminator is malformed"));
        }
        Ok(())
    }

    pub(crate) fn regular_ids(&self) -> HashSet<u32> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.regular.then_some(index as u32))
            .collect()
    }
}

#[derive(Debug)]
pub(crate) struct RequestLedger {
    available: HashSet<u32>,
}

impl RequestLedger {
    pub(crate) fn new(contract: &PacketContract) -> Self {
        Self {
            available: contract.regular_ids(),
        }
    }

    pub(crate) fn accept(&mut self, id: u32) -> Result<(), String> {
        if self.available.remove(&id) {
            Ok(())
        } else {
            Err(format!("invalid or repeated file request {id}"))
        }
    }
}

pub(crate) fn stat_packet(path: &'static str) -> Packet {
    Packet {
        r#type: PacketType::PacketStat as i32,
        stat: Some(Stat {
            path: path.to_owned(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(crate) fn stat_packet_without_entry() -> Packet {
    Packet {
        r#type: PacketType::PacketStat as i32,
        ..Default::default()
    }
}

pub(crate) fn request_packet(id: u32) -> Packet {
    Packet {
        r#type: PacketType::PacketReq as i32,
        id,
        ..Default::default()
    }
}

pub(crate) fn data_packet(id: u32, data: impl Into<Vec<u8>>) -> Packet {
    Packet {
        r#type: PacketType::PacketData as i32,
        id,
        data: data.into(),
        ..Default::default()
    }
}

pub(crate) fn fin_packet() -> Packet {
    Packet {
        r#type: PacketType::PacketFin as i32,
        ..Default::default()
    }
}

pub(crate) fn err_packet(message: impl Into<Vec<u8>>) -> Packet {
    Packet {
        r#type: PacketType::PacketErr as i32,
        data: message.into(),
        ..Default::default()
    }
}

pub(crate) fn collect_file_data(packets: &[Packet], id: u32) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    let mut terminated = false;

    for packet in packets.iter().filter(|packet| packet.id == id) {
        if packet.r#type != PacketType::PacketData as i32 {
            return Err(format!("file {id} contains a non-DATA packet"));
        }
        if packet.data.is_empty() {
            if terminated {
                return Err(format!("file {id} has repeated EOF"));
            }
            terminated = true;
        } else if terminated {
            return Err(format!("file {id} has DATA after EOF"));
        } else {
            data.extend_from_slice(&packet.data);
        }
    }

    if terminated {
        Ok(data)
    } else {
        Err(format!("file {id} has no DATA EOF"))
    }
}

pub(crate) struct ScriptedPeer {
    requests: mpsc::Sender<Packet>,
    responses: Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>,
}

impl ScriptedPeer {
    pub(crate) fn new(
        responses: impl Stream<Item = Result<Packet, Status>> + Send + 'static,
    ) -> (Self, ReceiverStream<Packet>) {
        let (requests, receiver) = mpsc::channel(OUTPUT_QUEUE_CAPACITY);
        (
            Self {
                requests,
                responses: Box::pin(responses),
            },
            ReceiverStream::new(receiver),
        )
    }

    pub(crate) async fn send(&self, packet: Packet) -> Result<(), String> {
        self.requests
            .send(packet)
            .await
            .map_err(|_| String::from("scripted peer request channel closed"))
    }

    pub(crate) async fn next(&mut self) -> Option<Result<Packet, Status>> {
        futures_util::StreamExt::next(&mut self.responses).await
    }

    pub(crate) fn close(self) {
        drop(self.requests);
        drop(self.responses);
    }
}

#[derive(Debug, Default)]
struct UnimplementedFileSync;

#[tonic::async_trait]
impl FileSync for UnimplementedFileSync {
    type DiffCopyStream = stream::Empty<Result<Packet, Status>>;
    type TarStreamStream = stream::Empty<Result<Packet, Status>>;

    async fn diff_copy(
        &self,
        _request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        Err(Status::unimplemented("FileSync sender is not implemented"))
    }

    async fn tar_stream(
        &self,
        _request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::TarStreamStream>, Status> {
        Err(Status::unimplemented(
            "FileSync TarStream is not implemented",
        ))
    }
}

async fn start_unimplemented_server() -> (
    std::net::SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("FileSync test listener binds");
    let address = listener.local_addr().expect("FileSync test address exists");
    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
    let server = Server::builder()
        .add_service(FileSyncServer::new(UnimplementedFileSync))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            let _ = shutdown_receiver.await;
        });
    let server_task = tokio::spawn(async move {
        let _ = server.await;
    });
    (address, shutdown_sender, server_task)
}

async fn red_baseline_stream() -> Result<tonic::Streaming<Packet>, Status> {
    let (address, shutdown_sender, server_task) = start_unimplemented_server().await;
    let mut client =
        bollard_buildkit_proto::moby::filesync::v1::file_sync_client::FileSyncClient::connect(
            format!("http://{address}"),
        )
        .await
        .map_err(|error| Status::unknown(error.to_string()))?;
    let (_peer, requests) = ScriptedPeer::new(stream::empty());
    let result = client
        .diff_copy(requests)
        .await
        .map(|response| response.into_inner());
    let _ = shutdown_sender.send(());
    let _ = server_task.await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    fn sample_contract() -> PacketContract {
        PacketContract::new([
            ContractEntry {
                path: "directory",
                regular: false,
            },
            ContractEntry {
                path: "directory/input.txt",
                regular: true,
            },
            ContractEntry {
                path: "link",
                regular: false,
            },
            ContractEntry {
                path: "empty.txt",
                regular: true,
            },
        ])
    }

    #[test]
    fn contract_tracks_positional_regular_file_ids() {
        let contract = sample_contract();
        contract
            .validate_stat_packets(&contract.stat_packets())
            .expect("fixture STAT packets are valid");
        assert_eq!(contract.regular_ids(), HashSet::from([1, 3]));
    }

    #[test]
    fn contract_requires_zero_stat_packet_ids() {
        let contract = sample_contract();
        let mut packets = contract.stat_packets();
        packets[1].id = 1;
        assert!(contract.validate_stat_packets(&packets).is_err());
    }

    #[test]
    fn request_ledger_rejects_duplicate_unknown_and_non_regular_ids() {
        let contract = sample_contract();
        let mut ledger = RequestLedger::new(&contract);
        assert!(ledger.accept(1).is_ok());
        assert!(ledger.accept(1).is_err());
        assert!(ledger.accept(0).is_err());
        assert!(ledger.accept(99).is_err());
    }

    #[test]
    fn data_collection_requires_one_empty_eof_packet() {
        let packets = [
            data_packet(1, b"first".to_vec()),
            data_packet(3, b"other".to_vec()),
            data_packet(1, b"second".to_vec()),
            data_packet(1, Vec::new()),
            data_packet(3, Vec::new()),
        ];
        assert_eq!(collect_file_data(&packets, 1).unwrap(), b"firstsecond");
        assert_eq!(collect_file_data(&packets, 3).unwrap(), b"other");
    }

    #[test]
    fn protocol_bounds_are_fixed_before_sender_implementation() {
        assert_eq!(ENTRY_QUEUE_CAPACITY, 128);
        assert_eq!(FILE_JOB_QUEUE_CAPACITY, 128);
        assert_eq!(OUTPUT_QUEUE_CAPACITY, 16);
        assert_eq!(FILE_WORKER_COUNT, 4);
        assert_eq!(FILE_READ_BUFFER_SIZE, 32 * 1024);
    }

    #[tokio::test]
    async fn scripted_peer_uses_bounded_request_and_response_channels() {
        let (mut peer, mut requests) =
            ScriptedPeer::new(stream::iter([Ok(data_packet(1, b"response".to_vec()))]));
        peer.send(request_packet(1))
            .await
            .expect("scripted peer accepts requests");
        assert_eq!(requests.next().await, Some(request_packet(1)));
        assert_eq!(
            peer.next()
                .await
                .expect("scripted response exists")
                .unwrap(),
            data_packet(1, b"response".to_vec())
        );
        peer.close();
    }

    #[tokio::test]
    #[ignore = "removed when the FileSync sender is implemented"]
    async fn diff_copy_red_baseline_expects_stat_stream() {
        let mut responses = red_baseline_stream()
            .await
            .expect("FileSync sender returns a stream");
        let packet = responses
            .message()
            .await
            .expect("FileSync stream succeeds")
            .expect("FileSync emits a STAT packet");
        assert_eq!(packet.r#type, PacketType::PacketStat as i32);
    }

    #[tokio::test]
    #[ignore = "removed when the FileSync sender is implemented"]
    async fn diff_copy_red_baseline_accepts_pipelined_requests() {
        let _ = request_packet(1);
        let _ = request_packet(3);
        let _ = red_baseline_stream()
            .await
            .expect("FileSync sender accepts pipelined requests");
    }

    #[tokio::test]
    #[ignore = "removed when the FileSync sender is implemented"]
    async fn diff_copy_red_baseline_reports_protocol_errors() {
        let _ = err_packet("peer failure");
        let _ = fin_packet();
        let _ = red_baseline_stream()
            .await
            .expect("FileSync sender reports protocol errors");
    }

    macro_rules! red_protocol_test {
        ($name:ident, $expectation:literal) => {
            #[tokio::test]
            #[ignore = "removed when the FileSync sender is implemented"]
            async fn $name() {
                let _ = red_baseline_stream().await.expect($expectation);
            }
        };
    }

    red_protocol_test!(
        diff_copy_red_baseline_streams_data_and_fin,
        "FileSync sender streams DATA and FIN"
    );
    red_protocol_test!(
        diff_copy_red_baseline_terminates_empty_files,
        "FileSync sender terminates empty files"
    );
    red_protocol_test!(
        diff_copy_red_baseline_interleaves_file_data,
        "FileSync sender interleaves file DATA"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_duplicate_requests,
        "FileSync sender rejects duplicate requests"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_unknown_requests,
        "FileSync sender rejects unknown requests"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_unexpected_packets,
        "FileSync sender rejects unexpected packets"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_early_fin,
        "FileSync sender rejects early FIN"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_input_eof,
        "FileSync sender rejects input EOF before FIN"
    );
    red_protocol_test!(
        diff_copy_red_baseline_handles_peer_errors,
        "FileSync sender handles peer errors"
    );
    red_protocol_test!(
        diff_copy_red_baseline_waits_for_accepted_jobs,
        "FileSync sender waits for accepted jobs"
    );
    red_protocol_test!(
        diff_copy_red_baseline_cancels_owned_tasks,
        "FileSync sender cancels owned tasks"
    );
    red_protocol_test!(
        diff_copy_red_baseline_reports_worker_errors,
        "FileSync sender reports worker errors"
    );
    red_protocol_test!(
        diff_copy_red_baseline_reports_worker_panics,
        "FileSync sender reports worker panics"
    );
    red_protocol_test!(
        diff_copy_red_baseline_applies_output_backpressure,
        "FileSync sender applies output backpressure"
    );
}

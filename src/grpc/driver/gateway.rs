use std::{collections::HashMap, time::Duration};

use bollard_buildkit_proto::{
    google::rpc::Status as RpcStatus,
    moby::buildkit::v1::{
        frontend::{
            llb_bridge_client::LlbBridgeClient, result, PingRequest, Ref, Result as GatewayResult,
            ReturnRequest, SolveRequest as GatewaySolveRequest,
        },
        SolveRequest, SolveResponse,
    },
    pb,
};
use futures_util::try_join;
use tonic::{Code, Request, Status};

use crate::grpc::{error::GrpcError, BuildRef};

use super::{
    moby::Moby, run_until, Driver, GrpcServer, TearDownGuard, DEFAULT_MAX_RECV_MSG_SIZE,
    DEFAULT_MAX_SEND_MSG_SIZE,
};

const CAP_SOLVE_BASE: &str = "solve.base";
const CAP_RETURN: &str = "return";
const CAP_PROTO_REF_ARRAY: &str = "proto.refarray";

/// Inputs for the private, single-session Gateway build coordinator.
pub(crate) struct GatewaySolve {
    pub(crate) definition: pb::Definition,
    pub(crate) outer_request: SolveRequest,
    /// Session providers prepared by the caller for this one build.
    pub(crate) services: Vec<GrpcServer>,
    pub(crate) result_metadata: HashMap<String, Vec<u8>>,
    /// Cache imports sent with the definition in the inner Gateway `SolveRequest`.
    pub(crate) cache_imports:
        Vec<bollard_buildkit_proto::moby::buildkit::v1::frontend::CacheOptionsEntry>,
    pub(crate) build_ref: BuildRef,
    pub(crate) timeout: Option<Duration>,
}

/// Run a single Moby Gateway build through one shared session and gRPC channel.
///
/// The outer Control solve and the Gateway callback are polled concurrently;
/// dropping either side cancels the other before the driver is torn down.
pub(crate) async fn solve(driver: &Moby, build: GatewaySolve) -> Result<SolveResponse, GrpcError> {
    solve_with_driver(driver, build).await
}

async fn solve_with_driver<D: Driver>(
    driver: &D,
    mut build: GatewaySolve,
) -> Result<SolveResponse, GrpcError> {
    let session_id = crate::grpc::new_id();
    let deadline = build
        .timeout
        .map(|timeout| {
            std::time::Instant::now()
                .checked_add(timeout)
                .ok_or_else(|| {
                    GrpcError::from(Status::invalid_argument(
                        "gateway solve timeout is too large",
                    ))
                })
        })
        .transpose()?;

    build.outer_request.r#ref = build.build_ref.as_ref().to_string();
    build.outer_request.frontend.clear();
    build.outer_request.definition = None;
    build.outer_request.session = session_id.clone();

    let tear_down_handler = driver.begin_solve()?;
    let mut tear_down_guard = TearDownGuard::new(tear_down_handler);

    let services = std::mem::take(&mut build.services);
    let handle = match run_until(
        deadline,
        "gateway solve",
        driver.grpc_handle(&session_id, services),
    )
    .await
    {
        Ok(handle) => handle,
        Err(error) => {
            if let Err(teardown_error) = tear_down_guard.tear_down().await {
                // Preserve the setup error; cleanup failure is secondary diagnostic context.
                log::warn!(
                    "failed to tear down Moby driver after Gateway setup failure: {teardown_error}"
                );
            }
            return Err(error);
        }
    };

    log::debug!(
        "sending Gateway solve request: {:?}",
        super::SolveRequestSummary::from(&build.outer_request)
    );

    let result = run_until(deadline, "gateway solve", async move {
        let mut gateway_client = handle.gateway_client(&build.build_ref);
        let mut control_client = handle
            .control_client
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);
        gateway_client = gateway_client
            .max_decoding_message_size(DEFAULT_MAX_RECV_MSG_SIZE)
            .max_encoding_message_size(DEFAULT_MAX_SEND_MSG_SIZE);

        let outer = async move {
            control_client
                .solve(build.outer_request)
                .await
                .map(|response| response.into_inner())
                .map_err(GrpcError::from)
        };
        let callback = run_gateway_callback(
            gateway_client,
            build.definition,
            build.result_metadata,
            build.cache_imports,
        );

        let (response, ()) = try_join!(outer, callback)?;
        Ok(response)
    })
    .await;
    let teardown_result = tear_down_guard.tear_down().await;

    match result {
        Err(error) => {
            if let Err(teardown_error) = teardown_result {
                // Don't hide the primary Gateway/outer-solve error with a teardown error.
                log::warn!(
                    "failed to tear down Moby driver after Gateway solve failure: {teardown_error}"
                );
            }
            Err(error)
        }
        Ok(response) => teardown_result.map(|_| response),
    }
}

async fn run_gateway_callback(
    mut client: LlbBridgeClient<
        tonic::codegen::InterceptedService<tonic::transport::Channel, super::DriverInterceptor>,
    >,
    definition: pb::Definition,
    result_metadata: HashMap<String, Vec<u8>>,
    cache_imports: Vec<bollard_buildkit_proto::moby::buildkit::v1::frontend::CacheOptionsEntry>,
) -> Result<(), GrpcError> {
    let ping = client
        .ping(Request::new(PingRequest {}))
        .await
        .map_err(GrpcError::from)?
        .into_inner();
    let caps = ping.frontend_api_caps;
    if let Err(error) = require_capability(&caps, CAP_SOLVE_BASE)
        .and_then(|()| require_capability(&caps, CAP_RETURN))
    {
        let status = match &error {
            GrpcError::GatewayCapabilityUnavailable { capability, reason } => {
                Status::failed_precondition(format!(
                    "required Gateway capability `{capability}` is unavailable: {reason}"
                ))
            }
            _ => Status::failed_precondition("required Gateway capability is unavailable"),
        };
        if let Err(return_error) = send_gateway_error(&mut client, &status).await {
            log::debug!(
                "Gateway Return failed after capability negotiation failure: {return_error}"
            );
        }
        return Err(error);
    }
    let array_refs = supports_capability(&caps, CAP_PROTO_REF_ARRAY);

    let response = client
        .solve(Request::new(GatewaySolveRequest {
            definition: Some(definition),
            allow_result_return: true,
            allow_result_array_ref: true,
            cache_imports,
            ..Default::default()
        }))
        .await;

    let response = match response {
        Ok(response) => response.into_inner(),
        Err(error) => {
            let return_error = send_gateway_error(&mut client, &error).await;
            if let Err(return_error) = return_error {
                log::debug!("Gateway Return failed after Gateway Solve error: {return_error}");
            }
            return Err(error.into());
        }
    };

    let gateway_result = match response.result {
        Some(result) => match with_result_metadata(result, result_metadata, array_refs) {
            Ok(result) => result,
            Err(error) => {
                let return_error = send_gateway_error(&mut client, &error).await;
                if let Err(return_error) = return_error {
                    log::debug!(
                        "Gateway Return failed after invalid Gateway result: {return_error}"
                    );
                }
                return Err(error.into());
            }
        },
        None => {
            let error = Status::new(Code::FailedPrecondition, "Gateway Solve returned no result");
            let return_error = send_gateway_error(&mut client, &error).await;
            if let Err(return_error) = return_error {
                log::debug!("Gateway Return failed after missing Gateway result: {return_error}");
            }
            return Err(error.into());
        }
    };

    client
        .r#return(Request::new(ReturnRequest {
            result: Some(gateway_result),
            error: None,
        }))
        .await
        .map_err(GrpcError::from)?;
    Ok(())
}

fn with_result_metadata(
    mut result: GatewayResult,
    metadata: HashMap<String, Vec<u8>>,
    array_refs: bool,
) -> Result<GatewayResult, Status> {
    let reference = match result.result.take() {
        Some(result::Result::Ref(reference)) => reference,
        Some(result::Result::RefDeprecated(id)) => Ref { id, def: None },
        Some(result::Result::Refs(_)) | Some(result::Result::RefsDeprecated(_)) => {
            return Err(Status::failed_precondition(
                "Gateway Solve returned multiple references; only one-platform single-reference results are supported",
            ));
        }
        None => {
            return Err(Status::failed_precondition(
                "Gateway Solve result did not contain a reference",
            ));
        }
    };

    result.metadata.extend(metadata);
    result.result = Some(if array_refs {
        result::Result::Ref(reference)
    } else {
        result::Result::RefDeprecated(reference.id)
    });
    Ok(result)
}

/// Try to report the primary callback error to BuildKit without replacing it if Return also fails.
async fn send_gateway_error(
    client: &mut LlbBridgeClient<
        tonic::codegen::InterceptedService<tonic::transport::Channel, super::DriverInterceptor>,
    >,
    error: &Status,
) -> Result<(), GrpcError> {
    client
        .r#return(Request::new(ReturnRequest {
            result: None,
            error: Some(RpcStatus {
                code: error.code() as i32,
                message: error.message().to_string(),
                details: Vec::new(),
            }),
        }))
        .await
        .map_err(GrpcError::from)?;
    Ok(())
}

fn supports_capability(
    caps: &[bollard_buildkit_proto::moby::buildkit::v1::apicaps::ApiCap],
    id: &str,
) -> bool {
    caps.iter().any(|cap| cap.id == id && cap.enabled)
}

fn require_capability(
    caps: &[bollard_buildkit_proto::moby::buildkit::v1::apicaps::ApiCap],
    id: &str,
) -> Result<(), GrpcError> {
    match caps.iter().find(|cap| cap.id == id) {
        Some(cap) if cap.enabled => Ok(()),
        Some(cap) => Err(GrpcError::GatewayCapabilityUnavailable {
            capability: id.to_string(),
            reason: if cap.disabled_reason_msg.is_empty() {
                cap.disabled_reason.clone()
            } else {
                cap.disabled_reason_msg.clone()
            },
        }),
        None => Err(GrpcError::GatewayCapabilityUnavailable {
            capability: id.to_string(),
            reason: String::from("capability was not advertised by Gateway Ping"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        pin::Pin,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use bollard_buildkit_proto::moby::buildkit::v1::frontend;
    use bollard_buildkit_proto::moby::buildkit::v1::{
        control_server::{Control, ControlServer},
        frontend::{
            llb_bridge_server::{LlbBridge, LlbBridgeServer},
            EvaluateRequest, EvaluateResponse, ExecMessage, InputsRequest, InputsResponse,
            NewContainerRequest, NewContainerResponse, PingRequest, PongResponse, ReadDirRequest,
            ReadDirResponse, ReadFileRequest, ReadFileResponse, ReleaseContainerRequest,
            ReleaseContainerResponse, ResolveImageConfigRequest, ResolveImageConfigResponse,
            ResolveSourceMetaRequest, ResolveSourceMetaResponse, ReturnRequest, ReturnResponse,
            SolveRequest as GatewayProtoSolveRequest, StatFileRequest, StatFileResponse,
            WarnRequest, WarnResponse,
        },
        BuildHistoryEvent, BuildHistoryRequest, BytesMessage, DiskUsageRequest, DiskUsageResponse,
        InfoRequest, InfoResponse, ListWorkersRequest, ListWorkersResponse, PruneRequest,
        SolveRequest as ControlSolveRequest, StatusRequest, StatusResponse,
        UpdateBuildHistoryRequest, UpdateBuildHistoryResponse, UsageRecord,
    };
    use futures_util::stream::Empty;
    use tokio::{net::TcpListener, sync::oneshot};
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::{
        transport::{Endpoint, Server},
        Request, Response, Status,
    };

    use super::*;

    #[derive(Default)]
    struct Exchange {
        control_request: Option<ControlSolveRequest>,
        control_metadata: Option<tonic::metadata::MetadataMap>,
        gateway_request: Option<GatewaySolveRequest>,
        gateway_metadata: Option<tonic::metadata::MetadataMap>,
        return_request: Option<ReturnRequest>,
        return_metadata: Option<tonic::metadata::MetadataMap>,
    }

    struct TestControl {
        exchange: Arc<Mutex<Exchange>>,
        behavior: ControlBehavior,
    }

    enum ControlBehavior {
        Succeed,
        FailAfterGatewayStarts {
            error: Status,
            gateway_started: Arc<tokio::sync::Notify>,
        },
    }

    enum GatewayBehavior {
        Succeed,
        Fail(Status),
        Pending,
    }

    /// Signals when the server drops a pending Gateway handler after client
    /// cancellation.
    struct DropSignal(Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.lock().unwrap().take() {
                let _ = sender.send(());
            }
        }
    }

    type EmptyUsage = Empty<std::result::Result<UsageRecord, Status>>;
    type EmptyStatus = Empty<std::result::Result<StatusResponse, Status>>;
    type EmptyBytes = Empty<std::result::Result<BytesMessage, Status>>;
    type EmptyHistory = Empty<std::result::Result<BuildHistoryEvent, Status>>;

    #[tonic::async_trait]
    impl Control for TestControl {
        type PruneStream = EmptyUsage;
        type StatusStream = EmptyStatus;
        type SessionStream = EmptyBytes;
        type ListenBuildHistoryStream = EmptyHistory;

        async fn disk_usage(
            &self,
            _: Request<DiskUsageRequest>,
        ) -> Result<Response<DiskUsageResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn prune(
            &self,
            _: Request<PruneRequest>,
        ) -> Result<Response<Self::PruneStream>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn solve(
            &self,
            request: Request<ControlSolveRequest>,
        ) -> std::result::Result<Response<SolveResponse>, Status> {
            {
                let mut exchange = self.exchange.lock().unwrap();
                exchange.control_metadata = Some(request.metadata().clone());
                exchange.control_request = Some(request.into_inner());
            }
            match &self.behavior {
                ControlBehavior::Succeed => Ok(Response::new(SolveResponse::default())),
                ControlBehavior::FailAfterGatewayStarts {
                    error,
                    gateway_started,
                } => {
                    // Wait until the Gateway handler is pending so cancellation
                    // is observable when this outer solve fails.
                    gateway_started.notified().await;
                    Err(error.clone())
                }
            }
        }
        async fn status(
            &self,
            _: Request<StatusRequest>,
        ) -> Result<Response<Self::StatusStream>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn session(
            &self,
            _: Request<tonic::Streaming<BytesMessage>>,
        ) -> Result<Response<Self::SessionStream>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn list_workers(
            &self,
            _: Request<ListWorkersRequest>,
        ) -> Result<Response<ListWorkersResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn info(&self, _: Request<InfoRequest>) -> Result<Response<InfoResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn listen_build_history(
            &self,
            _: Request<BuildHistoryRequest>,
        ) -> Result<Response<Self::ListenBuildHistoryStream>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn update_build_history(
            &self,
            _: Request<UpdateBuildHistoryRequest>,
        ) -> Result<Response<UpdateBuildHistoryResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
    }

    struct TestGateway {
        exchange: Arc<Mutex<Exchange>>,
        capabilities: Vec<bollard_buildkit_proto::moby::buildkit::v1::apicaps::ApiCap>,
        behavior: GatewayBehavior,
        gateway_started: Option<Arc<tokio::sync::Notify>>,
        handler_dropped: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    }

    type EmptyExec = Empty<std::result::Result<ExecMessage, Status>>;

    #[tonic::async_trait]
    impl LlbBridge for TestGateway {
        type ExecProcessStream = EmptyExec;

        async fn resolve_image_config(
            &self,
            _: Request<ResolveImageConfigRequest>,
        ) -> Result<Response<ResolveImageConfigResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn resolve_source_meta(
            &self,
            _: Request<ResolveSourceMetaRequest>,
        ) -> Result<Response<ResolveSourceMetaResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn solve(
            &self,
            request: Request<GatewayProtoSolveRequest>,
        ) -> std::result::Result<Response<frontend::SolveResponse>, Status> {
            // Keep this guard alive through the handler future. If the client
            // cancels its Gateway RPC, dropping the server handler signals it.
            let _drop_signal = DropSignal(Arc::clone(&self.handler_dropped));
            {
                let mut exchange = self.exchange.lock().unwrap();
                exchange.gateway_metadata = Some(request.metadata().clone());
                exchange.gateway_request = Some(request.into_inner());
            }
            if let Some(started) = &self.gateway_started {
                started.notify_one();
            }
            match &self.behavior {
                GatewayBehavior::Succeed => Ok(Response::new(frontend::SolveResponse {
                    result: Some(frontend::Result {
                        metadata: HashMap::new(),
                        attestations: HashMap::new(),
                        result: Some(result::Result::Ref(Ref {
                            id: "result-ref".to_string(),
                            def: None,
                        })),
                    }),
                    ..Default::default()
                })),
                GatewayBehavior::Fail(error) => Err(error.clone()),
                GatewayBehavior::Pending => std::future::pending().await,
            }
        }
        async fn read_file(
            &self,
            _: Request<ReadFileRequest>,
        ) -> Result<Response<ReadFileResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn read_dir(
            &self,
            _: Request<ReadDirRequest>,
        ) -> Result<Response<ReadDirResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn stat_file(
            &self,
            _: Request<StatFileRequest>,
        ) -> Result<Response<StatFileResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn evaluate(
            &self,
            _: Request<EvaluateRequest>,
        ) -> Result<Response<EvaluateResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn ping(
            &self,
            request: Request<PingRequest>,
        ) -> Result<Response<PongResponse>, Status> {
            let mut exchange = self.exchange.lock().unwrap();
            exchange.gateway_metadata = Some(request.metadata().clone());
            Ok(Response::new(PongResponse {
                frontend_api_caps: self.capabilities.clone(),
                ..Default::default()
            }))
        }
        async fn r#return(
            &self,
            request: Request<ReturnRequest>,
        ) -> Result<Response<ReturnResponse>, Status> {
            let mut exchange = self.exchange.lock().unwrap();
            exchange.return_metadata = Some(request.metadata().clone());
            exchange.return_request = Some(request.into_inner());
            Ok(Response::new(ReturnResponse {}))
        }
        async fn inputs(
            &self,
            _: Request<InputsRequest>,
        ) -> Result<Response<InputsResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn new_container(
            &self,
            _: Request<NewContainerRequest>,
        ) -> Result<Response<NewContainerResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn release_container(
            &self,
            _: Request<ReleaseContainerRequest>,
        ) -> Result<Response<ReleaseContainerResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn exec_process(
            &self,
            _: Request<tonic::Streaming<ExecMessage>>,
        ) -> Result<Response<Self::ExecProcessStream>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn read_file_container(
            &self,
            _: Request<ReadFileRequest>,
        ) -> Result<Response<ReadFileResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn read_dir_container(
            &self,
            _: Request<ReadDirRequest>,
        ) -> Result<Response<ReadDirResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn stat_file_container(
            &self,
            _: Request<StatFileRequest>,
        ) -> Result<Response<StatFileResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
        async fn warn(&self, _: Request<WarnRequest>) -> Result<Response<WarnResponse>, Status> {
            Err(Status::unimplemented("test"))
        }
    }

    struct TestDriver {
        endpoint: Endpoint,
        teardown_calls: Arc<AtomicUsize>,
    }

    impl Driver for TestDriver {
        async fn grpc_handle(
            &self,
            session_id: &str,
            _: Vec<GrpcServer>,
        ) -> Result<super::super::GrpcHandle, GrpcError> {
            let channel = self.endpoint.connect().await?;
            Ok(super::super::GrpcHandle::new(
                channel,
                super::super::DriverInterceptor {
                    session_id: session_id.to_string(),
                    metadata_grpc_method: Vec::new(),
                    build_ref: None,
                },
            ))
        }
        fn begin_solve(&self) -> Result<Box<dyn super::super::DriverTearDownHandler>, GrpcError> {
            Ok(Box::new(TestTeardown(Arc::clone(&self.teardown_calls))))
        }
    }

    struct TestTeardown(Arc<AtomicUsize>);

    impl super::super::DriverTearDownHandler for TestTeardown {
        fn tear_down(
            &self,
        ) -> Pin<Box<dyn futures_core::Future<Output = Result<(), GrpcError>> + Send + 'static>>
        {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::ready(Ok(())))
        }
    }

    async fn start_server(
        control: TestControl,
        gateway: TestGateway,
    ) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = Server::builder()
            .add_service(ControlServer::new(control))
            .add_service(LlbBridgeServer::new(gateway))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                let _ = shutdown_rx.await;
            });
        let task = tokio::spawn(async move {
            server.await.unwrap();
        });
        (address, shutdown_tx, task)
    }

    async fn stop_server(shutdown: oneshot::Sender<()>, task: tokio::task::JoinHandle<()>) {
        let _ = shutdown.send(());
        task.await.unwrap();
    }

    fn all_caps() -> Vec<bollard_buildkit_proto::moby::buildkit::v1::apicaps::ApiCap> {
        [CAP_SOLVE_BASE, CAP_RETURN, CAP_PROTO_REF_ARRAY]
            .into_iter()
            .map(
                |id| bollard_buildkit_proto::moby::buildkit::v1::apicaps::ApiCap {
                    id: id.to_string(),
                    enabled: true,
                    ..Default::default()
                },
            )
            .collect()
    }

    fn test_control(exchange: Arc<Mutex<Exchange>>, behavior: ControlBehavior) -> TestControl {
        TestControl { exchange, behavior }
    }

    fn test_gateway(exchange: Arc<Mutex<Exchange>>, behavior: GatewayBehavior) -> TestGateway {
        TestGateway {
            exchange,
            capabilities: all_caps(),
            behavior,
            gateway_started: None,
            handler_dropped: Arc::new(Mutex::new(None)),
        }
    }

    fn build_request(build_ref: BuildRef, timeout: Option<Duration>) -> GatewaySolve {
        GatewaySolve {
            definition: pb::Definition {
                def: vec![vec![1, 2, 3]],
                ..Default::default()
            },
            services: vec![],
            outer_request: ControlSolveRequest {
                frontend: "must-be-cleared".to_string(),
                frontend_attrs: HashMap::from([(
                    "platform".to_string(),
                    "linux/amd64".to_string(),
                )]),
                ..Default::default()
            },
            result_metadata: HashMap::from([(
                "containerimage.config".to_string(),
                b"{\"config\":{}}".to_vec(),
            )]),
            cache_imports: vec![frontend::CacheOptionsEntry {
                r#type: "registry".to_string(),
                attrs: HashMap::from([("ref".to_string(), "example.com/cache:latest".to_string())]),
            }],
            build_ref,
            timeout,
        }
    }

    fn required_metadata<'a>(
        metadata: &'a tonic::metadata::MetadataMap,
        key: &'static str,
    ) -> &'a str {
        metadata.get(key).unwrap().to_str().unwrap()
    }

    #[tokio::test]
    async fn coordinates_shared_session_build_ref_solve_and_return() {
        let exchange = Arc::new(Mutex::new(Exchange::default()));
        let (address, shutdown, task) = start_server(
            test_control(exchange.clone(), ControlBehavior::Succeed),
            test_gateway(exchange.clone(), GatewayBehavior::Succeed),
        )
        .await;
        let teardown_calls = Arc::new(AtomicUsize::new(0));
        let driver = TestDriver {
            endpoint: Endpoint::from_shared(format!("http://{address}")).unwrap(),
            teardown_calls: teardown_calls.clone(),
        };
        let build_ref = BuildRef::random();
        solve_with_driver(
            &driver,
            build_request(build_ref.clone(), Some(Duration::from_secs(2))),
        )
        .await
        .unwrap();

        {
            let exchange = exchange.lock().unwrap();
            let outer = exchange.control_request.as_ref().unwrap();
            let session_id = outer.session.as_str();
            assert_eq!(outer.r#ref, build_ref.as_ref());
            assert!(outer.frontend.is_empty());
            assert!(outer.definition.is_none());
            assert_eq!(
                outer.frontend_attrs.get("platform").map(String::as_str),
                Some("linux/amd64")
            );
            assert_eq!(
                required_metadata(
                    exchange.control_metadata.as_ref().unwrap(),
                    "x-docker-expose-session-uuid"
                ),
                session_id
            );

            let gateway_request = exchange.gateway_request.as_ref().unwrap();
            assert_eq!(
                gateway_request.definition.as_ref().unwrap().def,
                vec![vec![1, 2, 3]]
            );
            assert!(gateway_request.allow_result_return);
            assert!(gateway_request.allow_result_array_ref);
            // Cache imports travel on the inner Gateway solve, because the outer
            // Control request carries no definition.
            assert_eq!(gateway_request.cache_imports.len(), 1);
            assert_eq!(gateway_request.cache_imports[0].r#type, "registry");
            assert_eq!(
                gateway_request.cache_imports[0]
                    .attrs
                    .get("ref")
                    .map(String::as_str),
                Some("example.com/cache:latest")
            );
            let gateway_metadata = exchange.gateway_metadata.as_ref().unwrap();
            assert_eq!(
                required_metadata(gateway_metadata, "x-docker-expose-session-uuid"),
                session_id
            );
            assert_eq!(
                required_metadata(gateway_metadata, "buildkit-controlapi-buildid"),
                build_ref.as_ref()
            );

            let returned = exchange
                .return_request
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap();
            assert_eq!(
                returned.metadata.get("containerimage.config").unwrap(),
                b"{\"config\":{}}"
            );
            assert_eq!(
                returned.result,
                Some(result::Result::Ref(Ref {
                    id: "result-ref".to_string(),
                    def: None
                }))
            );
            assert_eq!(
                required_metadata(
                    exchange.return_metadata.as_ref().unwrap(),
                    "buildkit-controlapi-buildid"
                ),
                build_ref.as_ref()
            );
        }
        assert_eq!(teardown_calls.load(Ordering::SeqCst), 1);
        stop_server(shutdown, task).await;
    }

    #[tokio::test]
    async fn rejects_missing_capability_before_gateway_solve() {
        let exchange = Arc::new(Mutex::new(Exchange::default()));
        let caps = vec![
            bollard_buildkit_proto::moby::buildkit::v1::apicaps::ApiCap {
                id: CAP_SOLVE_BASE.to_string(),
                enabled: true,
                ..Default::default()
            },
        ];
        let (address, shutdown, task) = start_server(
            test_control(exchange.clone(), ControlBehavior::Succeed),
            TestGateway {
                capabilities: caps,
                ..test_gateway(exchange.clone(), GatewayBehavior::Succeed)
            },
        )
        .await;
        let teardown_calls = Arc::new(AtomicUsize::new(0));
        let driver = TestDriver {
            endpoint: Endpoint::from_shared(format!("http://{address}")).unwrap(),
            teardown_calls: teardown_calls.clone(),
        };
        let error = solve_with_driver(
            &driver,
            build_request(BuildRef::random(), Some(Duration::from_secs(2))),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, GrpcError::GatewayCapabilityUnavailable { capability, .. } if capability == CAP_RETURN)
        );
        {
            let exchange = exchange.lock().unwrap();
            assert!(exchange.gateway_request.is_none());
            let returned = exchange.return_request.as_ref().unwrap();
            assert!(returned.result.is_none());
            assert_eq!(
                returned.error.as_ref().unwrap().code,
                Code::FailedPrecondition as i32
            );
        }
        assert_eq!(teardown_calls.load(Ordering::SeqCst), 1);
        stop_server(shutdown, task).await;
    }

    #[tokio::test]
    async fn returns_gateway_solve_error_to_outer_build() {
        let exchange = Arc::new(Mutex::new(Exchange::default()));
        let error = Status::invalid_argument("invalid LLB");
        let (address, shutdown, task) = start_server(
            test_control(exchange.clone(), ControlBehavior::Succeed),
            test_gateway(exchange.clone(), GatewayBehavior::Fail(error)),
        )
        .await;
        let teardown_calls = Arc::new(AtomicUsize::new(0));
        let driver = TestDriver {
            endpoint: Endpoint::from_shared(format!("http://{address}")).unwrap(),
            teardown_calls: teardown_calls.clone(),
        };
        let result = solve_with_driver(
            &driver,
            build_request(BuildRef::random(), Some(Duration::from_secs(2))),
        )
        .await;
        assert!(
            matches!(result, Err(GrpcError::TonicStatus { err }) if err.code() == Code::InvalidArgument)
        );
        {
            let exchange = exchange.lock().unwrap();
            assert_eq!(
                exchange
                    .return_request
                    .as_ref()
                    .unwrap()
                    .error
                    .as_ref()
                    .unwrap()
                    .code,
                Code::InvalidArgument as i32
            );
            assert_eq!(
                exchange
                    .return_request
                    .as_ref()
                    .unwrap()
                    .error
                    .as_ref()
                    .unwrap()
                    .message,
                "invalid LLB"
            );
            assert!(exchange.return_request.as_ref().unwrap().result.is_none());
        }
        assert_eq!(teardown_calls.load(Ordering::SeqCst), 1);
        stop_server(shutdown, task).await;
    }

    #[tokio::test]
    async fn timeout_cancels_gateway_callback_and_tears_down() {
        let exchange = Arc::new(Mutex::new(Exchange::default()));
        let (address, shutdown, task) = start_server(
            test_control(exchange.clone(), ControlBehavior::Succeed),
            test_gateway(exchange, GatewayBehavior::Pending),
        )
        .await;
        let teardown_calls = Arc::new(AtomicUsize::new(0));
        let driver = TestDriver {
            endpoint: Endpoint::from_shared(format!("http://{address}")).unwrap(),
            teardown_calls: teardown_calls.clone(),
        };
        let result = solve_with_driver(
            &driver,
            build_request(BuildRef::random(), Some(Duration::from_millis(30))),
        )
        .await;
        assert!(
            matches!(result, Err(GrpcError::TonicStatus { err }) if err.code() == Code::DeadlineExceeded)
        );
        assert_eq!(teardown_calls.load(Ordering::SeqCst), 1);
        stop_server(shutdown, task).await;
    }

    #[tokio::test]
    async fn outer_control_failure_cancels_gateway_callback() {
        let exchange = Arc::new(Mutex::new(Exchange::default()));
        let control_error = Status::permission_denied("outer solve rejected");
        let (dropped_tx, dropped_rx) = oneshot::channel();
        let gateway_started = Arc::new(tokio::sync::Notify::new());
        let (address, shutdown, task) = start_server(
            test_control(
                exchange.clone(),
                ControlBehavior::FailAfterGatewayStarts {
                    error: control_error,
                    gateway_started: Arc::clone(&gateway_started),
                },
            ),
            TestGateway {
                gateway_started: Some(gateway_started),
                handler_dropped: Arc::new(Mutex::new(Some(dropped_tx))),
                ..test_gateway(exchange.clone(), GatewayBehavior::Pending)
            },
        )
        .await;
        let teardown_calls = Arc::new(AtomicUsize::new(0));
        let driver = TestDriver {
            endpoint: Endpoint::from_shared(format!("http://{address}")).unwrap(),
            teardown_calls: teardown_calls.clone(),
        };
        let result = solve_with_driver(
            &driver,
            build_request(BuildRef::random(), Some(Duration::from_secs(2))),
        )
        .await;
        assert!(
            matches!(result, Err(GrpcError::TonicStatus { err }) if err.code() == Code::PermissionDenied)
        );
        assert_eq!(teardown_calls.load(Ordering::SeqCst), 1);
        // The Gateway callback ran and must be dropped once the outer solve fails.
        assert!(exchange.lock().unwrap().gateway_request.is_some());
        tokio::time::timeout(Duration::from_secs(15), dropped_rx)
            .await
            .expect("the pending Gateway callback should be cancelled promptly")
            .expect("the drop signal should be delivered");
        stop_server(shutdown, task).await;
    }

    #[test]
    fn with_result_metadata_converts_single_refs_and_merges_metadata() {
        let metadata = HashMap::from([(String::from("containerimage.config"), b"{}{}".to_vec())]);

        // Deprecated non-array input with array refs enabled returns the array form.
        let result = with_result_metadata(
            GatewayResult {
                metadata: HashMap::new(),
                result: Some(result::Result::RefDeprecated(String::from("legacy-ref"))),
                ..Default::default()
            },
            metadata.clone(),
            true,
        )
        .expect("deprecated reference input is accepted");
        assert!(matches!(
            result.result,
            Some(result::Result::Ref(Ref { ref id, .. })) if id == "legacy-ref"
        ));

        // Structured single-ref input with array refs disabled returns the
        // deprecated single-ref form and caller metadata takes precedence.
        let result = with_result_metadata(
            GatewayResult {
                metadata: HashMap::from([(
                    String::from("containerimage.config"),
                    b"stale".to_vec(),
                )]),
                result: Some(result::Result::Ref(Ref {
                    id: String::from("array-ref"),
                    def: None,
                })),
                ..Default::default()
            },
            metadata.clone(),
            false,
        )
        .expect("array reference input is accepted");
        assert!(matches!(
            result.result,
            Some(result::Result::RefDeprecated(id)) if id == "array-ref"
        ));
        assert_eq!(
            result
                .metadata
                .get("containerimage.config")
                .map(Vec::as_slice),
            Some(b"{}{}".as_slice())
        );

        // Both current and deprecated multi-reference forms are rejected.
        for refs in [
            result::Result::Refs(frontend::RefMap::default()),
            result::Result::RefsDeprecated(frontend::RefMapDeprecated::default()),
        ] {
            let error = with_result_metadata(
                GatewayResult {
                    metadata: HashMap::new(),
                    result: Some(refs),
                    ..Default::default()
                },
                metadata.clone(),
                true,
            )
            .expect_err("multiple references must be rejected");
            assert_eq!(error.code(), Code::FailedPrecondition);
        }

        // A missing reference is rejected.
        let error = with_result_metadata(
            GatewayResult {
                metadata: HashMap::new(),
                result: None,
                ..Default::default()
            },
            metadata.clone(),
            true,
        )
        .expect_err("a missing reference must be rejected");
        assert_eq!(error.code(), Code::FailedPrecondition);
    }
}

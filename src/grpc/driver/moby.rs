#![cfg(feature = "buildkit")]

use bollard_buildkit_proto::{health, moby::buildkit::v1::control_client::ControlClient};
use http::{request::Builder, Method};
use hyper::Body;
use tonic::transport::{Channel, Endpoint};

use crate::{
    errors::Error,
    grpc::{io::GrpcTransport, GrpcClient, GrpcServer, HealthServerImpl},
    Docker,
};

/// The Moby driver handles a GRPC connection with an upgraded `/session` and `/grpc` endpoints in
/// Docker itself.
#[derive(Debug)]
pub struct Moby {
    pub(crate) docker: Docker,
}

impl Moby {
    async fn grpc_handle(
        &mut self,
        session_id: &str,
        services: Vec<GrpcServer>,
    ) -> Result<ControlClient<Channel>, Error> {
        let grpc_client = GrpcClient {
            client: self.docker.clone(),
            session_id: String::from(session_id),
        };

        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(grpc_client)
            .await?;

        let control_client = ControlClient::new(channel);

        let url = "/session";

        let opt: Option<serde_json::Value> = None;
        let metadata_grpc_method: Vec<String> = services.iter().flat_map(|s| s.names()).collect();

        let req = self.docker.build_request(
            &url,
            Builder::new()
                .method(Method::POST)
                .header("Connection", "Upgrade")
                .header("Upgrade", "h2c")
                .header("X-Docker-Expose-Session-Uuid", session_id)
                .header(
                    "X-Docker-Expose-Session-Grpc-Method",
                    metadata_grpc_method.join(","),
                ),
            opt,
            Ok(Body::empty()),
        );

        let (read, write) = self.docker.process_upgraded(req).await?;

        let output = Box::pin(read);
        let input = Box::pin(write);
        let transport = GrpcTransport {
            read: output,
            write: input,
        };

        tokio::spawn(async {
            let health = health::health_server::HealthServer::new(HealthServerImpl::new());
            let mut builder = tonic::transport::Server::builder();
            let mut router = builder.add_service(health);
            for service in services {
                router = service.append(router);
            }
            trace!("router: {:#?}", router);
            match router
                .serve_with_incoming(futures_util::stream::iter(vec![Ok::<
                    _,
                    tonic::transport::Error,
                >(
                    transport
                )]))
                .await
            {
                Err(e) => error!("Failed to serve grpc connection: {}", e),
                _ => (),
            }
        });

        Ok(control_client)
    }
}

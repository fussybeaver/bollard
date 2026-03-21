//! Integration tests for Podman socket connectivity.
//!
//! These tests require a running Podman daemon (rootless or system).
//! Skip with: `cargo test --test podman_test -- --ignored` to list them,
//! or run them explicitly when Podman is available.

#[cfg(unix)]
mod podman_integration {
    use bollard::Docker;
    use tokio::runtime::Runtime;

    /// Connect via `connect_with_podman_defaults()` and ping the daemon.
    ///
    /// Requires a rootless or system Podman socket to be available.
    #[test]
    fn ping_via_podman_defaults() {
        let docker = match Docker::connect_with_podman_defaults() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skipping: no Podman socket available ({e})");
                return;
            }
        };

        let rt = Runtime::new().unwrap();
        let pong = rt.block_on(docker.ping());
        assert!(pong.is_ok(), "ping failed: {:?}", pong.err());
    }

    /// Verify that version negotiation works against Podman.
    #[test]
    fn version_negotiation_with_podman() {
        let docker = match Docker::connect_with_podman_defaults() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skipping: no Podman socket available ({e})");
                return;
            }
        };

        let rt = Runtime::new().unwrap();
        let result = rt.block_on(docker.negotiate_version());
        assert!(
            result.is_ok(),
            "version negotiation failed: {:?}",
            result.err()
        );
    }

    /// Verify that listing containers works against Podman.
    #[test]
    fn list_containers_with_podman() {
        use bollard::query_parameters::ListContainersOptions;

        let docker = match Docker::connect_with_podman_defaults() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skipping: no Podman socket available ({e})");
                return;
            }
        };

        let rt = Runtime::new().unwrap();
        let result = rt.block_on(docker.list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        })));
        assert!(result.is_ok(), "list_containers failed: {:?}", result.err());
    }
}

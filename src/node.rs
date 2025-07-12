//! Node API: Nodes are instances of the Engine participating in a swarm. Swarm mode must be enabled for these endpoints to work.
#![allow(deprecated)]

use bollard_stubs::models::{Node, NodeSpec};
use bytes::Bytes;
use http_body_util::Full;
use serde::Serialize;
use std::{collections::HashMap, hash::Hash};

use crate::{docker::BodyType, errors::Error, Docker};
use http::{request::Builder, Method};

/// Parameters used in the [List Nodes API](super::Docker::list_nodes())
///
/// ## Examples
///
/// ```rust
/// use bollard::node::ListNodesOptions;
///
/// use std::collections::HashMap;
///
/// let mut filters = HashMap::new();
/// filters.insert("label", vec!["maintainer=some_maintainer"]);
///
/// ListNodesOptions {
///     filters
/// };
/// ```
///
/// ```rust
/// # use bollard::node::ListNodesOptions;
/// # use std::default::Default;
///
/// ListNodesOptions::<&str> {
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::ListNodesOptions and associated ListNodesOptionsBuilder"
)]
pub struct ListNodesOptions<T>
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    /// JSON encoded value of the filters (a `map[string][]string`) to process on the nodes list.
    ///
    /// Available filters:
    ///  - `id=<node-id>`: Matches all or part of a node ID.
    ///  - `label=<engine-label>`: Matches a node by an engine label.
    ///  - `membership=["accepted"|"pending"]`: Filters nodes by membership.
    ///  - `name=<node-name>`: Matches all or part of a node name.
    ///  - `node.label=<node-label>`: Filters nodes by node label.
    ///  - `role=["manager"|"worker"]`: Filters nodes by roll.
    #[serde(serialize_with = "crate::docker::serialize_as_json")]
    pub filters: HashMap<T, Vec<T>>,
}

impl<T> From<ListNodesOptions<T>> for crate::query_parameters::ListNodesOptions
where
    T: Into<String> + Eq + Hash + serde::ser::Serialize,
{
    fn from(opts: ListNodesOptions<T>) -> Self {
        crate::query_parameters::ListNodesOptionsBuilder::default()
            .filters(
                &opts
                    .filters
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into_iter().map(T::into).collect()))
                    .collect(),
            )
            .build()
    }
}

/// Parameters used in the [Delete Node API](Docker::delete_node())
///
/// ## Examples
///
/// ```rust
/// use bollard::node::DeleteNodeOptions;
///
/// use std::default::Default;
///
/// DeleteNodeOptions{
///     force: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::DeleteNodeOptions and associated DeleteNodeOptionsBuilder"
)]
pub struct DeleteNodeOptions {
    /// Force remove a node from the swarm.
    pub force: bool,
}

impl From<DeleteNodeOptions> for crate::query_parameters::DeleteNodeOptions {
    fn from(opts: DeleteNodeOptions) -> Self {
        crate::query_parameters::DeleteNodeOptionsBuilder::default()
            .force(opts.force)
            .build()
    }
}

/// Configuration for the [Update Node API](Docker::update_node())
///
/// ## Examples
///
/// ```rust
/// use bollard::node::UpdateNodeOptions;
/// use std::default::Default;
///
/// UpdateNodeOptions {
///     version: 2,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize)]
#[deprecated(
    since = "0.19.0",
    note = "use the OpenAPI generated bollard::query_parameters::UpdateNodeOptions and associated UpdateNodeOptionsBuilder"
)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNodeOptions {
    /// The version number of the node object being updated. This is required to avoid conflicting writes.
    pub version: u64,
}

impl From<UpdateNodeOptions> for crate::query_parameters::UpdateNodeOptions {
    fn from(opts: UpdateNodeOptions) -> Self {
        crate::query_parameters::UpdateNodeOptionsBuilder::default()
            .version(opts.version as i64)
            .build()
    }
}

impl Docker {
    /// ---
    ///
    /// # List Nodes
    ///
    /// # Arguments
    ///
    ///  - Optional [List Nodes Options](ListNodesOptions) struct.
    ///
    /// # Returns
    ///
    ///  - A vector of [Node](Node) struct, wrapped in a
    ///    Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::node::ListNodesOptions;
    ///
    /// use std::collections::HashMap;
    ///
    /// let mut list_nodes_filters = HashMap::new();
    /// list_nodes_filters.insert("node.label", vec!["my-node-label"]);
    ///
    /// let config = ListNodesOptions {
    ///     filters: list_nodes_filters,
    /// };
    ///
    /// docker.list_nodes(Some(config));
    /// ```
    pub async fn list_nodes(
        &self,
        options: Option<impl Into<crate::query_parameters::ListNodesOptions>>,
    ) -> Result<Vec<Node>, Error> {
        let url = "/nodes";

        let req = self.build_request(
            url,
            Builder::new().method(Method::GET),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Inspect a Node
    ///
    /// # Arguments
    ///
    ///  - Node id or name as a string slice.
    ///
    /// # Returns
    ///
    ///  - A [Models](Node) struct, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// docker.inspect_node("my_node_name");
    /// ```
    pub async fn inspect_node(&self, node_name: &str) -> Result<Node, Error> {
        let url = format!("/nodes/{node_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::GET),
            None::<String>,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_value(req).await
    }

    /// ---
    ///
    /// # Delete Node
    ///
    /// Delete a node.
    ///
    /// # Arguments
    ///
    /// - Node id or name as a string slice.
    /// - Optional [Delete Node Options](DeleteNodeOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::node::DeleteNodeOptions;
    ///
    /// let options = Some(DeleteNodeOptions {
    ///     force: true,
    ///     ..Default::default()
    /// });
    ///
    /// docker.delete_node("my-node", options);
    /// ```
    pub async fn delete_node(
        &self,
        node_name: &str,
        options: Option<impl Into<crate::query_parameters::DeleteNodeOptions>>,
    ) -> Result<(), Error> {
        let url = format!("/nodes/{node_name}");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::DELETE),
            options.map(Into::into),
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );

        self.process_into_unit(req).await
    }

    /// ---
    ///
    /// # Update Node
    ///
    /// Update a node.
    ///
    /// # Arguments
    ///
    ///  - Node id as string slice.
    ///  - [Update Node Options](UpdateNodeOptions) struct.
    ///
    /// # Returns
    ///
    ///  - unit type `()`, wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bollard::Docker;
    /// # let docker = Docker::connect_with_http_defaults().unwrap();
    ///
    /// use bollard::node::UpdateNodeOptions;
    /// use bollard::models::{NodeSpec, NodeSpecAvailabilityEnum, NodeSpecRoleEnum};
    ///
    /// let spec = NodeSpec {
    ///     availability: Some(NodeSpecAvailabilityEnum::ACTIVE),
    ///     name: Some("my-new-node-name".to_string()),
    ///     role: Some(NodeSpecRoleEnum::MANAGER),
    ///     ..Default::default()
    /// };
    ///
    /// let options = UpdateNodeOptions {
    ///     version: 2,
    ///     ..Default::default()
    /// };
    ///
    /// docker.update_node("my-node-id", spec, options);
    /// ```
    pub async fn update_node(
        &self,
        node_id: &str,
        spec: NodeSpec,
        options: impl Into<crate::query_parameters::UpdateNodeOptions>,
    ) -> Result<(), Error> {
        let url = format!("/nodes/{node_id}/update");

        let req = self.build_request(
            &url,
            Builder::new().method(Method::POST),
            Some(options.into()),
            Docker::serialize_payload(Some(spec)),
        );

        self.process_into_unit(req).await
    }
}

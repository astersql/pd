// Copyright 2026 AsterSQL.
//! Conditional RM registration. All local metadata mutations use the canonical manager.
use crate::PdServices;
use astersql_pd_mcs_server::{DelegateChannel, DelegateConnections};
use astersql_pd_proto::resource_manager::{
    self as p, resource_manager_client::ResourceManagerClient,
    resource_manager_server::ResourceManager,
};
use futures_util::{Future, stream};
use prost::Message;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};
#[derive(Clone)]
pub(crate) struct RegisteredResourceManager {
    local: astersql_pd_rm_server::Service,
    proxy: Option<Proxy>,
}
impl RegisteredResourceManager {
    pub(crate) fn new(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Self {
        let local = astersql_pd_rm_server::Service::new(
            Arc::new(crate::grpc_resource_manager::ResourceManagerHost(
                services.clone(),
            )),
            cancel.clone(),
        );
        let proxy = if services.resource_manager_fallback_enabled() {
            None
        } else {
            Some(Proxy::new(services, cancel))
        };
        Self { local, proxy }
    }
}
#[derive(Clone)]
struct Proxy {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
    pool: Result<Arc<DelegateConnections>, String>,
    primary: Option<Arc<astersql_pd_mcs_server::PrimaryObserver>>,
}
fn client_tls(
    value: Option<astersql_pd_grpcutil::TLSConfig>,
) -> Result<Option<Arc<rustls::ClientConfig>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let bytes = value.sslca_bytes.as_ref().is_some_and(|v| !v.is_empty())
        || value.ssl_cert_bytes.as_ref().is_some_and(|v| !v.is_empty())
        || value.sslkey_bytes.as_ref().is_some_and(|v| !v.is_empty());
    if !bytes && value.cert_path.is_empty() && value.key_path.is_empty() {
        return Ok(None);
    }
    crate::client_auth::configuration(&value).map(|configured| Some(Arc::new(configured)))
}
fn same(first: &str, second: &str) -> bool {
    crate::grpc_forward::base(first).is_some_and(|a| Some(a) == crate::grpc_forward::base(second))
}
// grpc-go classifies connection IO failures as Unavailable. Tonic's h2
// conversion leaves IO errors without an HTTP/2 reason as Unknown. Wire statuses
// have no local error source, so business Unknown replies remain unchanged.
fn delegate_error(error: Status) -> Status {
    if error.code() != tonic::Code::Unknown {
        return error;
    }
    let mut source = std::error::Error::source(&error);
    while let Some(current) = source {
        let io = current.is::<std::io::Error>()
            || current
                .downcast_ref::<h2::Error>()
                .is_some_and(h2::Error::is_io);
        if io {
            let mut converted = Status::with_details_and_metadata(
                tonic::Code::Unavailable,
                error.message(),
                error.details().to_vec().into(),
                error.metadata().clone(),
            );
            converted.set_source(Arc::new(error));
            return converted;
        }
        source = current.source();
    }
    error
}
fn rebuild(status: &Status) -> bool {
    matches!(
        status.code(),
        tonic::Code::Unavailable
            | tonic::Code::DeadlineExceeded
            | tonic::Code::Internal
            | tonic::Code::Unknown
            | tonic::Code::ResourceExhausted
    )
}
impl Proxy {
    fn new(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Self {
        let security = services.delegate_security();
        let pool = client_tls(security.clone()).map(|configured| {
            let runtime = tokio::runtime::Handle::current();
            match configured {
                None => Arc::new(DelegateConnections::new(runtime, &cancel, None)),
                Some(_) => Arc::new(DelegateConnections::new_with_rustls_factory(
                    runtime,
                    &cancel,
                    Arc::new(move || {
                        client_tls(security.clone())?
                            .ok_or_else(|| "delegate TLS policy missing".into())
                    }),
                )),
            }
        });
        let primary = if services.mcs_enabled() {
            services.discovery_store().map(|store| {
                let key = astersql_pd_keypath::ClusterPaths::new(services.identity().cluster_id)
                    .election_path(Some(&astersql_pd_keypath::MsParam {
                        service_name: "resource_manager".into(),
                        group_id: 0,
                    }));
                Arc::new(astersql_pd_mcs_server::PrimaryObserver::start(
                    &tokio::runtime::Handle::current(),
                    &cancel,
                    store,
                    key.into_bytes(),
                    |bytes| {
                        p::Participant::decode(bytes)
                            .ok()?
                            .listen_urls
                            .into_iter()
                            .next()
                    },
                ))
            })
        } else {
            None
        };
        Self {
            services,
            cancel,
            pool,
            primary,
        }
    }
    fn pool(&self) -> Result<&Arc<DelegateConnections>, Status> {
        self.pool
            .as_ref()
            .map_err(|error| Status::unavailable(error.clone()))
    }
    fn client<'a>(
        &'a self,
        target: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn Future<Output = Result<ResourceManagerClient<DelegateChannel>, Status>> + Send + 'a,
        >,
    > {
        Box::pin(async move { Ok(ResourceManagerClient::new(self.pool()?.get(target).await?)) })
    }
    async fn primary(&self) -> Result<String, Status> {
        let unavailable = || Status::unavailable("resource manager service is not available");
        for _ in 0..25 {
            if let Some(target) = self
                .primary
                .as_ref()
                .and_then(|observer| observer.current())
            {
                if same(&target, &self.services.identity().client_url) {
                    return Err(Status::unavailable(
                        "Microservice mode should not support resource manager requests",
                    ));
                }
                return Ok(target);
            }
            tokio::select! {_=self.cancel.clone().cancelled_owned()=>return Err(unavailable()),_=tokio::time::sleep(Duration::from_millis(100))=>{}}
        }
        Err(unavailable())
    }
    async fn metadata_target<T>(&self, request: &Request<T>) -> Result<Option<String>, Status> {
        let forwarded = request
            .metadata()
            .get("pd-forwarded-host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let leader = crate::http_redirect::leader(self.services.as_ref()).await;
        let leader = leader
            .filter(|leader| !leader.client_urls.is_empty())
            .ok_or_else(|| {
                if forwarded.is_empty() {
                    Status::unavailable("pd leader is not available")
                } else {
                    Status::unknown("pd: not leader")
                }
            })?;
        let target = if forwarded.is_empty() {
            leader.client_urls[0].clone()
        } else {
            if !leader.client_urls.iter().any(|url| same(url, forwarded)) {
                return Err(Status::invalid_argument(format!(
                    "forwarded host {forwarded:?} is not a client URL of the PD leader: pd is not leader of cluster"
                )));
            }
            forwarded.into()
        };
        Ok(if same(&target, &self.services.identity().client_url) {
            None
        } else {
            Some(target)
        })
    }
    fn finish<T>(&self, target: &str, result: Result<T, Status>) -> Result<T, Status> {
        let result = result.map_err(delegate_error);
        if result.as_ref().is_err_and(rebuild) {
            if let Ok(pool) = self.pool() {
                pool.invalidate(target);
            }
        }
        result
    }
}
#[derive(Clone)]
struct Budget {
    host: CancellationToken,
    request: CancellationToken,
    deadline: Option<tokio::time::Instant>,
}
impl Budget {
    fn new<T>(request: &Request<T>, host: &CancellationToken) -> Self {
        Self {
            host: host.clone(),
            request: crate::request_context::current()
                .map(|c| c.cancel)
                .unwrap_or_else(|| host.child_token()),
            deadline: request
                .metadata()
                .get("grpc-timeout")
                .and_then(|v| v.to_str().ok())
                .and_then(crate::grpc_forward::timeout)
                .and_then(|duration| tokio::time::Instant::now().checked_add(duration)),
        }
    }
    fn prepare<T>(&self, request: &mut Request<T>) {
        if let Some(deadline) = self.deadline {
            request.set_timeout(deadline.saturating_duration_since(tokio::time::Instant::now()));
        }
    }
    fn run<'a, T: Send + 'a>(
        &self,
        work: impl Future<Output = Result<T, Status>> + Send + 'a,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<T, Status>> + Send + 'a>> {
        let budget = self.clone();
        Box::pin(async move {
            let deadline = async {
                match budget.deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! {biased;
                _=budget.host.cancelled_owned()=>Err(Status::unavailable("server closed")),
                _=budget.request.cancelled_owned()=>Err(Status::cancelled("context canceled")),
                _=deadline=>Err(Status::deadline_exceeded("context deadline exceeded")),
                result=work=>result,
            }
        })
    }
}

#[tonic::async_trait]
impl ResourceManager for RegisteredResourceManager {
    async fn list_resource_groups(
        &self,
        mut request: Request<p::ListResourceGroupsRequest>,
    ) -> Result<Response<p::ListResourceGroupsResponse>, Status> {
        let Some(proxy) = &self.proxy else {
            return ResourceManager::list_resource_groups(&self.local, request).await;
        };
        let budget = Budget::new(&request, &proxy.cancel);
        budget
            .run(async {
                let target = proxy.primary().await?;
                let mut client = proxy.client(&target).await?;
                budget.prepare(&mut request);
                let result = client.list_resource_groups(request).await;
                proxy.finish(&target, result)
            })
            .await
    }
    async fn get_resource_group(
        &self,
        mut request: Request<p::GetResourceGroupRequest>,
    ) -> Result<Response<p::GetResourceGroupResponse>, Status> {
        let Some(proxy) = &self.proxy else {
            return ResourceManager::get_resource_group(&self.local, request).await;
        };
        let budget = Budget::new(&request, &proxy.cancel);
        budget
            .run(async {
                let target = proxy.primary().await?;
                let mut client = proxy.client(&target).await?;
                budget.prepare(&mut request);
                let result = client.get_resource_group(request).await;
                proxy.finish(&target, result)
            })
            .await
    }
    async fn add_resource_group(
        &self,
        mut request: Request<p::PutResourceGroupRequest>,
    ) -> Result<Response<p::PutResourceGroupResponse>, Status> {
        let Some(proxy) = &self.proxy else {
            return ResourceManager::add_resource_group(&self.local, request).await;
        };
        if request.get_ref().group.is_none() {
            return Err(Status::invalid_argument("resource group is required"));
        }
        let budget = Budget::new(&request, &proxy.cancel);
        budget
            .run(async {
                let Some(target) = proxy.metadata_target(&request).await? else {
                    return ResourceManager::add_resource_group(&self.local, request).await;
                };
                request
                    .metadata_mut()
                    .insert("pd-forwarded-host", "".parse().expect("empty metadata"));
                let mut client = proxy.client(&target).await?;
                budget.prepare(&mut request);
                let result = client.add_resource_group(request).await;
                proxy.finish(&target, result)
            })
            .await
    }
    async fn modify_resource_group(
        &self,
        mut request: Request<p::PutResourceGroupRequest>,
    ) -> Result<Response<p::PutResourceGroupResponse>, Status> {
        let Some(proxy) = &self.proxy else {
            return ResourceManager::modify_resource_group(&self.local, request).await;
        };
        if request.get_ref().group.is_none() {
            return Err(Status::invalid_argument("resource group is required"));
        }
        let budget = Budget::new(&request, &proxy.cancel);
        budget
            .run(async {
                let Some(target) = proxy.metadata_target(&request).await? else {
                    return ResourceManager::modify_resource_group(&self.local, request).await;
                };
                request
                    .metadata_mut()
                    .insert("pd-forwarded-host", "".parse().expect("empty metadata"));
                let mut client = proxy.client(&target).await?;
                budget.prepare(&mut request);
                let result = client.modify_resource_group(request).await;
                proxy.finish(&target, result)
            })
            .await
    }
    async fn delete_resource_group(
        &self,
        mut request: Request<p::DeleteResourceGroupRequest>,
    ) -> Result<Response<p::DeleteResourceGroupResponse>, Status> {
        let Some(proxy) = &self.proxy else {
            return ResourceManager::delete_resource_group(&self.local, request).await;
        };
        if request.get_ref().resource_group_name.is_empty() {
            return Err(Status::invalid_argument("resource group name is required"));
        }
        let budget = Budget::new(&request, &proxy.cancel);
        budget
            .run(async {
                let Some(target) = proxy.metadata_target(&request).await? else {
                    return ResourceManager::delete_resource_group(&self.local, request).await;
                };
                request
                    .metadata_mut()
                    .insert("pd-forwarded-host", "".parse().expect("empty metadata"));
                let mut client = proxy.client(&target).await?;
                budget.prepare(&mut request);
                let result = client.delete_resource_group(request).await;
                proxy.finish(&target, result)
            })
            .await
    }
    async fn acquire_token_buckets(
        &self,
        request: Request<tonic::Streaming<p::TokenBucketsRequest>>,
    ) -> Result<Response<tonic::codegen::BoxStream<p::TokenBucketsResponse>>, Status> {
        let Some(proxy) = &self.proxy else {
            return ResourceManager::acquire_token_buckets(&self.local, request).await;
        };
        let budget = Budget::new(&request, &proxy.cancel);
        let target = budget.run(proxy.primary()).await?;
        let mut client = budget.run(proxy.client(&target)).await?;
        let failed = CancellationToken::new();
        let input_error = Arc::new(Mutex::new(None::<Status>));
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let upstream = stream::unfold(
            (input, budget.clone(), failed.clone(), input_error.clone()),
            |(mut input, budget, failed, error)| async move {
                match budget.run(input.message()).await {
                    Ok(Some(message)) => Some((message, (input, budget, failed, error))),
                    Ok(None) => None,
                    Err(status) => {
                        *error.lock().unwrap() = Some(status);
                        failed.cancel();
                        None
                    }
                }
            },
        );
        let upstream: std::pin::Pin<
            Box<dyn futures_util::Stream<Item = p::TokenBucketsRequest> + Send + 'static>,
        > = Box::pin(upstream);
        let mut request = Request::new(upstream);
        *request.metadata_mut() = metadata;
        budget.prepare(&mut request);
        let setup: std::pin::Pin<
            Box<
                dyn Future<
                        Output = Result<
                            Response<tonic::Streaming<p::TokenBucketsResponse>>,
                            Status,
                        >,
                    > + Send
                    + '_,
            >,
        > = Box::pin(client.acquire_token_buckets(request));
        let result=budget.run(async {tokio::select!{biased;
            _=failed.clone().cancelled_owned()=>Err(input_error.lock().unwrap().clone().unwrap_or_else(||Status::cancelled("context canceled"))),
            response=setup=>response,
        }}).await;
        let response = match result.map_err(delegate_error) {
            Ok(response) => response,
            Err(error) => {
                proxy.pool()?.invalidate(&target);
                return Err(error);
            }
        };
        let (metadata, remote, extensions) = response.into_parts();
        let proxy = proxy.clone();
        let output = stream::try_unfold(
            (proxy, target, remote, budget, failed, input_error),
            |(proxy, target, mut remote, budget, failed, input_error)| async move {
                let result=budget.run(async {tokio::select!{biased;
                _=failed.clone().cancelled_owned()=>Err(input_error.lock().unwrap().clone().unwrap_or_else(||Status::cancelled("context canceled"))),
                message=remote.message()=>message.map_err(delegate_error),
            }}).await;
                match result {
                    Ok(Some(message)) => Ok(Some((
                        message,
                        (proxy, target, remote, budget, failed, input_error),
                    ))),
                    Ok(None) => Ok(None),
                    Err(error) => {
                        if let Ok(pool) = proxy.pool() {
                            pool.invalidate(&target);
                        }
                        Err(error)
                    }
                }
            },
        );
        let output: tonic::codegen::BoxStream<p::TokenBucketsResponse> = Box::pin(output);
        Ok(Response::from_parts(metadata, output, extensions))
    }
}
#[cfg(test)]
#[path = "grpc_resource_proxy_test.rs"]
mod tests;

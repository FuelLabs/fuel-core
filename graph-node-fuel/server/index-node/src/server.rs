use hyper::service::make_service_fn;
use hyper::Server;
use std::net::{Ipv4Addr, SocketAddrV4};

use graph::{
    blockchain::BlockchainMap,
    components::store::Store,
    prelude::{IndexNodeServer as IndexNodeServerTrait, *},
};

use crate::service::IndexNodeService;
use thiserror::Error;

/// Errors that may occur when starting the server.
#[derive(Debug, Error)]
pub enum IndexNodeServeError {
    #[error("Bind error: {0}")]
    BindError(#[from] hyper::Error),
}

/// A GraphQL server based on Hyper.
pub struct IndexNodeServer<Q, S> {
    logger: Logger,
    blockchain_map: Arc<BlockchainMap>,
    graphql_runner: Arc<Q>,
    store: Arc<S>,
    link_resolver: Arc<dyn LinkResolver>,
}

impl<Q, S> IndexNodeServer<Q, S> {
    /// Creates a new GraphQL server.
    pub fn new(
        logger_factory: &LoggerFactory,
        blockchain_map: Arc<BlockchainMap>,
        graphql_runner: Arc<Q>,
        store: Arc<S>,
        link_resolver: Arc<dyn LinkResolver>,
    ) -> Self {
        let logger = logger_factory.component_logger(
            "IndexNodeServer",
            Some(ComponentLoggerConfig {
                elastic: Some(ElasticComponentLoggerConfig {
                    index: String::from("index-node-server-logs"),
                }),
            }),
        );

        IndexNodeServer {
            logger,
            blockchain_map,
            graphql_runner,
            store,
            link_resolver,
        }
    }
}

impl<Q, S> IndexNodeServerTrait for IndexNodeServer<Q, S>
where
    Q: GraphQlRunner,
    S: Store,
{
    type ServeError = IndexNodeServeError;

    fn serve(
        &mut self,
        port: u16,
    ) -> Result<Box<dyn Future<Item = (), Error = ()> + Send>, Self::ServeError> {
        let logger = self.logger.clone();

        info!(
            logger,
            "Starting index node server at: http://localhost:{}", port
        );

        let addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);

        // On every incoming request, launch a new GraphQL service that writes
        // incoming queries to the query sink.
        let logger_for_service = self.logger.clone();
        let graphql_runner = self.graphql_runner.clone();
        let store = self.store.clone();
        let service = IndexNodeService::new(
            logger_for_service.clone(),
            self.blockchain_map.clone(),
            graphql_runner,
            store,
            self.link_resolver.clone(),
        );
        let new_service =
            make_service_fn(move |_| futures03::future::ok::<_, Error>(service.clone()));

        // Create a task to run the server and handle HTTP requests
        let task = Server::try_bind(&addr.into())?
            .serve(new_service)
            .map_err(move |e| error!(logger, "Server error"; "error" => format!("{}", e)));

        Ok(Box::new(task.compat()))
    }
}

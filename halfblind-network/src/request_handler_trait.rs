use crate::ConnectionContext;
use async_trait::async_trait;
use halfblind_protobuf_network::ProtoResponse;
use std::error::Error;
use std::sync::Arc;

#[async_trait]
pub trait RequestHandler: Send + Sync {
    async fn handle(
        &self,
        message_id: u64,
        message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>>;
}

pub struct HandlerRegistration {
    pub type_url: fn () -> String,
    pub handler: fn() -> Arc<dyn RequestHandler + Send + Sync + 'static>,
}

// Create the collection point
inventory::collect!(HandlerRegistration);

#[macro_export]
macro_rules! request_handler {
    ($request:ident => $handler:ident) => {
        #[derive(Default)]
        pub struct $handler;

        #[async_trait::async_trait]
        impl halfblind_network::RequestHandler for $handler {
            async fn handle(
                &self,
                message_id: u64,
                message_timestamp: u64,
                payload: &[u8],
                ctx: std::sync::Arc<halfblind_network::ConnectionContext>,
            ) -> Result<halfblind_protobuf_network::ProtoResponse, Box<dyn std::error::Error + Send + Sync>> {
                let instant = std::time::Instant::now();
                let req = <$request as prost::Message>::decode(payload)?;
                // Call the local 'handle' function
                let result = handle(message_id, message_timestamp, req, ctx).await;
                #[cfg(feature = "profile-network-requests")]
                println!("{} took {:?}", stringify!($request), instant.elapsed());
                result
            }
        }
        halfblind_network::register_handler!($request, $handler);
    };
}

#[macro_export]
macro_rules! register_handler {
    ($request:ty, $handler:ty) => {
        inventory::submit! {
            halfblind_network::HandlerRegistration {
                type_url: || halfblind_protobuf::get_type_url::<$request>(),
                handler: || std::sync::Arc::new(<$handler>::default()),
            }
        }
    };
}


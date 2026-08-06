#[macro_export]
macro_rules! define_request_handler_type {
    ($service_locator_type:ty) => {
        #[async_trait::async_trait]
        pub trait RequestHandler: Send + Sync {
            async fn handle(
                &self,
                message_timestamp: u64,
                payload: &[u8],
                ctx: std::sync::Arc<halfblind_network::ConnectionContext>,
                services_locator: std::sync::Arc<$service_locator_type>,
            ) -> Result<halfblind_protobuf_network::ProtoResponse, halfblind_protobuf_network::ProtoResponse>;
        }

        pub struct HandlerRegistration {
            pub type_url: fn () -> String,
            pub handler: fn() -> std::sync::Arc<dyn RequestHandler + Send + Sync + 'static>,
        }
        
        // Use inventory crate to create a collection of handlers
        // this way we don't have to manually keep track of each handler that is created in another place
        inventory::collect!(HandlerRegistration);
    };
}

#[macro_export]
macro_rules! request_handler {
    ($request:ident => $response:ty, $service_locator_type:ty) => {
        type HandleResult = Result<$response, halfblind_protobuf_network::ProtoResponse>;

        #[derive(Default)]
        struct RequestHandlerImpl;

        #[async_trait::async_trait]
        impl RequestHandler for RequestHandlerImpl {
            async fn handle(
                &self,
                message_timestamp: u64,
                payload: &[u8],
                ctx: std::sync::Arc<halfblind_network::ConnectionContext>,
                services_locator: std::sync::Arc<$service_locator_type>,
            ) -> Result<halfblind_protobuf_network::ProtoResponse, halfblind_protobuf_network::ProtoResponse> {
                let instant = std::time::Instant::now();
                let req = halfblind_network::decode_or_error::<$request>(payload)?;
                // Call the local 'handle' function
                let result: HandleResult = handle(message_timestamp, req, ctx, services_locator.clone()).await;
                let result = result?;
                #[cfg(feature = "profile-network-requests")]
                println!("{} took {:?}", stringify!($request), instant.elapsed());
                halfblind_network::utils::encode_ok(&result)
            }
        }

        // Use inventory crate to save the type at compile time
        // This way we can keep track of all the created handlers in a single place
        inventory::submit! {
            HandlerRegistration {
                type_url: || halfblind_protobuf::get_type_url::<$request>(),
                handler: || std::sync::Arc::new(RequestHandlerImpl::default()),
            }
        }
    };
}

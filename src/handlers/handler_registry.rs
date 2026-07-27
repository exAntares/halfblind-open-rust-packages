use crate::services::services::Services;
use dashmap::DashMap;
use halfblind_network::define_request_handler_type;
use once_cell::sync::Lazy;
use std::sync::Arc;

define_request_handler_type!(Services);

pub static HANDLER_REGISTRY_BY_ANY_TYPE: Lazy<DashMap<String, Arc<dyn RequestHandler + Send + Sync + 'static>>,> = Lazy::new(|| {
    let map = DashMap::new();
    for registration in inventory::iter::<HandlerRegistration> {
        println!("Registering handler for type: {}", (registration.type_url)());
        map.insert((registration.type_url)(), (registration.handler)().clone());
    }
    map
});
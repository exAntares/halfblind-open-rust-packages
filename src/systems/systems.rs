use crate::characters::characters_service::CharactersService;
use crate::characters::characters_service_impl::CharactersServiceImpl;
use crate::inventory::inventory_item_utils::{generate_inventory_item_for_player_default, try_aggregate_inventories};
use crate::map::maps_service::MapsService;
use crate::map::maps_service_impl::MapsServiceImpl;
use crate::map_update::maps_update_service::MapsUpdateService;
use crate::map_update::maps_update_service_impl::MapsUpdateServiceImpl;
use halfblind_database_service::{DatabaseService, DatabaseServiceImpl};
use halfblind_inventory_service::{InventoryService, InventoryServiceImpl};
use halfblind_itemdefinitions_service::{ItemDefinitionsService, ItemDefinitionsServiceImpl};
use protobuf_itemdefinition::ItemDefinitionsResponse;
use halfblind_random::{RandomService, RandomServiceImpl};
use once_cell::sync::{Lazy, OnceCell};
use prost::Message;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

// Having the bytes at compile time is amazing
const ITEM_DEFS_BYTES: &[u8] = include_bytes!("../../data/ItemDefinitions.bytes");

static ITEM_DEFINITIONS_RESPONSE_DEFAULT: Lazy<ItemDefinitionsResponse> =
    Lazy::new(|| ItemDefinitionsResponse::decode(ITEM_DEFS_BYTES).unwrap());

pub static POOL: OnceCell<Arc<Pool<Postgres>>> = OnceCell::new();
pub static SYSTEMS: Lazy<Arc<Systems>> = Lazy::new(|| {
    let pool = POOL.get().expect("Database POOL must be initialized before accessing SYSTEMS");
    println!("Creating Systems...");
    let seed: [u8; 32] = rand::random();
    let random_service = Arc::new(RandomServiceImpl::new(seed));
    let items_definitions_impl = Arc::new(ItemDefinitionsServiceImpl::new(
        &ITEM_DEFINITIONS_RESPONSE_DEFAULT
    ));
    let database_impl = Arc::new(DatabaseServiceImpl::new(pool.clone()));
    let characters_impl = Arc::new(CharactersServiceImpl::new(
        database_impl.clone(),
        items_definitions_impl.clone(),
        random_service.clone(),
    ));
    let inventory_service_impl = Arc::new(InventoryServiceImpl::new(
        database_impl.clone(),
        Arc::new(try_aggregate_inventories),
        Arc::new(generate_inventory_item_for_player_default),
    ));
    let maps_update_service = Arc::new(MapsUpdateServiceImpl::new(
        characters_impl.clone(),
        items_definitions_impl.clone(),
        inventory_service_impl.clone(),
        random_service.clone(),
    ));
    let systems = Arc::new(Systems::new(
        database_impl,
        characters_impl,
        items_definitions_impl,
        inventory_service_impl,
        maps_update_service,
        random_service,
    ));
    systems
});

pub struct Systems {
    // Arc → allows sharing across threads.
    // RwLock → allows multiple concurrent readers or exclusive writers.
    // You must ensure your impl implements Send + Sync (or wrap it properly).
    pub characters_service: Arc<dyn CharactersService + Send + Sync>,
    pub maps_service: Arc<dyn MapsService + Send + Sync>,
    pub database_service: Arc<dyn DatabaseService + Send + Sync>,
    pub items_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
    pub inventory_service: Arc<dyn InventoryService + Send + Sync>,
    pub maps_update_service: Arc<dyn MapsUpdateService + Send + Sync>,
    pub random_service: Arc<dyn RandomService + Send + Sync>,
}

impl Systems {
    pub fn new(
        database_service: Arc<dyn DatabaseService + Send + Sync>,
        characters_service: Arc<dyn CharactersService + Send + Sync>,
        items_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
        inventory_service: Arc<dyn InventoryService + Send + Sync>,
        maps_update_service: Arc<dyn MapsUpdateService + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
    ) -> Self {
        Self {
            maps_service: Arc::new(MapsServiceImpl::new(
                characters_service.clone(),
                items_definitions_service.clone(),
                inventory_service.clone(),
                maps_update_service.clone(),
            )),
            database_service,
            characters_service,
            items_definitions_service,
            inventory_service,
            maps_update_service,
            random_service,
        }
    }
}

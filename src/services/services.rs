use crate::characters::characters_service::CharactersService;
use crate::characters::characters_service_impl::CharactersServiceImpl;
use crate::inventory::inventory_service_impl::InventoryServiceImpl;
use crate::item_definitions::ItemDefinitionLookupServiceImpl;
use crate::map::maps_service::MapsService;
use crate::map::maps_service_impl::MapsServiceImpl;
use crate::map_update::maps_update_service::MapsUpdateService;
use crate::map_update::maps_update_service_impl::MapsUpdateServiceImpl;
use crate::transactions::transaction_service_impl::TransactionServiceImpl;
use halfblind_database_service::{DatabaseService, DatabaseServiceImpl};
use halfblind_inventory_service::InventoryService;
use halfblind_itemdefinitions_service::{ItemDefinitionsService, ItemDefinitionsServiceImpl};
use halfblind_random::{RandomService, RandomServiceImpl};
use halfblind_transactions::TransactionService;
use prost::Message;
use proto_gen::InventoryItem;
use protobuf_itemdefinition::ItemDefinitionsResponse;
use sqlx::{Pool, Postgres};
use std::sync::{Arc, LazyLock, OnceLock};

// Having the bytes at compile time is amazing
const ITEM_DEFS_BYTES: &[u8] = include_bytes!("../../data/ItemDefinitions.bytes");

static ITEM_DEFINITIONS_RESPONSE_DEFAULT: LazyLock<ItemDefinitionsResponse> =
    LazyLock::new(|| ItemDefinitionsResponse::decode(ITEM_DEFS_BYTES).unwrap());

pub static POOL: OnceLock<Arc<Pool<Postgres>>> = OnceLock::new();

pub fn create_arc_services() -> Arc<Services> {
    let pool = POOL.get().expect("Database POOL must be initialized before accessing services");
    println!("Creating Systems...");
    let item_definition_lookup_service = Arc::new(ItemDefinitionLookupServiceImpl::default());
    let transaction_service = Arc::new(TransactionServiceImpl::default());
    let random_service = Arc::new(RandomServiceImpl::new(rand::random()));
    let items_definitions_impl = Arc::new(ItemDefinitionsServiceImpl::new(
        &ITEM_DEFINITIONS_RESPONSE_DEFAULT
    ));
    let database_impl = Arc::new(DatabaseServiceImpl::new(pool.clone()));
    let characters_impl = Arc::new(CharactersServiceImpl::new(
        database_impl.clone(),
        item_definition_lookup_service.clone(),
        random_service.clone(),
    ));
    let inventory_service_impl = Arc::new(InventoryServiceImpl::new(
        database_impl.clone(),
        random_service.clone(),
        item_definition_lookup_service.clone(),
    ));
    let maps_update_service = Arc::new(MapsUpdateServiceImpl::new(
        characters_impl.clone(),
        item_definition_lookup_service.clone(),
        inventory_service_impl.clone(),
        random_service.clone(),
    ));
    let systems = Arc::new(Services::new(
        database_impl,
        characters_impl,
        items_definitions_impl,
        inventory_service_impl,
        maps_update_service,
        random_service,
        transaction_service,
        item_definition_lookup_service,
    ));
    systems
}

pub struct Services {
    // Arc → allows sharing across threads.
    // RwLock → allows multiple concurrent readers or exclusive writers.
    // You must ensure your impl implements Send + Sync (or wrap it properly).
    pub characters_service: Arc<dyn CharactersService + Send + Sync>,
    pub maps_service: Arc<dyn MapsService + Send + Sync>,
    pub database_service: Arc<dyn DatabaseService + Send + Sync>,
    pub items_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
    pub inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
    pub maps_update_service: Arc<dyn MapsUpdateService + Send + Sync>,
    pub random_service: Arc<dyn RandomService + Send + Sync>,
    pub transaction_service: Arc<dyn TransactionService<InventoryItem> + Send + Sync>,
    pub item_definition_lookup_service: Arc<ItemDefinitionLookupServiceImpl>,
}

impl Services {
    pub fn new(
        database_service: Arc<dyn DatabaseService + Send + Sync>,
        characters_service: Arc<dyn CharactersService + Send + Sync>,
        items_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        maps_update_service: Arc<dyn MapsUpdateService + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
        transition_service: Arc<dyn TransactionService<InventoryItem> + Send + Sync>,
        item_definition_lookup_service: Arc<ItemDefinitionLookupServiceImpl>,
    ) -> Self {
        Self {
            maps_service: Arc::new(MapsServiceImpl::new(
                characters_service.clone(),
                items_definitions_service.clone(),
                item_definition_lookup_service.clone(),
                inventory_service.clone(),
                maps_update_service.clone(),
                database_service.clone(),
            )),
            database_service,
            characters_service,
            items_definitions_service,
            inventory_service,
            maps_update_service,
            random_service,
            transaction_service: transition_service,
            item_definition_lookup_service,
        }
    }
}

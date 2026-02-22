use crate::ItemDefinitionsService;
use protobuf_itemdefinition::ItemDefinition;
use protobuf_itemdefinition::ItemDefinitionsResponse;
use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::sync::OnceLock;
use uuid::Uuid;

pub struct ItemDefinitionsServiceImpl {
    default_item_definitions_by_id: OnceLock<HashMap<u64, &'static ItemDefinition>>,
    item_definitions_response_default: &'static ItemDefinitionsResponse,
}

impl ItemDefinitionsService for ItemDefinitionsServiceImpl {
    fn get_item_definitions_response_for_player(
        &self,
        player_uuid: Uuid,
    ) -> Result<&'static ItemDefinitionsResponse, Box<dyn Error + Send + Sync>> {
        Ok(&self.item_definitions_response_default)
    }

    fn get_item_definition_for_player(
        &self,
        _player_uuid: Uuid,
        id: u64,
    ) -> Option<&'static ItemDefinition> {
        self.default_item_definitions_by_id
            .get()
            .and_then(|map| map.get(&id))
            .copied()
    }
}

impl ItemDefinitionsServiceImpl {
    pub fn new(
        item_definitions_response_default: &'static ItemDefinitionsResponse,
    ) -> Self {
        let default_item_definitions_by_id = load_from_file(&item_definitions_response_default.definitions).unwrap();
        Self {
            default_item_definitions_by_id,
            item_definitions_response_default,
        }
    }
}

pub fn load_from_file(definitions: &'static Vec<ItemDefinition>) -> Result<OnceLock<HashMap<u64, &'static ItemDefinition>>, Box<dyn Error>> {
    // Assuming `ItemDefinition` has a field `id: u64`
    let map: HashMap<u64, &ItemDefinition> =
        definitions.into_iter().map(|def| (def.id, def)).collect();
    let result = OnceLock::new();
    result
        .set(map)
        .map_err(|_| io::Error::new(io::ErrorKind::AlreadyExists, "Items already initialized"))?;
    Ok(result)
}

use protobuf_itemdefinition::ItemDefinition;
use protobuf_itemdefinition::ItemDefinitionsResponse;
use std::error::Error;
use uuid::Uuid;

pub trait ItemDefinitionsService {
    fn get_item_definitions_response_for_player(
        &self,
        player_uuid: Uuid,
    ) -> Result<&'static ItemDefinitionsResponse, Box<dyn Error + Send + Sync>>;

    fn get_item_definition_for_player(
        &self,
        player_uuid: Uuid,
        item_definition_id: u64,
    ) -> Option<&'static ItemDefinition>;
}

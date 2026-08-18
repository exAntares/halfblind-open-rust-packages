use prost::Message;
use protobuf_itemdefinition::ItemDefinitionsResponse;
use protobuf_itemdefinition::build_utils::generate_item_definition_service;
use std::sync::LazyLock;

const ITEM_DEFS_BYTES: &[u8] = include_bytes!("data/ItemDefinitions.bytes");
static ITEM_DEFINITIONS_RESPONSE_DEFAULT: LazyLock<ItemDefinitionsResponse> =
    LazyLock::new(|| ItemDefinitionsResponse::decode(ITEM_DEFS_BYTES).unwrap());

fn main() {
    // Tell Cargo to rerun the build script if ItemDefinitions.bytes changes
    println!("cargo:rerun-if-changed=data/ItemDefinitions.bytes");

    let proto_path = vec!["proto-gen/src/protobuf_game.rs", "protobuf-itemdefinition/src/protobuf_itemdefinition.rs"];
    generate_item_definition_service(ITEM_DEFINITIONS_RESPONSE_DEFAULT.definitions.as_slice(), proto_path);
}


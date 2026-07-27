use once_cell::sync::Lazy;
use prost::Message;
use protobuf_itemdefinition::build_utils::generate_item_definition_service;
use protobuf_itemdefinition::ItemDefinitionsResponse;
use std::collections::HashSet;
use std::fs;

const ITEM_DEFS_BYTES: &[u8] = include_bytes!("data/ItemDefinitions.bytes");
static ITEM_DEFINITIONS_RESPONSE_DEFAULT: Lazy<ItemDefinitionsResponse> =
    Lazy::new(|| ItemDefinitionsResponse::decode(ITEM_DEFS_BYTES).unwrap());

fn main() {
    // Tell Cargo to rerun the build script if ItemDefinitions.bytes changes
    println!("cargo:rerun-if-changed=data/ItemDefinitions.bytes");

    let proto_path = vec!["proto-gen/src/protobuf_game.rs", "protobuf-itemdefinition/src/protobuf_itemdefinition.rs"];
    generate_item_definition_service(ITEM_DEFINITIONS_RESPONSE_DEFAULT.definitions.as_slice(), proto_path);
}

fn get_all_proto_ending_with(proto_path: &str, ends_with: &str) -> HashSet<String> {
    match fs::exists(proto_path) {
        Ok(exists) => {
            if !exists {
                panic!("Protobuf file {} does not exist", proto_path);
            }
        }
        Err(e) => {
            panic!("Failed to read protobuf_game: {}", e);
        }
    }
    let content = fs::read_to_string(proto_path).expect("Failed to read protobuf_game");

    let mut requests = HashSet::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("pub struct ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let name_part = parts[2];
                // Clean up in case the opening brace is attached or there are other characters
                let struct_name = name_part.trim_end_matches('{').trim();

                if struct_name.ends_with(ends_with) {
                    requests.insert(struct_name.to_string());
                }
            }
        }
    }
    requests
}

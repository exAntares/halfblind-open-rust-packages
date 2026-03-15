
#[cfg(test)]
mod gen_test {
    // Import the singleton components from the proto-gen crate
    use crate::protobuf_game::{
        AbilityPointsPerLevelSingletonComponent,
        InventoryItemBaseBuyFromNpcValueSingletonComponent,
        MaximumVisibleInventoryCapacitySingletonComponent,
    };
    use prost::bytes::Bytes;
    use prost::Message;
    use prost_types::Any;
    use protobuf_itemdefinition::{ItemDefinition, ItemDefinitionsResponse};
    use std::fs;
    use std::path::Path;

    fn pack_any<T: Message>(type_name: &str, message: &T) -> Any {
        Any {
            type_url: format!("type.googleapis.com/{}", type_name),
            value: message.encode_to_vec(),
        }
    }

    #[test]
    fn generate_item_definitions_bytes() {
        // Create singleton item definitions with unique IDs
        let mut definitions = Vec::new();

        // AbilityPointsPerLevelSingletonComponent
        let ability_points = AbilityPointsPerLevelSingletonComponent {
            points_per_level: 5,
        };
        definitions.push(ItemDefinition {
            id: 1000001,
            any_components: vec![pack_any(
                "protobuf_game.AbilityPointsPerLevelSingletonComponent",
                &ability_points,
            )],
        });

        // MaximumVisibleInventoryCapacitySingletonComponent
        let max_inventory = MaximumVisibleInventoryCapacitySingletonComponent {
            capacity: 50,
        };
        definitions.push(ItemDefinition {
            id: 1000002,
            any_components: vec![pack_any(
                "protobuf_game.MaximumVisibleInventoryCapacitySingletonComponent",
                &max_inventory,
            )],
        });

        // InventoryItemBaseBuyFromNpcValueSingletonComponent
        let buy_value = InventoryItemBaseBuyFromNpcValueSingletonComponent {
            percentage_value: 1.5,
        };
        definitions.push(ItemDefinition {
            id: 1000003,
            any_components: vec![pack_any(
                "protobuf_game.InventoryItemBaseBuyFromNpcValueSingletonComponent",
                &buy_value,
            )],
        });

        // Create the response
        let response = ItemDefinitionsResponse { definitions };

        // Encode to bytes
        let mut out = Vec::new();
        let bytes = match response.encode(&mut out) {
            Ok(_) => Bytes::from(out),
            Err(_) => {
                eprintln!("Failed to encode response");
                return;
            },
        };

        // Write to file
        let output_path = Path::new("../data/ItemDefinitions.bytes");
        fs::create_dir_all(output_path.parent().unwrap()).unwrap();
        fs::write(output_path, &bytes).expect("Failed to write ItemDefinitions.bytes");

        println!(
            "Generated ItemDefinitions.bytes with {} definitions ({} bytes)",
            response.definitions.len(),
            bytes.len()
        );
    }
}
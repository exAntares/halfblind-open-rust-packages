use once_cell::sync::Lazy;
use prost::Message;
use protobuf_itemdefinition::{ItemDefinition, ItemDefinitionsResponse};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;

const ITEM_DEFS_BYTES: &[u8] = include_bytes!("data/ItemDefinitions.bytes");
static ITEM_DEFINITIONS_RESPONSE_DEFAULT: Lazy<ItemDefinitionsResponse> =
    Lazy::new(|| ItemDefinitionsResponse::decode(ITEM_DEFS_BYTES).unwrap());

fn main() {
    // Tell Cargo to rerun the build script if ItemDefinitions.bytes changes
    println!("cargo:rerun-if-changed=data/ItemDefinitions.bytes");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("component_lookups.rs");

    // Discover all unique component types
    let component_types =
        discover_component_types(ITEM_DEFINITIONS_RESPONSE_DEFAULT.definitions.as_slice());

    // Generate the component lookups
    let generated_code = generate_component_lookups(&component_types);

    // Write the generated code to OUT_DIR
    fs::write(&dest_path, generated_code).expect("Failed to write component_lookups.rs");
    
    generate_network_handlers_file();
}

fn generate_network_handlers_file() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("network_handlers.rs");
    let proto_path = "proto-gen/src/protobuf_game.rs";
    let proto_requests: HashSet<String> = get_all_proto_ending_with(proto_path, "Request");
    let generated_code = generate_network_handlers(&proto_requests);
    fs::write(&dest_path, generated_code).expect("Failed to write network_handlers.rs");
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

fn generate_network_handlers(proto_requests: &HashSet<String>) -> String {
    let mut code = String::new();
    let src_modules = get_all_src_modules();
    for module in src_modules {
        if module == "handlers" {
            continue;
        }
        code.push_str(&format!("use crate::{}::*;\n", module));
    }
    code.push_str("use dashmap::DashMap;\n");
    code.push_str("use once_cell::sync::Lazy;\n");
    code.push_str("use std::sync::{Arc, Mutex};\n");
    code.push_str("use halfblind_network::*;\n");
    code.push_str("use halfblind_protobuf::*;\n");
    code.push_str("use proto_gen::*;\n");
    code.push_str("\n");
    code.push_str("pub static HANDLER_REGISTRY_BY_ANY_TYPE: Lazy<DashMap<String, Arc<dyn RequestHandler + Send + Sync + 'static>>,> = Lazy::new(|| {\n");
    code.push_str("    let map = DashMap::new();\n");
    // Generate hashmaps for each component type
    for proto_request in proto_requests {
        if proto_request == "ProtoRequest" {
            // ProtoRequest does not need a handler, it is actually a wrapper for all the other requests
            continue;
        }
        code.push_str("        map.insert(\n");
        code.push_str(&format!("            get_type_url::<{}>(),\n", proto_request));
        code.push_str(&format!("            Arc::new({}::default()) as Arc<dyn RequestHandler + Send + Sync + 'static>,\n", proto_request.replace("Request", "Handler")));
        code.push_str("        );\n");
    }

    code.push_str("for registration in inventory::iter::<HandlerRegistration> {\n");
    code.push_str("     map.insert((registration.type_url)(), (registration.handler)().clone());\n");
    code.push_str("}\n");

    code.push_str("    map\n");
    code.push_str("});\n");
    code
}

fn get_all_src_modules() -> Vec<String> {
    let src_path = Path::new("src");
    let mut modules = Vec::new();
    if let Ok(entries) = fs::read_dir(src_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        modules.push(name.to_string());
                    }
                }
            }
        }
    }
    modules
}

fn generate_component_lookups(component_types: &HashSet<String>) -> String {
    let mut code = String::new();
    for component_type in component_types {
        code.push_str(&format!("// {}\n", component_type));
    }
    code.push_str("use std::collections::HashMap;\n");
    code.push_str("use once_cell::sync::Lazy;\n");
    code.push_str("use proto_gen::*;\n");
    code.push_str("use prost::Message;\n");
    code.push_str("use ::protobuf_itemdefinition::*;\n");
    code.push_str("\n");
    code.push_str("const ITEM_DEFS_BYTES: &[u8] = include_bytes!(\"../../../../../data/ItemDefinitions.bytes\");\n\n");

    code.push_str(
        "static ITEM_DEFINITIONS_RESPONSE_DEFAULT: Lazy<ItemDefinitionsResponse> =\
     Lazy::new(|| ItemDefinitionsResponse::decode(ITEM_DEFS_BYTES).unwrap());\n\n",
    );

    // Generate hashmaps for each component type
    for component_type in component_types {
        let static_name = component_type_to_static_name(component_type);
        code.push_str(&generate_component_lookup(
            component_type,
            static_name,
            ITEM_DEFINITIONS_RESPONSE_DEFAULT.definitions.as_slice(),
        ));
    }
    code
}

fn discover_component_types(
    definitions: &[ItemDefinition],
) -> HashSet<String> {
    let mut component_types = HashSet::new();

    for def in definitions {
        for component in &def.any_components {
            // Extract the component type name from the type_url
            if let Some(type_name) = extract_component_type_name(&component.type_url) {
                component_types.insert(type_name);
            }
        }
    }

    let proto_path = "proto-gen/src/protobuf_game.rs";
    let proto_components = get_all_proto_ending_with(proto_path,"Component");
    for proto_component in proto_components {
        if !component_types.contains(&proto_component) {
            component_types.insert(format!("proto_balancing::{}",proto_component));
        }
    }

    component_types
}

fn extract_component_type_name(type_url: &str) -> Option<String> {
    // Extract the last part of the type URL (e.g., "CharacterDefinitionComponent" from "type.googleapis.com/CharacterDefinitionComponent")
    type_url.split('/').last().map(|s| s.replace('.', "::"))
}

fn generate_component_lookup(
    component_type: &str,
    static_name: String,
    definitions: &[ItemDefinition],
) -> String {
    let mut code = String::new();
    let plain_name : String = component_type.split("::").last().unwrap().to_string();
    code.push_str(&format!(
        "pub static {}: Lazy<HashMap<u64, {}>> = Lazy::new(|| {{\n",
        static_name, plain_name
    ));
    code.push_str("    let mut map = HashMap::new();\n");

    // Add components to the map
    for (def_index, def) in definitions.iter().enumerate() {
        for (component_index, component) in def.any_components.iter().enumerate() {
            let component_type_url =
                format!("type.googleapis.com/{}", component_type.replace("::", "."));
            if component.type_url.contains(component_type_url.as_str()) {
                code.push_str("     {\n");
                code.push_str(&format!("          let component = &ITEM_DEFINITIONS_RESPONSE_DEFAULT.definitions[{}].any_components[{}];\n", def_index, component_index));
                code.push_str(&format!(
                    "          if let Ok(decoded_component) = {}::decode(component.value.as_slice()) {{\n",
                    plain_name
                ));
                code.push_str(&format!(
                    "          map.insert({}, decoded_component);\n",
                    def.id
                ));
                code.push_str("          }\n");
                code.push_str("     }\n");
            }
        }
    }

    code.push_str("    map\n");
    code.push_str("});\n\n");
    code
}

fn component_type_to_static_name(component_type: &str) -> String {
    // Convert CamelCase to SCREAMING_SNAKE_CASE and add _LOOKUP suffix
    let result = component_type.split("::").last().unwrap().to_string();
    format!("{}Lookup", result)
}

use std::fs;

fn main() {
    // Also rerun if the file we are fixing changes
    println!("cargo:rerun-if-changed=src/protobuf_game.rs");
    fix_protobuf_game_imports();
}

fn fix_protobuf_game_imports() {
    let proto_path = "src/protobuf_game.rs";
    if let Ok(content) = fs::read_to_string(proto_path) {
        let import_line = "use protobuf_itemdefinition::protobuf_itemdefinition;";
        let mut new_content = content.clone();

        // 1. Add import if not there
        if !new_content.contains(import_line) {
            //new_content = format!("{}\n{}", import_line, new_content);
        }

        // 2. Replace super::protobuf_itemdefinition => protobuf_itemdefinition
        new_content = new_content.replace("super::protobuf_itemdefinition", "protobuf_itemdefinition");

        if new_content != content {
            fs::write(proto_path, new_content).expect("Failed to update protobuf_game.rs");
        }
    }
}
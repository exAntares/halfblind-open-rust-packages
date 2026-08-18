use prost::Message;

pub fn get_type_url<M: Message>() -> String {
    // This is because typename would return something like "rust_grpc_server::generated::common::LoginResponse"
    // But we need "common.LoginResponse"
    let type_name = std::any::type_name::<M>()
        .split("::")
        .collect::<Vec<_>>() // collect into vector first
        .into_iter()
        .rev() // now we can reverse
        .take(2) // take 2 items
        .collect::<Vec<_>>() // collect into vector
        .into_iter()
        .rev() // reverse back to original order
        .collect::<Vec<_>>() // collect final result
        .join(".");
    format!("type.googleapis.com/{}", type_name)
}

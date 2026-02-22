use prost::Message;
use std::error::Error;

/// Pack any prost message into `google.protobuf.Any`
pub fn pack_any<M: Message>(msg: &M) -> prost_types::Any {
    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("Failed to encode message");
    prost_types::Any {
        type_url: get_type_url::<M>(),
        value: buf,
    }
}

/// Attempts to unpack a `prost_types::Any` into a concrete message `M`.
pub fn unpack_any<M: Message + Default>(msg: prost_types::Any) -> Result<M, Box<dyn Error>> {
    // Check that the type_url matches the expected type
    if msg.type_url != get_type_url::<M>() {
        return Err(format!("Type mismatch: expected `{}`, got `{}`", get_type_url::<M>(), msg.type_url).into());
    }
    // Decode the message safely
    let result = M::decode(msg.value.as_slice())?;
    Ok(result)
}

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

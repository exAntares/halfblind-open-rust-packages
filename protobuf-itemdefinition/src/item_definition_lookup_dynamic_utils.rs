use crate::protobuf_utils::get_type_url;
use prost::Message;
use prost_types::Any;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug)]
pub enum ItemDefinitionLookupDynamicError {
    MissingSingletonData(String),
}

pub fn get_hash_map<T: Message + Default>(
    indexes_by_component_type_url: &HashMap<String, Vec<(u64, &Any)>>,
) -> HashMap<u64, Arc<T>> {
    let mut result: HashMap<u64, Arc<T>> = Default::default();
    let component_type_url = get_type_url::<T>();
    if let Some(indexes) = indexes_by_component_type_url.get(&component_type_url){
        indexes.iter().for_each(|(id, any)|
            if let Ok(decoded_component) = T::decode(any.value.as_slice()) {
                result.insert(*id, Arc::new(decoded_component));
            });
    }
    result
}

pub fn get_singleton_component<T: Message + Default>(
    indexes_by_component_type_url: &HashMap<String, Vec<(u64, &Any)>>,
) -> Result<(u64, Arc<T>), ItemDefinitionLookupDynamicError> {
    let component_type_url = get_type_url::<T>();
    let indexes = indexes_by_component_type_url.get(&component_type_url)
        .ok_or_else(||ItemDefinitionLookupDynamicError::MissingSingletonData(format!("{} missing",component_type_url).into()))?;
    if indexes.is_empty() {
        return Err(ItemDefinitionLookupDynamicError::MissingSingletonData(format!("{} missing",component_type_url).into()))
    }
    let (id, any) = &indexes[0];
    if let Ok(decoded_component) = T::decode(any.value.as_slice()) {
        return Ok((*id, Arc::new(decoded_component)));
    }
    Err(ItemDefinitionLookupDynamicError::MissingSingletonData(format!("failed to decode data for {}",component_type_url).into()))
}
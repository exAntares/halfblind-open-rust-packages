use proto_gen::CharacterDefinitionComponent;
use std::sync::Arc;

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct DatabaseCharacter {
    pub player_uuid: uuid::Uuid,
    pub character_uuid: uuid::Uuid,
    pub character_definition_id: i64,
    pub strength: i32,
    pub agility: i32,
    pub intelligence: i32,
    pub vitality: i32,
    pub current_hp: i32,
    pub character_name: String,
}

#[derive(Clone, Debug)]
pub struct Character {
    pub character_db: DatabaseCharacter,
    pub character_definition: Arc<CharacterDefinitionComponent>,
}

impl Character {
    pub fn get_intelligence(&self) -> i32 {
        self.character_db.intelligence + self.character_definition.base_intelligence
    }

    pub fn get_strength(&self) -> i32 {
        self.character_db.strength + self.character_definition.base_strength
    }

    pub fn get_agility(&self) -> i32 {
        self.character_db.agility + self.character_definition.base_agility
    }
}

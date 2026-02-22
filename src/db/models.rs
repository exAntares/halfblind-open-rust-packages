use uuid::Uuid;

#[derive(sqlx::FromRow, Debug, Clone)]
pub struct PlayerItem {
    pub item_instance_id: Uuid,
    pub item_definition_id: i64,
    pub quantity: i64,
    pub is_equipped: bool,

    // Raw DB columns for rolled attributes (up to 3)
    pub attr_enum_1: Option<i32>,
    pub attr_val_1: Option<f64>,
    pub attr_enum_2: Option<i32>,
    pub attr_val_2: Option<f64>,
    pub attr_enum_3: Option<i32>,
    pub attr_val_3: Option<f64>,

    // Raw DB columns for player-custom attribute (up to 1)
    pub user_attr_enum_1: Option<i32>,
    pub usr_attr_val_1: Option<f64>,

    // Derived fields (not mapped directly from DB)
    #[sqlx(skip)]
    pub rolled_attributes: Vec<PlayerItemAttribute>,
    #[sqlx(skip)]
    pub player_custom_attributes: Vec<PlayerItemAttribute>,
}

#[derive(Debug, Clone)]
pub struct PlayerItemAttribute {
    pub attribute_enum: i32,
    pub attribute_val: f64,
}

impl PlayerItem {
    /// Populate the derived attribute vectors from the optional raw DB columns.
    pub fn populate_attributes(mut self) -> Self {
        // Rolled attributes
        if let (Some(id), Some(val)) = (self.attr_enum_1, self.attr_val_1) {
            self.rolled_attributes.push(PlayerItemAttribute {
                attribute_enum: id,
                attribute_val: val,
            });
        }
        if let (Some(id), Some(val)) = (self.attr_enum_2, self.attr_val_2) {
            self.rolled_attributes.push(PlayerItemAttribute {
                attribute_enum: id,
                attribute_val: val,
            });
        }
        if let (Some(id), Some(val)) = (self.attr_enum_3, self.attr_val_3) {
            self.rolled_attributes.push(PlayerItemAttribute {
                attribute_enum: id,
                attribute_val: val,
            });
        }

        // Player custom attributes
        if let (Some(id), Some(val)) = (self.user_attr_enum_1, self.usr_attr_val_1) {
            self.player_custom_attributes.push(PlayerItemAttribute {
                attribute_enum: id,
                attribute_val: val,
            });
        }

        self
    }
}

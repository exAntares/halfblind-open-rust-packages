use crate::Position;

impl std::ops::Sub for Position {
    type Output = Position;

    fn sub(self, other: Position) -> Position {
        Position {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl Position {
    /// Returns the length (magnitude) of the vector
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Returns a normalized version of this vector (length = 1)
    /// Returns a zero vector if the length is zero
    pub fn normalize(&self) -> Position {
        let len = self.length();
        if len > 0.0 {
            Position {
                x: self.x / len,
                y: self.y / len,
            }
        } else {
            Position { x: 0.0, y: 0.0 }
        }
    }
}

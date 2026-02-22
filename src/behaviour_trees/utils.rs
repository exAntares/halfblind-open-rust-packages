use crate::map::models::TargetPositions;
use proto_gen::Position;

pub fn move_to_positions(
    position: &mut Position,
    target_positions: &mut TargetPositions,
    speed: f32,
    delta_time: f32,
) -> bool {
    if target_positions.positions.len() <= target_positions.current_index {
        // We reached the end of the list, stop moving
        return true;
    } else {
        let target_position = target_positions.positions[target_positions.current_index];
        move_to(position, target_position, speed, delta_time);
        if *position == target_position {
            // move to the next point
            target_positions.current_index += 1;
        }
    }
    false
}

pub fn move_to(position: &mut Position, target_position: Position, speed: f32, delta_time: f32) {
    let direction_x = target_position.x - position.x;
    let direction_y = target_position.y - position.y;
    let distance = (direction_x * direction_x + direction_y * direction_y).sqrt();
    if distance > 0.0 {
        let step = speed * delta_time;
        if step > distance {
            position.x = target_position.x;
            position.y = target_position.y;
        } else {
            let norm_x = direction_x / distance;
            let norm_y = direction_y / distance;
            position.x += norm_x * step;
            position.y += norm_y * step;
        }
    }
}

use glam::{Vec2, Vec3};

use proto_gen::MeshData;
use std::collections::HashMap;
use std::sync::Arc;

// Previous helper functions remain the same
fn line_segments_intersect(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> bool {
    let r = Vec2::new(a2.x - a1.x, a2.y - a1.y);
    let s = Vec2::new(b2.x - b1.x, b2.y - b1.y);

    let rxs = r.perp_dot(s);
    let q_p = Vec2::new(b1.x - a1.x, b1.y - a1.y);

    if rxs.abs() < f32::EPSILON {
        return false;
    }

    let t = q_p.perp_dot(s) / rxs;
    let u = q_p.perp_dot(r) / rxs;

    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

fn segment_intersection_with_t(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> Option<(Vec2, f32)> {
    // Returns intersection point and t along a1->a2 if segments intersect, else None
    let r = a2 - a1;
    let s = b2 - b1;
    let rxs = r.perp_dot(s);
    let q_p = b1 - a1;
    if rxs.abs() < f32::EPSILON {
        return None;
    }
    let t = q_p.perp_dot(s) / rxs;
    let u = q_p.perp_dot(r) / rxs;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some((a1 + t * r, t))
    } else {
        None
    }
}

fn point_in_triangle_2d(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    // Barycentric technique
    let v0 = c - a;
    let v1 = b - a;
    let v2 = p - a;
    let dot00 = v0.dot(v0);
    let dot01 = v0.dot(v1);
    let dot02 = v0.dot(v2);
    let dot11 = v1.dot(v1);
    let dot12 = v1.dot(v2);
    let denom = dot00 * dot11 - dot01 * dot01;
    if denom.abs() < f32::EPSILON {
        return false;
    }
    let inv_denom = 1.0 / denom;
    let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
    let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;
    u >= -1e-5 && v >= -1e-5 && (u + v) <= 1.0 + 1e-5
}

fn line_segment_intersection(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> Option<Vec2> {
    let r = Vec2::new(a2.x - a1.x, a2.y - a1.y);
    let s = Vec2::new(b2.x - b1.x, b2.y - b1.y);

    let rxs = r.perp_dot(s);
    let q_p = Vec2::new(b1.x - a1.x, b1.y - a1.y);

    if rxs.abs() < f32::EPSILON {
        return None;
    }

    let t = q_p.perp_dot(s) / rxs;
    let u = q_p.perp_dot(r) / rxs;

    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(Vec2::new(a1.x + t * r.x, a1.y + t * r.y))
    } else {
        None
    }
}

/// Read-only (thread-safe) NavMesh structure exposing the same public methods as Polygon
#[derive(Debug, Clone)]
pub struct NavMesh {
    // 2D projection of triangles (x,z -> x,y), we use vertices[i].x, vertices[i].z from MeshData as 2D
    triangles2: Arc<[[Vec2; 3]]>,
    // For each triangle edge, optional neighbor triangle index (None means boundary edge)
    neighbors: Arc<[[Option<usize>; 3]]>,
    // Spatial hash grid for fast point queries: maps cell coordinates to list of triangle indices
    grid: Arc<HashMap<(i32, i32), Vec<usize>>>,
    // Cell size used by the spatial grid
    cell_size: f32,
}

impl NavMesh {
    pub fn empty() -> Self {
        NavMesh {
            triangles2: Arc::from(Vec::<[Vec2; 3]>::new().into_boxed_slice()),
            neighbors: Arc::from(Vec::<[Option<usize>; 3]>::new().into_boxed_slice()),
            grid: Arc::new(HashMap::new()),
            cell_size: 1.0,
        }
    }

    /// Build a NavMesh from MeshData triangles and create edge connectivity with delta 0.1.
    /// MeshData vertices are flattened [x,y,z,...], indices are triangles [i1,i2,i3,...].
    pub fn from_mesh_data(mesh: &MeshData) -> Self {
        // Build triangle arrays
        let verts = &mesh.vertices;
        let idxs = &mesh.indices;
        if verts.len() < 9 || idxs.len() < 3 {
            return NavMesh::empty();
        }
        let mut triangles3: Vec<[Vec3; 3]> = Vec::new();
        let mut triangles2: Vec<[Vec2; 3]> = Vec::new();
        for f in (0..idxs.len()).step_by(3) {
            let i0 = idxs[f] as usize;
            let i1 = idxs[f + 1] as usize;
            let i2 = idxs[f + 2] as usize;
            let v0 = Vec3::new(verts[3 * i0], verts[3 * i0 + 1], verts[3 * i0 + 2]);
            let v1 = Vec3::new(verts[3 * i1], verts[3 * i1 + 1], verts[3 * i1 + 2]);
            let v2 = Vec3::new(verts[3 * i2], verts[3 * i2 + 1], verts[3 * i2 + 2]);
            triangles3.push([v0, v1, v2]);
            // 2D projection: ignore height (y)
            triangles2.push([
                Vec2::new(v0.x, v0.z),
                Vec2::new(v1.x, v1.z),
                Vec2::new(v2.x, v2.z),
            ]);
        }

        // Build adjacency by quantizing edges with delta 0.1
        let delta = 0.1_f32;
        fn quantize(p: Vec2, d: f32) -> (i32, i32) {
            let qx = (p.x / d).round() as i32;
            let qy = (p.y / d).round() as i32;
            (qx, qy)
        }
        fn edge_key(a: Vec2, b: Vec2, d: f32) -> ((i32, i32), (i32, i32)) {
            let qa = quantize(a, d);
            let qb = quantize(b, d);
            if qa <= qb { (qa, qb) } else { (qb, qa) }
        }
        let mut map: HashMap<((i32, i32), (i32, i32)), Vec<(usize, usize)>> = HashMap::new();
        for (ti, tri) in triangles2.iter().enumerate() {
            let edges = [(0, 1), (1, 2), (2, 0)];
            for (ei, (a, b)) in edges.iter().enumerate() {
                let key = edge_key(tri[*a], tri[*b], delta);
                map.entry(key).or_default().push((ti, ei));
            }
        }
        let mut neighbors: Vec<[Option<usize>; 3]> = vec![[None, None, None]; triangles2.len()];
        for (_k, items) in map.into_iter() {
            if items.len() >= 2 {
                // connect first two
                let (t0, e0) = items[0];
                let (t1, e1) = items[1];
                neighbors[t0][e0] = Some(t1);
                neighbors[t1][e1] = Some(t0);
            }
        }

        // Compute world AABB (in XZ -> Vec2) to derive an optimal cell size
        let mut world_min_x = f32::INFINITY;
        let mut world_max_x = f32::NEG_INFINITY;
        let mut world_min_y = f32::INFINITY;
        let mut world_max_y = f32::NEG_INFINITY;
        for tri in triangles2.iter() {
            world_min_x = world_min_x.min(tri[0].x.min(tri[1].x).min(tri[2].x));
            world_max_x = world_max_x.max(tri[0].x.max(tri[1].x).max(tri[2].x));
            world_min_y = world_min_y.min(tri[0].y.min(tri[1].y).min(tri[2].y));
            world_max_y = world_max_y.max(tri[0].y.max(tri[1].y).max(tri[2].y));
        }
        let extent_x = (world_max_x - world_min_x).max(0.0);
        let extent_y = (world_max_y - world_min_y).max(0.0);
        let max_extent = extent_x.max(extent_y);

        // Decide grid resolution: 64x64 for small/medium meshes, 128x128 for larger ones
        // Heuristic: use triangle count as proxy for complexity
        let tri_count = triangles2.len();
        let target_cells_across: i32 = if tri_count > 8000 { 128 } else { 64 };

        // Derive cell size; clamp to a small epsilon to avoid division issues
        let mut cell_size: f32 = if max_extent > 1e-4 {
            (max_extent / target_cells_across as f32).max(1e-3)
        } else {
            1.0
        };

        // Build a simple spatial hash grid for fast point-in-triangle queries
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        let inv_cell = 1.0 / cell_size;
        for (ti, tri) in triangles2.iter().enumerate() {
            let mut min_x = tri[0].x.min(tri[1].x).min(tri[2].x);
            let mut max_x = tri[0].x.max(tri[1].x).max(tri[2].x);
            let mut min_y = tri[0].y.min(tri[1].y).min(tri[2].y);
            let mut max_y = tri[0].y.max(tri[1].y).max(tri[2].y);
            // Expand slightly to avoid precision misses on borders
            let eps = 1e-4;
            min_x -= eps;
            max_x += eps;
            min_y -= eps;
            max_y += eps;
            let min_cx = (min_x * inv_cell).floor() as i32;
            let max_cx = (max_x * inv_cell).floor() as i32;
            let min_cy = (min_y * inv_cell).floor() as i32;
            let max_cy = (max_y * inv_cell).floor() as i32;
            for cx in min_cx..=max_cx {
                for cy in min_cy..=max_cy {
                    grid.entry((cx, cy)).or_default().push(ti);
                }
            }
        }

        NavMesh {
            triangles2: triangles2.into(),
            neighbors: neighbors.into(),
            grid: Arc::new(grid),
            cell_size,
        }
    }

    /// Returns true if point is inside any triangle (2D projection, ignore height)
    pub fn contains_point(&self, point: Vec2) -> bool {
        if self.triangles2.is_empty() {
            return false;
        }
        // Use spatial grid for candidate triangles
        let cx = (point.x / self.cell_size).floor() as i32;
        let cy = (point.y / self.cell_size).floor() as i32;
        if let Some(list) = self.grid.get(&(cx, cy)) {
            for &i in list.iter() {
                let tri = &self.triangles2[i];
                if point_in_triangle_2d(point, tri[0], tri[1], tri[2]) {
                    return true;
                }
            }
            return false;
        }
        false
    }

    /// If the line crosses from the start triangle into non-connected space, return the first intersection point (2D).
    /// Otherwise, returns None.
    pub fn intersects_line(&self, start: Vec2, end: Vec2) -> Option<Vec2> {
        // Find containing triangle for start
        let mut current = match self.find_triangle_containing(start) {
            Some(i) => i,
            None => return None,
        };
        let mut cur_point = start;
        // Limit to number of triangles to avoid infinite loops
        for _ in 0..(self.triangles2.len() + 1) {
            let tri = &self.triangles2[current];
            // Find nearest intersection with any of the three edges
            let edges = [(0, 1, 0usize), (1, 2, 1usize), (2, 0, 2usize)];
            let mut nearest_t = f32::INFINITY;
            let mut hit_point: Option<Vec2> = None;
            let mut hit_edge_idx: usize = 0;
            for (a, b, eidx) in edges.iter() {
                if let Some((pt, t)) = segment_intersection_with_t(cur_point, end, tri[*a], tri[*b])
                {
                    // ignore intersections at t==0 (starting exactly at edge); require small epsilon
                    if t > 1e-5 && t < nearest_t {
                        nearest_t = t;
                        hit_point = Some(pt);
                        hit_edge_idx = *eidx;
                    }
                }
            }
            if let Some(p) = hit_point {
                match self.neighbors[current][hit_edge_idx] {
                    Some(next_tri) => {
                        // move slightly past the edge along direction and continue
                        let dir = (end - cur_point).normalize_or_zero();
                        cur_point = p + dir * 1e-4;
                        current = next_tri;
                        continue;
                    }
                    None => {
                        return Some(p);
                    }
                }
            } else {
                // No edge hit before reaching end; stays within mesh
                return None;
            }
        }
        None
    }

    fn find_triangle_containing(&self, p: Vec2) -> Option<usize> {
        if self.triangles2.is_empty() {
            return None;
        }
        let cx = (p.x / self.cell_size).floor() as i32;
        let cy = (p.y / self.cell_size).floor() as i32;
        if let Some(list) = self.grid.get(&(cx, cy)) {
            for &i in list.iter() {
                let tri = &self.triangles2[i];
                if point_in_triangle_2d(p, tri[0], tri[1], tri[2]) {
                    return Some(i);
                }
            }
            return None;
        }
        None
    }
}

// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Look-up tables (`.cube` files: see [`crate::cube_table`]) on their way to the card.
//!
//! **A frame names a table by its [`TableId`] and does not carry it** (as it names a picture): the table is *published* once, by whoever has it (the window, from the
//! resources of the project; the export, from the same), and the engine takes it from the shelf when a frame needs it and the card does not have it. The shelf holds a
//! table only by a weak reference, so that a table that nobody has any more (its project is closed, its resource is gone) is gone, and a frame that names it is made without
//! its effect, which is counted as an effect that was not run, as one the engine does not have.
//!
//! On the card a 3-D table is a 3-D texture of `size` points a side and a 1-D table is a texture 256 points wide (the point `n` at the column `n % 256` of the row
//! `n / 256`), both of four 32-bit floats a point, which `textureLoad` reads as they are (no filtering: what is looked up between the points is the pass's own arithmetic).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

use super::TableId;
use crate::cube_table::{CubeShape, CubeTable};

/// The tables that have been published, by serial number.
static PUBLISHED: LazyLock<Mutex<HashMap<u64, Weak<CubeTable>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// How many tables there may be on the shelf before the ones that nobody has any more are swept off it.
const SWEEP_WHEN_THERE_ARE_MORE: usize = 64;

/// How wide the texture of a 1-D table is.
pub const WIDTH_OF_A_1D_TABLE_ON_THE_CARD: u32 = 256;

/// How many tables the card keeps at most (the least recently used go first).
pub const MOST_TABLES_ON_THE_CARD: usize = 16;

/// Puts `table` on the shelf for the engine to take, and gives the id that frames name it by (its serial number: the same table is the same id, always).
pub fn publish_table(table: &Arc<CubeTable>) -> TableId {
    let mut shelf = PUBLISHED.lock().unwrap_or_else(PoisonError::into_inner);
    put_on_the_shelf(&mut shelf, table)
}

/// Puts `table` on `shelf`, after sweeping off it the tables that nobody has any more if there are [`SWEEP_WHEN_THERE_ARE_MORE`] of them or more.
fn put_on_the_shelf(shelf: &mut HashMap<u64, Weak<CubeTable>>, table: &Arc<CubeTable>) -> TableId {
    let id = table.serial_number();
    if shelf.len() >= SWEEP_WHEN_THERE_ARE_MORE {
        shelf.retain(|_, weak| weak.strong_count() > 0);
    }
    shelf.insert(id, Arc::downgrade(table));
    TableId(id)
}

/// The table published under `id`, if somebody still has it.
pub fn published_table(id: TableId) -> Option<Arc<CubeTable>> {
    PUBLISHED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&id.0)
        .and_then(Weak::upgrade)
}

/// A table as the card has it.
pub(super) struct TableOnTheCard {
    /// The 3-D texture of a 3-D table (a 1×1×1 texture for a 1-D table, which the pass does not read).
    pub cube: wgpu::TextureView,
    /// The strip of a 1-D table (a 1×1 texture for a 3-D table, which the pass does not read).
    pub strip: wgpu::TextureView,
    pub shape: CubeShape,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    /// Kept so that the textures live as long as the views.
    _textures: [wgpu::Texture; 2],
}

/// The format of the textures of tables.
const TABLE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// A texture of 1 point (what the pass is given for the kind of table that the call does not use).
fn a_texture_of_one_point(device: &wgpu::Device, dimension: wgpu::TextureDimension) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("engine table that is not used"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension,
        format: TABLE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// Puts `table` on the card.
pub(super) fn put_on_the_card(device: &wgpu::Device, queue: &wgpu::Queue, table: &CubeTable) -> TableOnTheCard {
    let as_bytes = |floats: &[f32]| -> Vec<u8> { floats.iter().flat_map(|number| number.to_le_bytes()).collect() };
    let (cube, strip) = match table.shape() {
        CubeShape::ThreeDimensional { points_a_side } => {
            let side = points_a_side as u32;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("engine 3-D table"),
                size: wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: side,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: TABLE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let floats: Vec<f32> = table.rows().iter().flat_map(|[r, g, b]| [*r, *g, *b, 1.0]).collect();
            queue.write_texture(
                texture.as_image_copy(),
                &as_bytes(&floats),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 16),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: side,
                },
            );
            let unused = a_texture_of_one_point(device, wgpu::TextureDimension::D2);
            (texture, unused)
        }
        CubeShape::OneDimensional { points } => {
            let width = WIDTH_OF_A_1D_TABLE_ON_THE_CARD;
            let height = (points as u32).div_ceil(width);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("engine 1-D table"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TABLE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let mut floats: Vec<f32> = table.rows().iter().flat_map(|[r, g, b]| [*r, *g, *b, 1.0]).collect();
            floats.resize((width * height * 4) as usize, 0.0);
            queue.write_texture(
                texture.as_image_copy(),
                &as_bytes(&floats),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 16),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            let unused = a_texture_of_one_point(device, wgpu::TextureDimension::D3);
            (unused, texture)
        }
    };
    // (The textures are in the order of the bindings: the cube, then the strip.)
    let view_of = |texture: &wgpu::Texture, dimension| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(dimension),
            ..Default::default()
        })
    };
    let cube_view = view_of(&cube, wgpu::TextureViewDimension::D3);
    let strip_view = view_of(&strip, wgpu::TextureViewDimension::D2);
    TableOnTheCard {
        cube: cube_view,
        strip: strip_view,
        shape: table.shape(),
        domain_min: table.domain_min(),
        domain_max: table.domain_max(),
        _textures: [cube, strip],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube_table::{CubeShape, CubeTable};

    fn a_table() -> Arc<CubeTable> {
        Arc::new(
            CubeTable::new(
                "x",
                [0.0; 3],
                [1.0; 3],
                CubeShape::OneDimensional { points: 2 },
                vec![[0.0; 3], [1.0; 3]],
            )
            .unwrap(),
        )
    }

    #[test]
    fn the_shelf_is_swept_of_the_tables_nobody_has_when_it_has_sixty_four_and_not_before() {
        assert_eq!(SWEEP_WHEN_THERE_ARE_MORE, 64);
        assert_eq!(MOST_TABLES_ON_THE_CARD, 16);
        let kept = a_table();
        let dead_entries = |how_many: u64| -> HashMap<u64, Weak<CubeTable>> {
            (0..how_many)
                .map(|number| (u64::MAX - number, Arc::downgrade(&a_table())))
                .collect()
        };
        let mut shelf = dead_entries(63);
        put_on_the_shelf(&mut shelf, &kept);
        assert_eq!(shelf.len(), 64, "(63 and the new one: nothing is swept yet)");
        let mut shelf = dead_entries(64);
        put_on_the_shelf(&mut shelf, &kept);
        assert_eq!(shelf.len(), 1, "(64 were there: the dead ones went)");
        // (And one somebody has stays.)
        let alive = a_table();
        let mut shelf = dead_entries(70);
        shelf.insert(alive.serial_number(), Arc::downgrade(&alive));
        put_on_the_shelf(&mut shelf, &kept);
        assert_eq!(shelf.len(), 2);
        assert!(shelf.contains_key(&alive.serial_number()));
    }

    #[test]
    fn a_published_table_is_found_by_its_id_while_somebody_has_it_and_not_after() {
        let table = a_table();
        let id = publish_table(&table);

        assert_eq!(id, TableId(table.serial_number()));
        assert_eq!(published_table(id).map(|found| found.serial_number()), Some(id.0));

        drop(table);
        assert!(published_table(id).is_none(), "the shelf does not keep a table alive");
        assert!(published_table(TableId(u64::MAX)).is_none());
    }

    #[test]
    fn publishing_a_table_again_gives_the_same_id_and_a_crowded_shelf_is_swept_of_the_tables_nobody_has() {
        let table = a_table();
        assert_eq!(publish_table(&table), publish_table(&table));
        let kept: Vec<Arc<CubeTable>> = (0..SWEEP_WHEN_THERE_ARE_MORE * 2)
            .map(|_| {
                let table = a_table();
                publish_table(&table);
                table
            })
            .collect();
        let dropped = a_table();
        let dropped_id = publish_table(&dropped);
        drop(dropped);
        // Publishing more sweeps the dead ones off.
        let another = a_table();
        publish_table(&another);
        assert!(published_table(dropped_id).is_none());
        assert!(
            kept.iter()
                .all(|table| published_table(TableId(table.serial_number())).is_some())
        );
        assert!(published_table(TableId(table.serial_number())).is_some());
    }
}

use crate::{images, models::*};
use arboard::Clipboard;
use chrono::Utc;
use image::{imageops::FilterType, Rgba, RgbaImage};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const DEFAULT_BACKGROUND: &str = "#15191f";
const MAX_EXPORT_SIDE: u32 = 32_768;
const MAX_EXPORT_PIXELS: u64 = 200_000_000;

pub fn list(connection: &Connection) -> Result<Vec<ReferenceBoardSummary>, String> {
    let mut statement = connection.prepare(
        "SELECT b.id,b.name,b.background,COUNT(CASE WHEN i.deleted_at IS NULL THEN i.id END),b.updated_at,b.last_opened_at
         FROM reference_boards b LEFT JOIN reference_board_items i ON i.board_id=b.id
         GROUP BY b.id ORDER BY COALESCE(b.last_opened_at,b.updated_at) DESC",
    ).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(ReferenceBoardSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                background: row.get(2)?,
                item_count: row.get(3)?,
                updated_at: row.get(4)?,
                last_opened_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn purge_deleted(connection: &mut Connection, base_dir: &Path) -> Result<(), String> {
    let cutoff = (Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let paths = {
        let mut statement = connection.prepare(
            "SELECT original_rel_path,thumbnail_rel_path FROM reference_board_items WHERE deleted_at IS NOT NULL AND deleted_at<?1",
        ).map_err(|e| e.to_string())?;
        let collected = statement
            .query_map([&cutoff], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        collected
    };
    connection
        .execute(
            "DELETE FROM reference_board_items WHERE deleted_at IS NOT NULL AND deleted_at<?1",
            [&cutoff],
        )
        .map_err(|e| e.to_string())?;
    for (original, thumbnail) in paths {
        images::remove_managed_file(base_dir, &original);
        images::remove_managed_file(base_dir, &thumbnail);
    }
    Ok(())
}

pub fn create(connection: &Connection, name: &str) -> Result<ReferenceBoardDetail, String> {
    let name = validated_name(name)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO reference_boards(id,name,background,created_at,updated_at,last_opened_at) VALUES(?1,?2,?3,?4,?4,?4)",
        params![id, name, DEFAULT_BACKGROUND, now],
    ).map_err(|e| format!("创建参考板失败：{e}"))?;
    get(connection, &id)
}

pub fn rename(connection: &Connection, id: &str, name: &str) -> Result<(), String> {
    let name = validated_name(name)?;
    let changed = connection
        .execute(
            "UPDATE reference_boards SET name=?1,updated_at=?2 WHERE id=?3",
            params![name, Utc::now().to_rfc3339(), id],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("参考板不存在".into());
    }
    Ok(())
}

pub fn get(connection: &Connection, id: &str) -> Result<ReferenceBoardDetail, String> {
    Uuid::parse_str(id).map_err(|_| "参考板 ID 无效".to_string())?;
    let mut board = connection.query_row(
        "SELECT id,name,background,view_x,view_y,view_scale,created_at,updated_at FROM reference_boards WHERE id=?1",
        [id],
        |row| Ok(ReferenceBoardDetail {
            id: row.get(0)?, name: row.get(1)?, background: row.get(2)?, view_x: row.get(3)?,
            view_y: row.get(4)?, view_scale: row.get(5)?, created_at: row.get(6)?, updated_at: row.get(7)?, items: Vec::new(),
        }),
    ).optional().map_err(|e| e.to_string())?.ok_or("参考板不存在")?;
    let mut statement = connection.prepare(
        "SELECT id,board_id,original_name,pixel_width,pixel_height,x,y,width,height,rotation,z_index,source_asset_id,source_image_id
         FROM reference_board_items WHERE board_id=?1 AND deleted_at IS NULL ORDER BY z_index,id",
    ).map_err(|e| e.to_string())?;
    board.items = statement
        .query_map([id], row_to_item)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    connection
        .execute(
            "UPDATE reference_boards SET last_opened_at=?1 WHERE id=?2",
            params![Utc::now().to_rfc3339(), id],
        )
        .map_err(|e| e.to_string())?;
    Ok(board)
}

pub fn delete(connection: &mut Connection, base_dir: &Path, id: &str) -> Result<(), String> {
    ensure_board(connection, id)?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM reference_boards WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    let directory = base_dir.join("reference-boards").join(id);
    if directory.exists() {
        fs::remove_dir_all(directory).map_err(|e| format!("清理参考板图片失败：{e}"))?;
    }
    Ok(())
}

pub fn duplicate(
    connection: &mut Connection,
    base_dir: &Path,
    id: &str,
) -> Result<ReferenceBoardDetail, String> {
    let source = get(connection, id)?;
    let target_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let target_name = unique_copy_name(connection, &source.name)?;
    connection.execute(
        "INSERT INTO reference_boards(id,name,background,view_x,view_y,view_scale,created_at,updated_at,last_opened_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,?7)",
        params![target_id,target_name,source.background,source.view_x,source.view_y,source.view_scale,now],
    ).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        for item in &source.items {
            let path: String = connection
                .query_row(
                    "SELECT original_rel_path FROM reference_board_items WHERE id=?1",
                    [&item.id],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let source_path = images::safe_join(base_dir, &path)?;
            let stored = images::import_image_to(
                base_dir,
                &source_path,
                &format!("reference-boards/{target_id}"),
            )?;
            insert_stored_item(
                connection,
                &target_id,
                &stored,
                item.x,
                item.y,
                item.width,
                item.height,
                item.rotation,
                item.z_index,
                item.source_asset_id.as_deref(),
                item.source_image_id.as_deref(),
            )?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = connection.execute("DELETE FROM reference_boards WHERE id=?1", [&target_id]);
        let _ = fs::remove_dir_all(base_dir.join("reference-boards").join(&target_id));
        return Err(error);
    }
    get(connection, &target_id)
}

pub fn add_asset_images(
    connection: &mut Connection,
    base_dir: &Path,
    board_id: &str,
    image_ids: &[String],
    placement: &ReferencePlacement,
) -> Result<ReferenceImageAddReport, String> {
    ensure_board(connection, board_id)?;
    if image_ids.is_empty() {
        return Ok(empty_report());
    }
    if image_ids.len() > 5_000 {
        return Err("一次最多添加 5000 张图片".into());
    }
    let mut inputs = Vec::new();
    let mut skipped = 0;
    for image_id in image_ids {
        let source = connection
            .query_row(
                "SELECT asset_id,original_name,original_rel_path FROM images WHERE id=?1",
                [image_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((asset_id, original_name, relative)) = source {
            inputs.push((
                images::safe_join(base_dir, &relative)?,
                Some(asset_id),
                Some(image_id.clone()),
                Some(original_name),
            ));
        } else {
            skipped += 1;
        }
    }
    add_paths_internal(connection, base_dir, board_id, inputs, placement, skipped)
}

pub fn add_media_images(
    connection: &mut Connection,
    base_dir: &Path,
    board_id: &str,
    entry_ids: &[String],
    placement: &ReferencePlacement,
) -> Result<ReferenceImageAddReport, String> {
    ensure_board(connection, board_id)?;
    let mut inputs = Vec::new();
    let mut skipped = 0;
    for entry_id in entry_ids {
        let source=connection.query_row("SELECT f.rel_path,f.original_name FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id WHERE e.id=?1 AND e.kind='image' AND e.deleted_at IS NULL",[entry_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).optional().map_err(|e|e.to_string())?;
        if let Some((relative, name)) = source {
            inputs.push((
                images::safe_join(base_dir, &relative)?,
                None,
                None,
                Some(name),
            ));
        } else {
            skipped += 1;
        }
    }
    add_paths_internal(connection, base_dir, board_id, inputs, placement, skipped)
}

pub fn import_paths(
    connection: &mut Connection,
    base_dir: &Path,
    board_id: &str,
    paths: &[String],
    placement: &ReferencePlacement,
) -> Result<ReferenceImageAddReport, String> {
    ensure_board(connection, board_id)?;
    if paths.len() > 5_000 {
        return Err("一次最多添加 5000 张图片".into());
    }
    let inputs = paths
        .iter()
        .map(|path| (PathBuf::from(path), None, None, None))
        .collect();
    add_paths_internal(connection, base_dir, board_id, inputs, placement, 0)
}

pub fn paste_clipboard(
    connection: &mut Connection,
    base_dir: &Path,
    board_id: &str,
    placement: &ReferencePlacement,
) -> Result<ReferenceBoardItem, String> {
    ensure_board(connection, board_id)?;
    let mut clipboard = Clipboard::new().map_err(|e| format!("读取剪贴板失败：{e}"))?;
    let image = clipboard.get_image().map_err(|_| "剪贴板中没有图片")?;
    let width = u32::try_from(image.width).map_err(|_| "剪贴板图片过宽")?;
    let height = u32::try_from(image.height).map_err(|_| "剪贴板图片过高")?;
    let stored = images::import_rgba_to(
        base_dir,
        &format!("reference-boards/{board_id}"),
        width,
        height,
        image.bytes.as_ref(),
    )?;
    let (display_width, display_height) = initial_size(stored.pixel_width, stored.pixel_height);
    let z = next_z(connection, board_id)?;
    if let Err(error) = insert_stored_item(
        connection,
        board_id,
        &stored,
        placement.x,
        placement.y,
        display_width,
        display_height,
        0.0,
        z,
        None,
        None,
    ) {
        cleanup_stored(base_dir, &stored);
        return Err(error);
    }
    item_by_id(connection, &stored.id)
}

pub fn duplicate_items(
    connection: &mut Connection,
    base_dir: &Path,
    board_id: &str,
    item_ids: &[String],
) -> Result<Vec<ReferenceBoardItem>, String> {
    ensure_board(connection, board_id)?;
    if item_ids.len() > 5_000 {
        return Err("一次最多复制 5000 张图片".into());
    }
    let mut staged = Vec::new();
    let start_z = next_z(connection, board_id)?;
    for id in item_ids {
        let staged_item = (|| -> Result<(images::StoredImage, ReferenceBoardItem), String> {
            let item = item_by_id(connection, id)?;
            if item.board_id != board_id {
                return Err("参考图不属于当前参考板".into());
            }
            let path: String = connection
                .query_row(
                    "SELECT original_rel_path FROM reference_board_items WHERE id=?1",
                    [id],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let source = images::safe_join(base_dir, &path)?;
            let stored = images::import_image_to(
                base_dir,
                &source,
                &format!("reference-boards/{board_id}"),
            )?;
            Ok((stored, item))
        })();
        match staged_item {
            Ok(item) => staged.push(item),
            Err(error) => {
                for (stored, _) in &staged {
                    cleanup_stored(base_dir, stored);
                }
                return Err(error);
            }
        }
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let result = (|| -> Result<(), String> {
        for (index, (stored, item)) in staged.iter().enumerate() {
            transaction.execute(
                "INSERT INTO reference_board_items(id,board_id,original_name,original_rel_path,thumbnail_rel_path,pixel_width,pixel_height,x,y,width,height,rotation,z_index,source_asset_id,source_image_id,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?16)",
                params![stored.id,board_id,stored.original_name,stored.original_rel_path,stored.thumbnail_rel_path,stored.pixel_width,stored.pixel_height,item.x+28.0,item.y+28.0,item.width,item.height,item.rotation,start_z+index as i64,item.source_asset_id,item.source_image_id,now],
            ).map_err(|e| e.to_string())?;
        }
        transaction
            .execute(
                "UPDATE reference_boards SET updated_at=?1 WHERE id=?2",
                params![now, board_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(transaction);
        for (stored, _) in &staged {
            cleanup_stored(base_dir, stored);
        }
        return Err(error);
    }
    if let Err(error) = transaction.commit() {
        for (stored, _) in &staged {
            cleanup_stored(base_dir, stored);
        }
        return Err(error.to_string());
    }
    staged
        .iter()
        .map(|(stored, _)| item_by_id(connection, &stored.id))
        .collect()
}

pub fn save_changes(
    connection: &mut Connection,
    base_dir: &Path,
    changes: ReferenceBoardChanges,
) -> Result<(), String> {
    ensure_board(connection, &changes.board_id)?;
    if changes.items.len() > 10_000
        || changes.deleted_ids.len() > 10_000
        || changes.restored_ids.len() > 10_000
    {
        return Err("一次保存的参考图数量过多".into());
    }
    if let Some(scale) = changes.view_scale {
        if !scale.is_finite() || !(0.05..=8.0).contains(&scale) {
            return Err("画布缩放比例无效".into());
        }
    }
    if let Some(background) = changes.background.as_deref() {
        parse_background(background)?;
    }
    for item in &changes.items {
        validate_transform(item)?;
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for id in &changes.deleted_ids {
        transaction.execute("UPDATE reference_board_items SET deleted_at=?1,updated_at=?1 WHERE id=?2 AND board_id=?3", params![Utc::now().to_rfc3339(),id,changes.board_id]).map_err(|e| e.to_string())?;
    }
    for id in &changes.restored_ids {
        transaction.execute("UPDATE reference_board_items SET deleted_at=NULL,updated_at=?1 WHERE id=?2 AND board_id=?3", params![Utc::now().to_rfc3339(),id,changes.board_id]).map_err(|e| e.to_string())?;
    }
    for item in &changes.items {
        let changed = transaction.execute(
            "UPDATE reference_board_items SET x=?1,y=?2,width=?3,height=?4,rotation=?5,z_index=?6,updated_at=?7 WHERE id=?8 AND board_id=?9",
            params![item.x,item.y,item.width,item.height,item.rotation,item.z_index,Utc::now().to_rfc3339(),item.id,changes.board_id],
        ).map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err(format!("参考图不存在：{}", item.id));
        }
    }
    let now = Utc::now().to_rfc3339();
    if let Some(name) = changes.name {
        transaction
            .execute(
                "UPDATE reference_boards SET name=?1 WHERE id=?2",
                params![validated_name(&name)?, changes.board_id],
            )
            .map_err(|e| e.to_string())?;
    }
    if let Some(background) = changes.background {
        transaction
            .execute(
                "UPDATE reference_boards SET background=?1 WHERE id=?2",
                params![background, changes.board_id],
            )
            .map_err(|e| e.to_string())?;
    }
    if changes.view_x.is_some() || changes.view_y.is_some() || changes.view_scale.is_some() {
        transaction.execute(
            "UPDATE reference_boards SET view_x=COALESCE(?1,view_x),view_y=COALESCE(?2,view_y),view_scale=COALESCE(?3,view_scale) WHERE id=?4",
            params![changes.view_x,changes.view_y,changes.view_scale,changes.board_id],
        ).map_err(|e| e.to_string())?;
    }
    transaction
        .execute(
            "UPDATE reference_boards SET updated_at=?1 WHERE id=?2",
            params![now, changes.board_id],
        )
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    let _ = base_dir;
    Ok(())
}

pub fn image_path(
    connection: &Connection,
    item_id: &str,
    thumbnail: bool,
) -> Result<String, String> {
    let column = if thumbnail {
        "thumbnail_rel_path"
    } else {
        "original_rel_path"
    };
    connection
        .query_row(
            &format!("SELECT {column} FROM reference_board_items WHERE id=?1"),
            [item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "参考图不存在".into())
}

pub fn export_png(
    connection: &Connection,
    base_dir: &Path,
    board_id: &str,
    destination: &Path,
    options: &ReferenceExportOptions,
) -> Result<(), String> {
    let board = get(connection, board_id)?;
    if board.items.is_empty() {
        return Err("参考板中没有图片".into());
    }
    let scale = if options.scale.is_finite() {
        options.scale.clamp(0.1, 4.0)
    } else {
        1.0
    };
    let bounds = board_bounds(&board.items).ok_or("无法计算参考板范围")?;
    let width = ((bounds.2 - bounds.0) * scale).ceil().max(1.0) as u32;
    let height = ((bounds.3 - bounds.1) * scale).ceil().max(1.0) as u32;
    if width > MAX_EXPORT_SIDE
        || height > MAX_EXPORT_SIDE
        || u64::from(width) * u64::from(height) > MAX_EXPORT_PIXELS
    {
        return Err("导出尺寸超过 32768px 或 2 亿像素，请降低导出倍率".into());
    }
    let color = parse_background(&board.background)?;
    let mut output = RgbaImage::from_pixel(width, height, color);
    for item in &board.items {
        let path = image_path(connection, &item.id, false)?;
        let source = image::open(images::safe_join(base_dir, &path)?)
            .map_err(|e| format!("读取参考图失败：{e}"))?
            .to_rgba8();
        composite_item(&mut output, &source, item, bounds.0, bounds.1, scale);
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temporary = destination.with_extension("png.tmp");
    output
        .save_with_format(&temporary, image::ImageFormat::Png)
        .map_err(|e| format!("导出 PNG 失败：{e}"))?;
    if destination.exists() {
        fs::remove_file(destination).map_err(|e| e.to_string())?;
    }
    fs::rename(temporary, destination).map_err(|e| format!("保存 PNG 失败：{e}"))
}

fn add_paths_internal(
    connection: &mut Connection,
    base_dir: &Path,
    board_id: &str,
    inputs: Vec<(PathBuf, Option<String>, Option<String>, Option<String>)>,
    placement: &ReferencePlacement,
    mut skipped: usize,
) -> Result<ReferenceImageAddReport, String> {
    let mut staged = Vec::new();
    let mut warnings = Vec::new();
    for (index, (path, asset_id, image_id, original_name)) in inputs.into_iter().enumerate() {
        match images::import_image_to(base_dir, &path, &format!("reference-boards/{board_id}")) {
            Ok(mut stored) => {
                if let Some(name) = original_name {
                    stored.original_name = name;
                }
                staged.push((index, stored, asset_id, image_id));
            }
            Err(error) => {
                skipped += 1;
                warnings.push(format!("{}：{error}", path.display()));
            }
        }
    }
    let start_z = next_z(connection, board_id)?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let result = (|| -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        for (position, (_input_index, stored, asset_id, image_id)) in staged.iter().enumerate() {
            let (width, height) = initial_size(stored.pixel_width, stored.pixel_height);
            let offset = position as f64 * 28.0;
            transaction.execute(
                "INSERT INTO reference_board_items(id,board_id,original_name,original_rel_path,thumbnail_rel_path,pixel_width,pixel_height,x,y,width,height,rotation,z_index,source_asset_id,source_image_id,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0,?12,?13,?14,?15,?15)",
                params![stored.id,board_id,stored.original_name,stored.original_rel_path,stored.thumbnail_rel_path,stored.pixel_width,stored.pixel_height,placement.x+offset,placement.y+offset,width,height,start_z+position as i64,asset_id,image_id,now],
            ).map_err(|e| e.to_string())?;
            ids.push(stored.id.clone());
        }
        transaction
            .execute(
                "UPDATE reference_boards SET updated_at=?1 WHERE id=?2",
                params![now, board_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(ids)
    })();
    match result {
        Ok(ids) => {
            transaction.commit().map_err(|e| e.to_string())?;
            let items = ids
                .iter()
                .map(|id| item_by_id(connection, id))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ReferenceImageAddReport {
                items,
                skipped,
                warnings,
            })
        }
        Err(error) => {
            drop(transaction);
            for (_, stored, _, _) in &staged {
                cleanup_stored(base_dir, stored);
            }
            Err(error)
        }
    }
}

fn insert_stored_item(
    connection: &Connection,
    board_id: &str,
    stored: &images::StoredImage,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rotation: f64,
    z_index: i64,
    source_asset_id: Option<&str>,
    source_image_id: Option<&str>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO reference_board_items(id,board_id,original_name,original_rel_path,thumbnail_rel_path,pixel_width,pixel_height,x,y,width,height,rotation,z_index,source_asset_id,source_image_id,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?16)",
        params![stored.id,board_id,stored.original_name,stored.original_rel_path,stored.thumbnail_rel_path,stored.pixel_width,stored.pixel_height,x,y,width,height,rotation,z_index,source_asset_id,source_image_id,now],
    ).map_err(|e| e.to_string())?;
    touch_board(connection, board_id)
}

fn row_to_item(row: &Row<'_>) -> rusqlite::Result<ReferenceBoardItem> {
    Ok(ReferenceBoardItem {
        id: row.get(0)?,
        board_id: row.get(1)?,
        original_name: row.get(2)?,
        pixel_width: row.get(3)?,
        pixel_height: row.get(4)?,
        x: row.get(5)?,
        y: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        rotation: row.get(9)?,
        z_index: row.get(10)?,
        source_asset_id: row.get(11)?,
        source_image_id: row.get(12)?,
    })
}

fn item_by_id(connection: &Connection, id: &str) -> Result<ReferenceBoardItem, String> {
    connection.query_row(
        "SELECT id,board_id,original_name,pixel_width,pixel_height,x,y,width,height,rotation,z_index,source_asset_id,source_image_id FROM reference_board_items WHERE id=?1",
        [id], row_to_item,
    ).optional().map_err(|e| e.to_string())?.ok_or_else(|| "参考图不存在".into())
}

fn ensure_board(connection: &Connection, id: &str) -> Result<(), String> {
    Uuid::parse_str(id).map_err(|_| "参考板 ID 无效".to_string())?;
    let exists = connection
        .query_row("SELECT 1 FROM reference_boards WHERE id=?1", [id], |_| {
            Ok(())
        })
        .optional()
        .map_err(|e| e.to_string())?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("参考板不存在".into())
    }
}
fn touch_board(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute(
            "UPDATE reference_boards SET updated_at=?1 WHERE id=?2",
            params![Utc::now().to_rfc3339(), id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn next_z(connection: &Connection, board_id: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(z_index),-1)+1 FROM reference_board_items WHERE board_id=?1",
            [board_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())
}
fn validated_name(name: &str) -> Result<String, String> {
    let value = name.trim();
    if value.is_empty() {
        Err("参考板名称不能为空".into())
    } else if value.chars().count() > 80 {
        Err("参考板名称不能超过 80 个字符".into())
    } else {
        Ok(value.into())
    }
}
fn unique_copy_name(connection: &Connection, source: &str) -> Result<String, String> {
    for index in 1..10_000 {
        let candidate = if index == 1 {
            format!("{source} - 副本")
        } else {
            format!("{source} - 副本 {index}")
        };
        let exists = connection
            .query_row(
                "SELECT 1 FROM reference_boards WHERE name=?1",
                [&candidate],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_some();
        if !exists {
            return Ok(candidate);
        }
    }
    Err("无法生成参考板副本名称".into())
}
fn initial_size(width: u32, height: u32) -> (f64, f64) {
    let max = width.max(height) as f64;
    let scale = (360.0 / max).min(1.0);
    (width as f64 * scale, height as f64 * scale)
}
fn cleanup_stored(base_dir: &Path, stored: &images::StoredImage) {
    images::remove_managed_file(base_dir, &stored.original_rel_path);
    images::remove_managed_file(base_dir, &stored.thumbnail_rel_path);
}
fn empty_report() -> ReferenceImageAddReport {
    ReferenceImageAddReport {
        items: Vec::new(),
        skipped: 0,
        warnings: Vec::new(),
    }
}
fn validate_transform(item: &ReferenceBoardItemTransform) -> Result<(), String> {
    let values = [item.x, item.y, item.width, item.height, item.rotation];
    if values.iter().any(|v| !v.is_finite())
        || item.width < 1.0
        || item.height < 1.0
        || item.width > 1_000_000.0
        || item.height > 1_000_000.0
    {
        return Err("参考图变换数据无效".into());
    }
    Ok(())
}

fn rotated_corners(item: &ReferenceBoardItem) -> [(f64, f64); 4] {
    let cx = item.x + item.width / 2.0;
    let cy = item.y + item.height / 2.0;
    let angle = item.rotation.to_radians();
    let (sin, cos) = angle.sin_cos();
    let rotate = |x: f64, y: f64| {
        let dx = x - cx;
        let dy = y - cy;
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    };
    [
        rotate(item.x, item.y),
        rotate(item.x + item.width, item.y),
        rotate(item.x + item.width, item.y + item.height),
        rotate(item.x, item.y + item.height),
    ]
}
fn board_bounds(items: &[ReferenceBoardItem]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for item in items {
        for (x, y) in rotated_corners(item) {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x.is_finite() {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}
fn parse_background(value: &str) -> Result<Rgba<u8>, String> {
    let value = value.trim().strip_prefix('#').ok_or("背景颜色无效")?;
    if value.len() != 6 {
        return Err("背景颜色无效".into());
    }
    let parse = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&value[range], 16).map_err(|_| "背景颜色无效".to_string())
    };
    Ok(Rgba([parse(0..2)?, parse(2..4)?, parse(4..6)?, 255]))
}

fn composite_item(
    output: &mut RgbaImage,
    source: &RgbaImage,
    item: &ReferenceBoardItem,
    min_x: f64,
    min_y: f64,
    scale: f64,
) {
    let width = (item.width * scale).round().max(1.0) as u32;
    let height = (item.height * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(source, width, height, FilterType::Lanczos3);
    let cx = (item.x + item.width / 2.0 - min_x) * scale;
    let cy = (item.y + item.height / 2.0 - min_y) * scale;
    let angle = item.rotation.to_radians();
    let (sin, cos) = angle.sin_cos();
    let half_w = width as f64 / 2.0;
    let half_h = height as f64 / 2.0;
    let extent_x = half_w * cos.abs() + half_h * sin.abs();
    let extent_y = half_w * sin.abs() + half_h * cos.abs();
    let left = (cx - extent_x).floor().max(0.0) as u32;
    let top = (cy - extent_y).floor().max(0.0) as u32;
    let right = (cx + extent_x).ceil().min(output.width() as f64) as u32;
    let bottom = (cy + extent_y).ceil().min(output.height() as f64) as u32;
    for y in top..bottom {
        for x in left..right {
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let sx = dx * cos + dy * sin + half_w;
            let sy = -dx * sin + dy * cos + half_h;
            if sx >= 0.0 && sy >= 0.0 && sx < width as f64 && sy < height as f64 {
                let source_pixel = *resized.get_pixel(sx.floor() as u32, sy.floor() as u32);
                alpha_blend(output.get_pixel_mut(x, y), source_pixel);
            }
        }
    }
}
fn alpha_blend(target: &mut Rgba<u8>, source: Rgba<u8>) {
    let alpha = source[3] as f32 / 255.0;
    let inverse = 1.0 - alpha;
    for channel in 0..3 {
        target[channel] =
            (source[channel] as f32 * alpha + target[channel] as f32 * inverse).round() as u8;
    }
    target[3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn database() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("images/originals")).unwrap();
        fs::create_dir_all(dir.path().join("images/thumbnails")).unwrap();
        let connection = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        (dir, connection)
    }

    #[test]
    fn board_crud_and_transform_validation() {
        let (dir, mut connection) = database();
        let board = create(&connection, "参考板").unwrap();
        assert_eq!(list(&connection).unwrap()[0].item_count, 0);
        rename(&connection, &board.id, "角色参考").unwrap();
        let changes = ReferenceBoardChanges {
            board_id: board.id.clone(),
            name: None,
            background: Some("#ffffff".into()),
            view_x: Some(12.0),
            view_y: Some(4.0),
            view_scale: Some(2.0),
            items: vec![],
            deleted_ids: vec![],
            restored_ids: vec![],
        };
        save_changes(&mut connection, dir.path(), changes).unwrap();
        let loaded = get(&connection, &board.id).unwrap();
        assert_eq!(loaded.background, "#ffffff");
        assert_eq!(loaded.view_scale, 2.0);
        delete(&mut connection, dir.path(), &board.id).unwrap();
        assert!(list(&connection).unwrap().is_empty());
    }

    #[test]
    fn copied_asset_image_survives_asset_file_removal() {
        let (dir, mut connection) = database();
        let board = create(&connection, "参考板").unwrap();
        let source = dir.path().join("source.png");
        RgbaImage::from_pixel(20, 10, Rgba([255, 0, 0, 255]))
            .save(&source)
            .unwrap();
        let report = import_paths(
            &mut connection,
            dir.path(),
            &board.id,
            &[source.to_string_lossy().into()],
            &ReferencePlacement { x: 0.0, y: 0.0 },
        )
        .unwrap();
        fs::remove_file(source).unwrap();
        let relative = image_path(&connection, &report.items[0].id, false).unwrap();
        assert!(images::safe_join(dir.path(), &relative).unwrap().is_file());
    }

    #[test]
    fn exports_rotated_board_png() {
        let (dir, mut connection) = database();
        let board = create(&connection, "参考板").unwrap();
        let source = dir.path().join("source.png");
        RgbaImage::from_pixel(40, 30, Rgba([255, 0, 0, 255]))
            .save(&source)
            .unwrap();
        let report = import_paths(
            &mut connection,
            dir.path(),
            &board.id,
            &[source.to_string_lossy().into()],
            &ReferencePlacement { x: 0.0, y: 0.0 },
        )
        .unwrap();
        save_changes(
            &mut connection,
            dir.path(),
            ReferenceBoardChanges {
                board_id: board.id.clone(),
                name: None,
                background: None,
                view_x: None,
                view_y: None,
                view_scale: None,
                items: vec![ReferenceBoardItemTransform {
                    id: report.items[0].id.clone(),
                    x: 0.0,
                    y: 0.0,
                    width: 40.0,
                    height: 30.0,
                    rotation: 45.0,
                    z_index: 0,
                }],
                deleted_ids: vec![],
                restored_ids: vec![],
            },
        )
        .unwrap();
        let output = dir.path().join("board.png");
        export_png(
            &connection,
            dir.path(),
            &board.id,
            &output,
            &ReferenceExportOptions { scale: 1.0 },
        )
        .unwrap();
        assert!(output.is_file());
    }

    #[test]
    fn soft_delete_can_be_restored_and_board_delete_cleans_snapshots() {
        let (dir, mut connection) = database();
        let board = create(&connection, "可撤销删除").unwrap();
        let source = dir.path().join("source.png");
        RgbaImage::from_pixel(12, 12, Rgba([0, 120, 255, 255]))
            .save(&source)
            .unwrap();
        let report = import_paths(
            &mut connection,
            dir.path(),
            &board.id,
            &[source.to_string_lossy().into()],
            &ReferencePlacement { x: 0.0, y: 0.0 },
        )
        .unwrap();
        let item_id = report.items[0].id.clone();
        save_changes(
            &mut connection,
            dir.path(),
            ReferenceBoardChanges {
                board_id: board.id.clone(),
                name: None,
                background: None,
                view_x: None,
                view_y: None,
                view_scale: None,
                items: vec![],
                deleted_ids: vec![item_id.clone()],
                restored_ids: vec![],
            },
        )
        .unwrap();
        assert!(get(&connection, &board.id).unwrap().items.is_empty());
        save_changes(
            &mut connection,
            dir.path(),
            ReferenceBoardChanges {
                board_id: board.id.clone(),
                name: None,
                background: None,
                view_x: None,
                view_y: None,
                view_scale: None,
                items: vec![],
                deleted_ids: vec![],
                restored_ids: vec![item_id],
            },
        )
        .unwrap();
        assert_eq!(get(&connection, &board.id).unwrap().items.len(), 1);
        let snapshots = dir.path().join("reference-boards").join(&board.id);
        assert!(snapshots.is_dir());
        delete(&mut connection, dir.path(), &board.id).unwrap();
        assert!(!snapshots.exists());
    }

    #[test]
    fn duplicate_failure_cleans_staged_files_and_path_escape_is_rejected() {
        let (dir, mut connection) = database();
        let board = create(&connection, "事务复制").unwrap();
        let source = dir.path().join("source.png");
        RgbaImage::from_pixel(8, 8, Rgba([255, 180, 0, 255]))
            .save(&source)
            .unwrap();
        let report = import_paths(
            &mut connection,
            dir.path(),
            &board.id,
            &[source.to_string_lossy().into()],
            &ReferencePlacement { x: 0.0, y: 0.0 },
        )
        .unwrap();
        let before_files = walkdir::WalkDir::new(dir.path().join("reference-boards"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .count();
        assert!(duplicate_items(
            &mut connection,
            dir.path(),
            &board.id,
            &[report.items[0].id.clone(), "missing".into()]
        )
        .is_err());
        let after_files = walkdir::WalkDir::new(dir.path().join("reference-boards"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .count();
        assert_eq!(before_files, after_files);
        assert_eq!(get(&connection, &board.id).unwrap().items.len(), 1);
        assert!(images::safe_join(dir.path(), "../outside.png").is_err());
    }
}

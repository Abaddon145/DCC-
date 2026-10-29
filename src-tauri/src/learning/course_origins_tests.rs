use super::{course_origins::*, source_profiles};
use crate::state::test_state;
use rusqlite::params;

#[test]
fn learning_import_course_origins_series_preview_uses_existing_mapping() {
    use super::import_course_preview::preview_courses_from_profile;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source_path = temp.path().join("source.sqlite");
    std::fs::write(
        &source_path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let source = rusqlite::Connection::open(source_path).unwrap();
    state.with_library_mut(|db, _| {
        let series = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO learning_series(id,name,revision,created_at,updated_at) VALUES(?1,'目标系列',0,'now','now')", [&series]).unwrap();
        let first = preview_courses_from_profile(&source,db,&[],&profile.id)?;
        assert_eq!(first.series[0].origin,Some(CourseOriginState::Unmapped));
        let key = SeriesOriginKey::new(&profile.id,"series-1")?;
        let tx = db.transaction().unwrap();
        record_series_in(&tx,&key,&series,&first.series[0].source_series_hash,&profile.access_epoch)?;
        tx.commit().unwrap();
        let second = preview_courses_from_profile(&source,db,&[],&profile.id)?;
        assert_eq!(second.series[0].origin,Some(CourseOriginState::Unchanged(series.clone())));
        source.execute("UPDATE series SET title='来源已修改' WHERE id='series-1'", []).unwrap();
        let changed = preview_courses_from_profile(&source,db,&[],&profile.id)?;
        assert_eq!(changed.series[0].origin,Some(CourseOriginState::SourceChanged(series.clone())));
        db.execute("UPDATE learning_series SET name='目标已修改',revision=1 WHERE id=?1", [&series]).unwrap();
        let both = preview_courses_from_profile(&source,db,&[],&profile.id)?;
        assert_eq!(both.series[0].origin,Some(CourseOriginState::BothChanged(series)));
        Ok(())
    }).unwrap();
}

#[test]
fn learning_import_course_origins_series_mapping_preserves_changes_and_identity() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let key = SeriesOriginKey::new(&profile.id, "source-series|一").unwrap();
    let hash = "a".repeat(64);
    state.with_library_mut(|db, _| {
        let series = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO learning_series(id,name,revision,created_at,updated_at) VALUES(?1,'课程系列',0,'now','now')", [&series]).unwrap();
        assert_eq!(inspect_series(db,&key,&hash)?, CourseOriginState::Unmapped);
        assert!(record_series_in(db,&key,&series,&hash,&profile.access_epoch).is_err());
        let tx = db.transaction().unwrap();
        assert!(record_series_in(&tx,&key,&series,&hash,"wrong").is_err());
        assert_eq!(
            record_series_in(&tx,&key,&uuid::Uuid::new_v4().to_string(),&hash,&profile.access_epoch).unwrap_err(),
            "目标系列不可用"
        );
        record_series_in(&tx,&key,&series,&hash,&profile.access_epoch)?;
        record_series_in(&tx,&key,&series,&hash,&profile.access_epoch)?;
        tx.commit().unwrap();
        assert_eq!(inspect_series(db,&key,&hash)?, CourseOriginState::Unchanged(series.clone()));
        assert_eq!(inspect_series(db,&key,&"b".repeat(64))?, CourseOriginState::SourceChanged(series.clone()));
        db.execute("UPDATE learning_series SET name='修改后',revision=1 WHERE id=?1", [&series]).unwrap();
        assert_eq!(inspect_series(db,&key,&hash)?, CourseOriginState::TargetChanged(series.clone()));
        db.execute("DELETE FROM learning_series WHERE id=?1", [&series]).unwrap();
        assert_eq!(inspect_series(db,&key,&hash)?, CourseOriginState::TargetUnavailable(series));
        Ok(())
    }).unwrap();
}

#[test]
fn learning_import_course_origins_preview_reports_existing_mapping_and_changes() {
    use super::import_course_preview::{
        preview_courses_from_profile, recheck_preview_from_profile, CourseMediaBinding,
    };
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let other_profile = source_profiles::create(&state, &scope).unwrap();
    let source_path = temp.path().join("source.sqlite");
    std::fs::write(
        &source_path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let source = rusqlite::Connection::open(source_path).unwrap();
    state.with_library_mut(|db, _| {
        let media = uuid::Uuid::new_v4().to_string();
        let file = uuid::Uuid::new_v4().to_string();
        let course = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','video',?2,'now','now')", params![media,file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4','video.mp4','now')", params![file,media]).unwrap();
        db.execute("INSERT INTO learning_courses(id,media_entry_id,created_at,updated_at) VALUES(?1,?2,'now','now')", params![course,media]).unwrap();
        let bindings = [CourseMediaBinding { source_item_id: "course-1".into(), media_id: media.clone() }];
        let first = preview_courses_from_profile(&source,db,&bindings,&profile.id)?;
        let item = &first.series[0].items[0];
        assert_eq!(item.origin, Some(CourseOriginState::Unmapped));
        let key = CourseOriginKey::new(&profile.id,"course-1")?;
        let tx = db.transaction().unwrap();
        record_in(&tx,&key,&course,&item.source_item_hash,&profile.access_epoch)?;
        tx.commit().unwrap();
        let second = preview_courses_from_profile(&source,db,&bindings,&profile.id)?;
        assert_eq!(second.series[0].items[0].origin, Some(CourseOriginState::Unchanged(course.clone())));
        assert!(recheck_preview_from_profile(&source,db,&bindings,&profile.id,&second).is_ok());
        assert!(recheck_preview_from_profile(&source,db,&bindings,&other_profile.id,&second).is_err());
        db.execute("UPDATE learning_source_profiles SET access_epoch=?1 WHERE id=?2", params![uuid::Uuid::new_v4().to_string(),profile.id]).unwrap();
        assert!(recheck_preview_from_profile(&source,db,&bindings,&profile.id,&second).is_err());
        db.execute("UPDATE learning_source_profiles SET access_epoch=?1 WHERE id=?2", params![profile.access_epoch,profile.id]).unwrap();
        let other_media = uuid::Uuid::new_v4().to_string();
        let other_file = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','other',?2,'now','now')", params![other_media,other_file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','other.mp4','other.mp4','other.mp4','now')", params![other_file,other_media]).unwrap();
        let mismatch = [CourseMediaBinding { source_item_id:"course-1".into(), media_id:other_media }];
        assert!(preview_courses_from_profile(&source,db,&mismatch,&profile.id).is_err());
        source.execute("UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'", []).unwrap();
        let changed = preview_courses_from_profile(&source,db,&bindings,&profile.id)?;
        assert_eq!(changed.series[0].items[0].origin, Some(CourseOriginState::SourceChanged(course.clone())));
        db.execute("UPDATE learning_origin_map SET source_sha256=?1 WHERE target_id=?2 AND target_kind='course'", params!["b".repeat(64),course]).unwrap();
        assert!(recheck_preview_from_profile(&source,db,&bindings,&profile.id,&changed).is_err());
        db.execute("UPDATE learning_origin_map SET source_sha256=?1 WHERE target_id=?2 AND target_kind='course'", params![item.source_item_hash,course]).unwrap();
        db.execute("UPDATE learning_courses SET position_ms=100,progress_revision=1 WHERE id=?1", [&course]).unwrap();
        let both = preview_courses_from_profile(&source,db,&bindings,&profile.id)?;
        assert_eq!(both.series[0].items[0].origin, Some(CourseOriginState::BothChanged(course.clone())));
        assert!(preview_courses_from_profile(&source,db,&bindings,&uuid::Uuid::new_v4().to_string()).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn learning_import_course_origins_preserves_identity_and_detects_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source_a = CourseOriginKey::new(&profile.id, "lesson|一").unwrap();
    let source_b = CourseOriginKey::new(&profile.id, "lesson|二").unwrap();
    let hash = "a".repeat(64);
    let changed_hash = "b".repeat(64);
    state.with_library_mut(|db, _| {
        let media = uuid::Uuid::new_v4().to_string();
        let file = uuid::Uuid::new_v4().to_string();
        let course = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','video',?2,'now','now')", params![media,file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4','video.mp4','now')", params![file,media]).unwrap();
        db.execute("INSERT INTO learning_courses(id,media_entry_id,created_at,updated_at) VALUES(?1,?2,'now','now')", params![course,media]).unwrap();
        assert_eq!(inspect(db,&source_a,&hash)?, CourseOriginState::Unmapped);
        assert!(record_in(db,&source_a,&course,&hash,&profile.access_epoch).is_err());
        {
            let tx = db.transaction().unwrap();
            record_in(&tx,&source_a,&course,&hash,&profile.access_epoch)?;
            tx.rollback().unwrap();
        }
        assert_eq!(inspect(db,&source_a,&hash)?, CourseOriginState::Unmapped);
        {
            let tx = db.transaction().unwrap();
            assert!(record_in(&tx,&source_a,&course,&hash,"wrong-epoch").is_err());
            assert_eq!(
                record_in(&tx,&source_a,&uuid::Uuid::new_v4().to_string(),&hash,&profile.access_epoch).unwrap_err(),
                "目标课程不可用"
            );
            record_in(&tx,&source_a,&course,&hash,&profile.access_epoch)?;
            record_in(&tx,&source_a,&course,&hash,&profile.access_epoch)?;
            record_in(&tx,&source_b,&course,&hash,&profile.access_epoch)?;
            tx.commit().unwrap();
        }
        assert_eq!(inspect(db,&source_a,&hash)?, CourseOriginState::Unchanged(course.clone()));
        assert_eq!(inspect(db,&source_b,&hash)?, CourseOriginState::Unchanged(course.clone()));
        assert_eq!(inspect(db,&source_a,&changed_hash)?, CourseOriginState::SourceChanged(course.clone()));
        db.execute("UPDATE learning_courses SET position_ms=100,progress_revision=progress_revision+1 WHERE id=?1", [&course]).unwrap();
        assert_eq!(inspect(db,&source_a,&hash)?, CourseOriginState::TargetChanged(course.clone()));
        assert_eq!(inspect(db,&source_a,&changed_hash)?, CourseOriginState::BothChanged(course.clone()));
        db.execute("UPDATE media_entries SET deleted_at='now' WHERE id=?1", [&media]).unwrap();
        assert_eq!(inspect(db,&source_a,&hash)?, CourseOriginState::TargetUnavailable(course.clone()));
        Ok(())
    }).unwrap();
}

#[test]
fn learning_import_course_origins_backup_keeps_mapping_and_rotates_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let key = CourseOriginKey::new(&profile.id, "source-1").unwrap();
    let series_key = SeriesOriginKey::new(&profile.id, "source-1").unwrap();
    let hash = "a".repeat(64);
    let archive = temp.path().join("backup.dccassetlib");
    let (course,series) = state.with_library_mut(|db, base| {
        let media = uuid::Uuid::new_v4().to_string();
        let file = uuid::Uuid::new_v4().to_string();
        let course = uuid::Uuid::new_v4().to_string();
        let series = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','video',?2,'now','now')", params![media,file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4','video.mp4','now')", params![file,media]).unwrap();
        db.execute("INSERT INTO learning_courses(id,media_entry_id,created_at,updated_at) VALUES(?1,?2,'now','now')", params![course,media]).unwrap();
        db.execute("INSERT INTO learning_series(id,name,created_at,updated_at) VALUES(?1,'系列','now','now')", [&series]).unwrap();
        let tx = db.transaction().unwrap();
        record_in(&tx,&key,&course,&hash,&profile.access_epoch)?;
        record_series_in(&tx,&series_key,&series,&hash,&profile.access_epoch)?;
        tx.commit().unwrap();
        crate::backup::export_library(db,base,&archive)?;
        Ok((course,series))
    }).unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &archive))
        .unwrap();
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let restored = source_profiles::get(&state, &scope, &profile.id).unwrap();
    assert_ne!(restored.access_epoch, profile.access_epoch);
    state
        .with_library_mut(|db, _| {
            assert_eq!(
                inspect(db, &key, &hash)?,
                CourseOriginState::Unchanged(course.clone())
            );
            assert_eq!(
                inspect_series(db, &series_key, &hash)?,
                CourseOriginState::Unchanged(series.clone())
            );
            let tx = db.transaction().unwrap();
            assert!(record_in(&tx, &key, &course, &hash, &profile.access_epoch).is_err());
            record_in(&tx, &key, &course, &hash, &restored.access_epoch)?;
            assert!(
                record_series_in(&tx, &series_key, &series, &hash, &profile.access_epoch).is_err()
            );
            record_series_in(&tx, &series_key, &series, &hash, &restored.access_epoch)?;
            tx.commit().unwrap();
            Ok(())
        })
        .unwrap();
}
